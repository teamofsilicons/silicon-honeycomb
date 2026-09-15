//! Secret values and step-up assertions are never persisted by Honeycomb.
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
pub struct Rotation {
    #[serde(default)]
    step_up_assertion: Option<String>,
}
pub async fn rotate(
    S(s): S<State>,
    h: HeaderMap,
    Path(app_id): Path<String>,
    Json(body): Json<Rotation>,
) -> Result<(StatusCode, Json<Value>)> {
    let c = context(&s, &h).await?;
    let app = find_app(&s, &c, &app_id, true).await?;
    let expected = revision(&h)?;
    let k = key(&h)?;
    if body
        .step_up_assertion
        .as_ref()
        .is_some_and(|v| v.is_empty() || v.len() > 16384)
    {
        return Err(Error::bad("Invalid IAM step-up assertion"));
    }
    // Renewed step-up evidence is transient authority, not a new rotation request.
    let digest = hex::encode(Sha256::digest(
        json!({"kind":"secret.rotate","app_id":app_id,"revision":expected}).to_string(),
    ));
    let mut tx = s.db.begin().await?;
    let existing =
        sqlx::query("SELECT * FROM operations WHERE plane=? AND actor=? AND idempotency_key=?")
            .bind(&c.plane)
            .bind(&c.identity()?.principal_id)
            .bind(k)
            .fetch_optional(&mut *tx)
            .await?;
    let (id, request) = if let Some(row) = existing {
        if row.get::<String, _>("request_hash") != digest {
            return Err(Error::conflict(
                "Idempotency key already used for another operation",
            ));
        }
        if row.get::<String, _>("state") == "accepted" {
            tx.commit().await?;
            return Ok((StatusCode::OK, Json(recover_result(&s, &c, &row).await?)));
        }
        let request: Value = serde_json::from_str(&row.get::<String, _>("request_json"))
            .map_err(|e| anyhow::anyhow!(e))?;
        (row.get::<String, _>("id"), request)
    } else {
        if app.revision != expected || app.effective_revision != expected || app.iam_revision == 0 {
            return Err(Error::conflict(
                "Secret rotation requires the current accepted application revision",
            ));
        }
        let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE plane=? AND resource=? AND state='pending' AND kind IN ('webhook.approve','webhook.rotate','secret.rotate','configure','publication.activate'))")
            .bind(&c.plane).bind(&app_id).fetch_one(&mut *tx).await?;
        if pending {
            return Err(Error::conflict(
                "Another application operation is pending; retry it with its original idempotency key",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let request = json!({"operation_id":id,"app_id":app_id,"configuration_revision":expected,"expected_iam_revision":app.iam_revision});
        sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,request_json,created_at) VALUES(?,?,?,?,'secret.rotate',?,?,?,?,?)")
            .bind(&id).bind(&c.plane).bind(&c.identity()?.principal_id).bind(k).bind(&app_id).bind(&digest).bind(expected).bind(request.to_string()).bind(now()).execute(&mut *tx).await?;
        (id, request)
    };
    tx.commit().await?;
    let response = s
        .management
        .rotate_secret(
            &request,
            c.token.as_deref().ok_or_else(Error::unauthorized)?,
            body.step_up_assertion.as_deref(),
            c.environment.as_deref(),
        )
        .await;
    match response {
        Ok(response)
            if response["state"] == "accepted"
                && response["operation_id"] == id
                && response["app_id"] == app_id
                && response["configuration_revision"] == expected =>
        {
            let iam = response["iam_revision"]
                .as_i64()
                .filter(|r| *r > request["expected_iam_revision"].as_i64().unwrap())
                .ok_or_else(|| Error::unavailable("IAM returned an invalid rotation revision"))?;
            let version = response["credential_version"]
                .as_i64()
                .filter(|r| *r > 0)
                .ok_or_else(|| Error::unavailable("IAM omitted the credential version"))?;
            let secret = response["app_secret"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Error::unavailable("IAM omitted the one-time replacement secret"))?;
            let mut tx = s.db.begin().await?;
            let update=sqlx::query("UPDATE applications SET iam_revision=?,credential_version=?,updated_at=? WHERE plane=? AND app_id=? AND revision=? AND iam_revision=? AND credential_version<?")
                .bind(iam).bind(version).bind(now()).bind(&c.plane).bind(&app_id).bind(expected).bind(request["expected_iam_revision"].as_i64().unwrap()).bind(version).execute(&mut *tx).await?;
            if update.rows_affected() != 1 {
                return Err(Error::conflict(
                    "Application changed during rotation; recover this operation before starting another",
                ));
            }
            let receipt = json!({"credential_version":version,"iam_revision":iam});
            sqlx::query("UPDATE operations SET state='accepted',error=NULL,result=? WHERE id=?")
                .bind(receipt.to_string())
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,?,?,'application.secret.rotate',?,?)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(&c.plane).bind(&c.identity()?.principal_id).bind(&app_id).bind(now()).execute(&mut *tx).await?;
            tx.commit().await?;
            Ok((
                StatusCode::OK,
                Json(
                    json!({"id":id,"resource":app_id,"state":"accepted","idempotency_key":k,"credential_version":version,"app_secret":secret}),
                ),
            ))
        }
        result => {
            let code = match result {
                Err(e) => e.1.code,
                _ => "awaiting_iam_acceptance".into(),
            };
            // Arbitrary upstream messages could include a credential; persist only the code.
            sqlx::query("UPDATE operations SET error=? WHERE id=? AND state='pending'")
                .bind(&code)
                .bind(&id)
                .execute(&s.db)
                .await?;
            Ok((
                StatusCode::ACCEPTED,
                Json(
                    json!({"id":id,"resource":app_id,"state":"pending","idempotency_key":k,"error_code":code,"message":"Rotation is saved and awaiting IAM. Retry the same request with its original idempotency key; supply fresh IAM step-up evidence if required."}),
                ),
            ))
        }
    }
}

pub async fn recover(S(s): S<State>, h: HeaderMap, Path(id): Path<String>) -> Result<Json<Value>> {
    key(&h)?;
    let c = context(&s, &h).await?;
    let row = sqlx::query("SELECT * FROM operations WHERE id=? AND plane=? AND actor=?")
        .bind(&id)
        .bind(&c.plane)
        .bind(&c.identity()?.principal_id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)?;
    let app: String = row.get("resource");
    find_app(&s, &c, &app, true).await?;
    if !["configure", "secret.rotate"].contains(&row.get::<String, _>("kind").as_str()) {
        return Err(Error::bad(
            "This operation does not produce an application secret",
        ));
    }
    Ok(Json(recover_result(&s, &c, &row).await?))
}
async fn recover_result(s: &State, c: &Context, row: &SqliteRow) -> Result<Value> {
    let id: String = row.get("id");
    let app: String = row.get("resource");
    let response = s
        .management
        .operation_result(
            &id,
            c.token.as_deref().ok_or_else(Error::unauthorized)?,
            c.environment.as_deref(),
        )
        .await?;
    if response["operation_id"] != id
        || response["app_id"] != app
        || response["configuration_revision"] != row.get::<i64, _>("revision")
    {
        return Err(Error::unavailable(
            "IAM operation recovery returned a different operation",
        ));
    }
    let state = response["state"]
        .as_str()
        .filter(|s| ["accepted", "pending", "rejected"].contains(s))
        .ok_or_else(|| Error::unavailable("IAM returned an invalid operation state"))?;
    sqlx::query("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,?,?,'application.secret.recover',?,?)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&c.plane).bind(&c.identity()?.principal_id).bind(&app).bind(now()).execute(&s.db).await?;
    if state == "accepted"
        && row.get::<String, _>("kind") == "secret.rotate"
        && row.get::<String, _>("state") == "pending"
    {
        let request: Value = serde_json::from_str(&row.get::<String, _>("request_json"))
            .map_err(|e| anyhow::anyhow!(e))?;
        let previous = request["expected_iam_revision"].as_i64().unwrap();
        let iam = response["iam_revision"]
            .as_i64()
            .filter(|r| *r > previous)
            .ok_or_else(|| Error::unavailable("IAM recovery omitted its accepted revision"))?;
        let version = response["credential_version"]
            .as_i64()
            .filter(|r| *r > 0)
            .ok_or_else(|| Error::unavailable("IAM recovery omitted its credential version"))?;
        let mut tx = s.db.begin().await?;
        let updated=sqlx::query("UPDATE applications SET iam_revision=?,credential_version=?,updated_at=? WHERE plane=? AND app_id=? AND revision=? AND iam_revision=? AND credential_version<?")
            .bind(iam).bind(version).bind(now()).bind(&c.plane).bind(&app).bind(row.get::<i64,_>("revision")).bind(previous).bind(version).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(Error::conflict(
                "Application changed before rotation recovery; reconcile its current IAM state",
            ));
        }
        sqlx::query("UPDATE operations SET state='accepted',error=NULL,result=? WHERE id=?")
            .bind(json!({"iam_revision":iam,"credential_version":version}).to_string())
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    let mut result = json!({"id":id,"resource":app,"state":state});
    if state == "accepted" {
        if let Some(secret) = response["app_secret"].as_str().filter(|s| !s.is_empty()) {
            result["app_secret"] = json!(secret);
        } else {
            result["message"] = json!(
                "IAM's secret replay window has expired. Request a new rotation if the secret was lost."
            );
        }
    }
    Ok(result)
}
