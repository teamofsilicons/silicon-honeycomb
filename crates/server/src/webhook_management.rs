//! Durable, actor-bound webhook changes. Signing material is encrypted; step-up is transient.
use crate::{
    State,
    api::{Context, context, find_app, key, revision},
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    extract::{Path, State as S},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, sqlite::SqliteRow};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    action: String,
    pending_endpoint_id: Option<uuid::Uuid>,
    webhook_secret: Option<String>,
    step_up_assertion: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retry {
    step_up_assertion: Option<String>,
}
fn step_up(value: Option<&str>) -> Result<()> {
    if value.is_some_and(|s| s.is_empty() || s.len() > 16384) {
        return Err(Error::bad("Invalid IAM step-up assertion"));
    }
    Ok(())
}
pub async fn status(S(s): S<State>, h: HeaderMap, Path(app): Path<String>) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    find_app(&s, &c, &app, true).await?;
    Ok(Json(
        s.management
            .webhook_state(&app, c.environment.as_deref())
            .await?,
    ))
}
pub async fn mutate(
    S(s): S<State>,
    h: HeaderMap,
    Path(app): Path<String>,
    Json(body): Json<Change>,
) -> Result<(StatusCode, Json<Value>)> {
    let c = context(&s, &h).await?;
    find_app(&s, &c, &app, true).await?;
    let expected = revision(&h)?;
    let k = key(&h)?;
    step_up(body.step_up_assertion.as_deref())?;
    let kind = match body.action.as_str() {
        "approve" if body.pending_endpoint_id.is_some() && body.webhook_secret.is_none() => {
            "webhook.approve"
        }
        "rotate"
            if body.pending_endpoint_id.is_none()
                && body.webhook_secret.as_ref().is_some_and(|v| {
                    (32..=4096).contains(&v.len()) && !v.chars().any(char::is_control)
                }) =>
        {
            "webhook.rotate"
        }
        _ => {
            return Err(Error::bad(
                "Choose approve with pending_endpoint_id, or rotate with a 32–4096 byte webhook_secret",
            ));
        }
    };
    // A new step-up assertion can retry the same mutation without changing its identity.
    let digest = hex::encode(Sha256::digest(json!({"kind":kind,"app":app,"revision":expected,"endpoint":body.pending_endpoint_id,"secret":body.webhook_secret}).to_string()));
    let actor = &c.identity()?.principal_id;
    let existing =
        sqlx::query("SELECT * FROM operations WHERE plane=? AND actor=? AND idempotency_key=?")
            .bind(&c.plane)
            .bind(actor)
            .bind(k)
            .fetch_optional(&s.db)
            .await?;
    if let Some(row) = existing {
        if row.get::<String, _>("request_hash") != digest {
            return Err(Error::conflict(
                "Idempotency key already used for another operation",
            ));
        }
        return apply(&s, &c, &row, body.step_up_assertion.as_deref()).await;
    }
    // Obtain the exact pending destination from IAM, never derive its identity from a URL.
    let remote = s
        .management
        .webhook_state(&app, c.environment.as_deref())
        .await?;
    let mut tx = s.db.begin().await?;
    let current = sqlx::query("SELECT revision,effective_revision,iam_revision FROM applications WHERE plane=? AND app_id=?")
        .bind(&c.plane).bind(&app).fetch_one(&mut *tx).await?;
    let iam: i64 = current.get("iam_revision");
    if expected != current.get::<i64, _>("revision")
        || expected != current.get::<i64, _>("effective_revision")
        || iam == 0
        || remote["app_id"] != app
        || remote["iam_revision"] != iam
        || remote["configuration_revision"] != expected
    {
        return Err(Error::conflict(
            "Webhook changes require the current accepted configuration. Refresh IAM state first.",
        ));
    }
    if kind == "webhook.approve" && remote["pending_endpoint_id"] != json!(body.pending_endpoint_id)
    {
        return Err(Error::conflict(
            "The pending webhook destination changed. Refresh before approving.",
        ));
    }
    let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE plane=? AND resource=? AND state IN ('pending','held_identifier_migration') AND kind IN ('configure','secret.rotate','publication.activate','webhook.approve','webhook.rotate'))")
        .bind(&c.plane).bind(&app).fetch_one(&mut *tx).await?;
    if pending {
        return Err(Error::conflict(
            "Finish or retry the pending application operation first",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let mut request = json!({"operation_id":id,"app_id":app,"kind":kind,"configuration_revision":expected,"expected_iam_revision":iam,"pending_endpoint_id":body.pending_endpoint_id});
    if let Some(secret) = body.webhook_secret {
        request["encrypted_webhook_secret"] = json!(s.encrypt(&secret)?);
    }
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,request_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
        .bind(&id).bind(&c.plane).bind(actor).bind(k).bind(kind).bind(&app).bind(digest).bind(expected).bind(request.to_string()).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    let row = sqlx::query("SELECT * FROM operations WHERE id=?")
        .bind(&id)
        .fetch_one(&s.db)
        .await?;
    apply(&s, &c, &row, body.step_up_assertion.as_deref()).await
}
pub async fn retry(
    S(s): S<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Retry>,
) -> Result<(StatusCode, Json<Value>)> {
    key(&h)?;
    step_up(body.step_up_assertion.as_deref())?;
    let c = context(&s, &h).await?;
    let row = sqlx::query("SELECT * FROM operations WHERE id=? AND plane=? AND actor=? AND kind IN ('webhook.approve','webhook.rotate')")
        .bind(&id).bind(&c.plane).bind(&c.identity()?.principal_id).fetch_optional(&s.db).await?.ok_or_else(Error::missing)?;
    find_app(&s, &c, &row.get::<String, _>("resource"), true).await?;
    apply(&s, &c, &row, body.step_up_assertion.as_deref()).await
}
async fn apply(
    s: &State,
    c: &Context,
    row: &SqliteRow,
    proof: Option<&str>,
) -> Result<(StatusCode, Json<Value>)> {
    let id: String = row.get("id");
    let app: String = row.get("resource");
    let k: String = row.get("idempotency_key");
    if row.get::<String, _>("state") == "accepted" {
        let receipt: Value = serde_json::from_str(&row.get::<String, _>("result"))
            .map_err(|e| anyhow::anyhow!(e))?;
        return Ok((StatusCode::OK, Json(receipt)));
    }
    let mut request: Value = serde_json::from_str(&row.get::<String, _>("request_json"))
        .map_err(|e| anyhow::anyhow!(e))?;
    let encrypted = request["encrypted_webhook_secret"]
        .as_str()
        .map(str::to_owned);
    request
        .as_object_mut()
        .unwrap()
        .remove("encrypted_webhook_secret");
    if let Some(secret) = &encrypted {
        request["webhook_secret"] = json!(s.decrypt(secret)?);
    }
    let response = s
        .management
        .webhook_mutation(
            &request,
            c.token.as_deref().ok_or_else(Error::unauthorized)?,
            proof,
            c.environment.as_deref(),
        )
        .await;
    let valid = response.as_ref().ok().filter(|r| {
        r["state"] == "accepted"
            && r["operation_id"] == id
            && r["app_id"] == app
            && r["iam_revision"]
                .as_i64()
                .is_some_and(|v| v > request["expected_iam_revision"].as_i64().unwrap())
            && if request["kind"] == "webhook.approve" {
                r["webhook_endpoint_id"] == request["pending_endpoint_id"]
            } else {
                r["webhook_secret_version"].as_i64().is_some_and(|v| v > 0)
            }
    });
    if let Some(r) = valid {
        // Allow an identical concurrent retry to return its durable receipt.
        let mut tx = s.db.begin().await?;
        let accepted: Option<String> =
            sqlx::query_scalar("SELECT result FROM operations WHERE id=? AND state='accepted'")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(receipt) = accepted {
            tx.commit().await?;
            return Ok((
                StatusCode::OK,
                Json(serde_json::from_str(&receipt).map_err(|e| anyhow::anyhow!(e))?),
            ));
        }
        let iam = r["iam_revision"].as_i64().unwrap();
        let changed = sqlx::query("UPDATE applications SET iam_revision=?,webhook_secret=COALESCE(?,webhook_secret),updated_at=? WHERE plane=? AND app_id=? AND revision=? AND iam_revision<=?")
            .bind(iam).bind(&encrypted).bind(now()).bind(&c.plane).bind(&app).bind(row.get::<i64,_>("revision")).bind(iam).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::conflict(
                "IAM accepted this change, but the application advanced. Reconcile before retrying this operation.",
            ));
        }
        let receipt = json!({"id":id,"resource":app,"kind":request["kind"],"state":"accepted","idempotency_key":k,"iam_revision":iam,"webhook_endpoint_id":r["webhook_endpoint_id"],"webhook_secret_version":r["webhook_secret_version"]});
        sqlx::query("UPDATE operations SET state='accepted',error=NULL,result=? WHERE id=?")
            .bind(receipt.to_string())
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,?,?,?,?,?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&c.plane)
        .bind(&c.identity()?.principal_id)
        .bind(request["kind"].as_str().unwrap())
        .bind(&app)
        .bind(now())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok((StatusCode::OK, Json(receipt)));
    }
    let code = response
        .err()
        .map(|e| e.1.code)
        .unwrap_or_else(|| "awaiting_iam_acceptance".into());
    sqlx::query("UPDATE operations SET error=? WHERE id=? AND state='pending'")
        .bind(&code)
        .bind(&id)
        .execute(&s.db)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(
            json!({"id":id,"resource":app,"kind":request["kind"],"state":"pending","idempotency_key":k,"error_code":code,"message":"Webhook change saved. Retry this operation with fresh IAM step-up evidence if required."}),
        ),
    ))
}
