//! Honeycomb owns desired bundle definitions; IAM accepts them with current manager authority.
use crate::{
    State,
    api::{Context, context, key, revision},
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    extract::{Path, State as S},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::BTreeSet;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    app_name: String,
    #[serde(default)]
    app_logo: Option<String>,
    app_ids: Vec<String>,
}
fn organization(id: &str) -> Result<&str> {
    if !id.split_once('>').is_some_and(|(org, bundle)| {
        honeycomb_core::valid_handle(org) && honeycomb_core::valid_handle(bundle)
    }) {
        return Err(Error::bad("Use an organization-qualified bundle ID"));
    }
    Ok(id.split_once('>').unwrap().0)
}
fn authorize(c: &Context, id: &str) -> Result<()> {
    c.admin(organization(id)?)?;
    if c.plane != "production" {
        return Err(Error::bad(
            "Bundle management currently supports production only; testing credentials cannot configure production bundles",
        ));
    }
    Ok(())
}
async fn validate(s: &State, id: &str, input: &Configuration) -> Result<()> {
    let org = organization(id)?;
    if input.app_name.trim().is_empty() || input.app_name.chars().count() > 200 {
        return Err(Error::bad("Bundle name must contain 1–200 characters"));
    }
    if input.app_ids.is_empty()
        || input.app_ids.len() > 100
        || input.app_ids.iter().collect::<BTreeSet<_>>().len() != input.app_ids.len()
    {
        return Err(Error::bad("Choose 1–100 unique applications"));
    }
    for member in &input.app_ids {
        if !honeycomb_core::valid_app_id(member) {
            return Err(Error::bad("Bundle members must use bare application IDs"));
        }
        let owner: Option<String> = sqlx::query_scalar(
            "SELECT org_id FROM applications WHERE plane='production' AND app_id=?",
        )
        .bind(member)
        .fetch_optional(&s.db)
        .await?;
        if owner.as_deref() != Some(org) {
            return Err(Error::bad(
                "Bundle members must be distinct applications from the bundle's organization",
            ));
        }
    }
    if let Some(logo) = &input.app_logo {
        let url = url::Url::parse(logo).map_err(|_| Error::bad("Logo must be an HTTPS URL"))?;
        if logo.len() > 2048
            || url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(Error::bad("Logo must be an HTTPS URL without credentials"));
        }
    }
    Ok(())
}
async fn accepted(s: &State, id: &str) -> Result<Value> {
    let value = s.management.bundle(id).await?;
    if value["bundle_id"] != id || value["org_id"] != organization(id)? {
        return Err(Error::unavailable(
            "IAM returned a different bundle identity",
        ));
    }
    Ok(value)
}
pub async fn get(S(s): S<State>, h: HeaderMap, Path(id): Path<String>) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    authorize(&c, &id)?;
    Ok(Json(accepted(&s, &id).await?))
}
pub async fn list(S(s): S<State>, h: HeaderMap) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    c.identity()?;
    if c.plane != "production" {
        return Err(Error::bad("Bundle management is production-only"));
    }
    let mut after = None;
    let mut cursors = BTreeSet::new();
    let mut items = Vec::new();
    loop {
        let page = s.management.bundle_inventory(after).await?;
        let rows = page["items"]
            .as_array()
            .ok_or_else(|| Error::unavailable("IAM omitted bundle inventory"))?;
        for row in rows {
            let id = row["resource_id"]
                .as_str()
                .ok_or_else(|| Error::unavailable("IAM omitted bundle identity"))?;
            if authorize(&c, id).is_ok() {
                let value = accepted(&s, id).await?;
                if value["deleted"] != true {
                    items.push(value);
                }
            }
        }
        match page["next_after"].as_str() {
            None => break,
            Some(cursor) => {
                if !cursors.insert(cursor.to_owned()) {
                    return Err(Error::unavailable("IAM repeated its inventory cursor"));
                }
                after = Some(
                    uuid::Uuid::parse_str(cursor)
                        .map_err(|_| Error::unavailable("Invalid IAM inventory cursor"))?,
                );
            }
        }
    }
    Ok(Json(json!({"items":items})))
}
pub async fn configure(
    S(s): S<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Configuration>,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    authorize(&c, &id)?;
    validate(&s, &id, &input).await?;
    let expected = revision(&h)?;
    let k = key(&h)?;
    let actor = &c.identity()?.principal_id;
    let digest = hex::encode(Sha256::digest(
        json!({"bundle_id":id,"expected":expected,"configuration":input}).to_string(),
    ));
    let existing = sqlx::query("SELECT id,request_hash,state,result FROM operations WHERE plane='production' AND actor=? AND idempotency_key=?").bind(actor).bind(k).fetch_optional(&s.db).await?;
    let (op, body) = if let Some(row) = existing {
        if row.get::<String, _>("request_hash") != digest {
            return Err(Error::conflict(
                "Idempotency key was used for a different request",
            ));
        }
        let op = row.get::<String, _>("id");
        if row.get::<String, _>("state") == "accepted" {
            let result: Value = serde_json::from_str(&row.get::<String, _>("result"))
                .map_err(|e| anyhow::anyhow!(e))?;
            return Ok(Json(result));
        }
        if row.get::<String, _>("state") == "rejected" {
            return Err(Error::conflict(
                "IAM rejected this operation. Read the current bundle, correct its configuration, and use a new idempotency key",
            ));
        }
        let body: String =
            sqlx::query_scalar("SELECT body FROM bundle_requests WHERE operation_id=?")
                .bind(&op)
                .fetch_one(&s.db)
                .await?;
        (
            op,
            serde_json::from_str::<Value>(&body).map_err(|e| anyhow::anyhow!(e))?,
        )
    } else {
        let current = match accepted(&s, &id).await {
            Ok(value) => Some(value),
            Err(e) if e.0 == StatusCode::NOT_FOUND => None,
            Err(e) => return Err(e),
        };
        let iam_revision = current
            .as_ref()
            .map_or(Some(0), |v| v["iam_revision"].as_i64())
            .ok_or_else(|| Error::unavailable("IAM omitted bundle revision"))?;
        if iam_revision != expected {
            return Err(Error::conflict(
                "Bundle revision changed. Reload before saving",
            ));
        }
        if current.as_ref().is_some_and(|v| v["deleted"] == true) {
            return Err(Error::conflict(
                "This bundle ID was deleted and cannot be reused",
            ));
        }
        let configuration_revision = current
            .as_ref()
            .map_or(Some(0), |v| v["configuration_revision"].as_i64())
            .ok_or_else(|| Error::unavailable("IAM omitted bundle configuration revision"))?
            + 1;
        let op = uuid::Uuid::new_v4().to_string();
        let body = json!({"operation_id":op,"expected_iam_revision":expected,"configuration_revision":configuration_revision,"app_name":input.app_name,"app_logo":input.app_logo,"app_ids":input.app_ids});
        let mut tx = s.db.begin().await?;
        sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,'production',?,?,'bundle.configure',?,?,?,?)").bind(&op).bind(actor).bind(k).bind(&id).bind(&digest).bind(configuration_revision).bind(now()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO bundle_requests(operation_id,bundle_id,body) VALUES(?,?,?)")
            .bind(&op)
            .bind(&id)
            .bind(body.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,'production',?,'bundle.configure',?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(actor).bind(&id).bind(now()).execute(&mut *tx).await?;
        tx.commit().await?;
        (op, body)
    };
    let response = s
        .management
        .configure_bundle(
            &id,
            &body,
            c.token.as_deref().ok_or_else(Error::unauthorized)?,
        )
        .await;
    match response {
        Ok(receipt) => {
            let config = &receipt["effective_configuration"];
            if receipt["state"] != "accepted"
                || receipt["operation_id"] != op
                || config["bundle_id"] != id
                || config["configuration_revision"] != body["configuration_revision"]
                || config["app_ids"] != body["app_ids"]
                || receipt["iam_revision"]
                    .as_i64()
                    .is_none_or(|n| n <= expected)
            {
                return Err(Error::unavailable(
                    "IAM has not confirmed this bundle configuration; retry with the same idempotency key",
                ));
            }
            sqlx::query("UPDATE operations SET state='accepted',result=?,error=NULL WHERE id=?")
                .bind(receipt.to_string())
                .bind(&op)
                .execute(&s.db)
                .await?;
            Ok(Json(receipt))
        }
        Err(error) => {
            // Authentication failures and uncertain transport results remain replayable.
            let rejected = matches!(
                error.0,
                StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY | StatusCode::CONFLICT
            );
            sqlx::query("UPDATE operations SET state=?,error=? WHERE id=? AND state<>'accepted'")
                .bind(if rejected { "rejected" } else { "pending" })
                .bind(&error.1.message)
                .bind(&op)
                .execute(&s.db)
                .await?;
            Err(error)
        }
    }
}
