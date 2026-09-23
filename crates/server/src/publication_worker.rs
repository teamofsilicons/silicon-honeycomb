//! Finish the exact publication requested by its owner after every approval.
//! IAM and Briefcase revalidate the authorizing manager's authority on every run.
use crate::{
    State,
    api::Context,
    error::{Error, Result},
    now,
};
use axum::{
    extract::{Path, State as S},
    http::{HeaderMap, HeaderValue},
};
use serde_json::Value;
use sqlx::Row;

pub(crate) async fn remember(s: &State, c: &Context, id: &str) -> Result<()> {
    let row = sqlx::query("SELECT p.state,p.revision,a.revision AS current_revision,a.org_id,o.actor AS activation_actor FROM publication_requests p JOIN applications a ON a.plane=p.plane AND a.app_id=p.app_id LEFT JOIN operations o ON o.id=p.activation_operation WHERE p.id=? AND p.plane=?")
        .bind(id).bind(&c.plane).fetch_one(&s.db).await?;
    let identity = c.identity()?;
    if !identity.admin(&row.get::<String, _>("org_id"))
        || matches!(
            row.get::<String, _>("state").as_str(),
            "published" | "denied" | "held_identifier_migration"
        )
        || row.get::<i64, _>("revision") != row.get::<i64, _>("current_revision")
        || row
            .get::<Option<String>, _>("activation_actor")
            .is_some_and(|actor| actor != identity.principal_id)
    {
        return Ok(());
    }
    let existing: Option<(String, String)> = sqlx::query_as(
        "SELECT encrypted_token,principal_id FROM publication_authorizations WHERE request_id=?",
    )
    .bind(id)
    .fetch_optional(&s.db)
    .await?;
    if let Some((existing, principal)) = existing
        && (principal != identity.principal_id
            || s.decrypt(&existing)? == c.token.as_deref().ok_or_else(Error::unauthorized)?)
    {
        return Ok(());
    }
    let token = s.encrypt(c.token.as_deref().ok_or_else(Error::unauthorized)?)?;
    sqlx::query("INSERT INTO publication_authorizations(request_id,encrypted_token,updated_at,next_attempt_at,principal_id) VALUES(?,?,?,?,?) ON CONFLICT(request_id) DO UPDATE SET encrypted_token=excluded.encrypted_token,updated_at=excluded.updated_at,next_attempt_at=excluded.next_attempt_at,principal_id=excluded.principal_id,attempts=0")
        .bind(id).bind(token).bind(now()).bind(now()).bind(&identity.principal_id).execute(&s.db).await?;
    Ok(())
}

pub(crate) async fn attempt(s: &State, id: &str) -> Result<()> {
    let Some(row) = sqlx::query("SELECT p.*,a.encrypted_token,a.attempts,a.principal_id FROM publication_requests p JOIN publication_authorizations a ON a.request_id=p.id WHERE p.id=? AND p.state IN ('awaiting_activation','activating')")
        .bind(id).fetch_optional(&s.db).await? else { return Ok(()); };
    let delay = (30_i64 * (1_i64 << row.get::<i64, _>("attempts").min(6))).min(1800);
    let claimed = sqlx::query("UPDATE publication_authorizations SET next_attempt_at=?,attempts=attempts+1 WHERE request_id=? AND next_attempt_at<=?")
        .bind(now()+delay).bind(id).bind(now()).execute(&s.db).await?;
    if claimed.rows_affected() == 0 {
        return Ok(());
    }
    let token = s.decrypt(&row.get::<String, _>("encrypted_token"))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| Error::unavailable("Publication authorization is invalid"))?,
    );
    let plane: String = row.get("plane");
    if plane != "production" {
        let key: Option<String> = sqlx::query_scalar(
            "SELECT encrypted_key FROM environments WHERE id=? AND state='ready'",
        )
        .bind(&plane)
        .fetch_optional(&s.db)
        .await?;
        let key =
            s.decrypt(&key.ok_or_else(|| Error::conflict("Testing environment is not ready"))?)?;
        headers.insert(
            "x-testing-environment-key",
            HeaderValue::from_str(&key)
                .map_err(|_| Error::unavailable("Testing authorization is invalid"))?,
        );
    }
    let key = if let Some(op) = row.get::<Option<String>, _>("activation_operation") {
        let existing = sqlx::query("SELECT actor,idempotency_key FROM operations WHERE id=?")
            .bind(op)
            .fetch_one(&s.db)
            .await?;
        if existing.get::<String, _>("actor") != row.get::<String, _>("principal_id") {
            return Ok(());
        }
        existing.get::<String, _>("idempotency_key")
    } else {
        format!("automatic-publication:{id}")
    };
    headers.insert(
        "idempotency-key",
        HeaderValue::from_str(&key).map_err(|_| Error::bad("Invalid publication operation"))?,
    );
    headers.insert(
        "if-match",
        HeaderValue::from_str(&row.get::<i64, _>("revision").to_string()).unwrap(),
    );
    let result = crate::activation::activate(S(s.clone()), headers, Path(id.to_owned())).await;
    match result {
        Ok((_, receipt)) => {
            if receipt.0["state"] == "accepted" {
                sqlx::query("DELETE FROM publication_authorizations WHERE request_id=?")
                    .bind(id)
                    .execute(&s.db)
                    .await?;
            } else {
                sqlx::query(
                    "UPDATE publication_requests SET error=? WHERE id=? AND state!='published'",
                )
                .bind(receipt.0["error"].as_str())
                .bind(id)
                .execute(&s.db)
                .await?;
            }
        }
        Err(error) => {
            let auth = matches!(error.0.as_u16(), 401 | 403);
            let message = if auth {
                "Publication is approved. The manager authorizing publication must sign in and open Sent requests to renew authorization; publication will then resume automatically."
            } else {
                &error.1.message
            };
            sqlx::query(
                "UPDATE publication_requests SET error=? WHERE id=? AND state!='published'",
            )
            .bind(message)
            .bind(id)
            .execute(&s.db)
            .await?;
            if auth {
                sqlx::query("DELETE FROM publication_authorizations WHERE request_id=?")
                    .bind(id)
                    .execute(&s.db)
                    .await?;
            }
        }
    }
    Ok(())
}

pub(crate) async fn after_approval(s: &State, c: &Context, id: &str) -> Result<Value> {
    // App managers can complete an existing request; outside reviewers cannot.
    remember(s, c, id).await?;
    attempt(s, id).await?;
    sqlx::query("UPDATE publication_requests SET error='Publication is approved. An application manager must sign in and open Sent requests to resume automatic publication.' WHERE id=? AND state='awaiting_activation' AND error IS NULL AND NOT EXISTS(SELECT 1 FROM publication_authorizations WHERE request_id=?)")
        .bind(id).bind(id).execute(&s.db).await?;
    let row = sqlx::query("SELECT state,error FROM publication_requests WHERE id=? AND plane=?")
        .bind(id)
        .bind(&c.plane)
        .fetch_one(&s.db)
        .await?;
    Ok(
        serde_json::json!({"publication_state":row.get::<String,_>("state"),"publication_error":row.get::<Option<String>,_>("error")}),
    )
}

pub async fn tick(s: &State) -> Result<()> {
    crate::publication_intents::tick(s).await?;
    sqlx::query("DELETE FROM publication_authorizations WHERE updated_at<? OR request_id IN (SELECT p.id FROM publication_requests p JOIN applications a ON a.plane=p.plane AND a.app_id=p.app_id WHERE p.state IN ('published','denied') OR p.revision!=a.revision)").bind(now()-86400).execute(&s.db).await?;
    let ids: Vec<String> = sqlx::query_scalar("SELECT p.id FROM publication_requests p JOIN publication_authorizations a ON a.request_id=p.id WHERE p.state IN ('awaiting_activation','activating') AND a.next_attempt_at<=? ORDER BY a.next_attempt_at LIMIT 20").bind(now()).fetch_all(&s.db).await?;
    for id in ids {
        if let Err(error) = attempt(s, &id).await {
            tracing::warn!(request_id=%id,code=%error.1.code,"Automatic publication deferred");
        }
    }
    Ok(())
}
pub async fn run(s: State) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
    loop {
        interval.tick().await;
        if let Err(error) = tick(&s).await {
            tracing::warn!(code=%error.1.code,"Publication worker retrying");
        }
    }
}
