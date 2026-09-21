//! Retry default-public submissions after configuration reconciliation or outages.
use crate::{
    State,
    api::Context,
    error::{Error, Result},
    now,
};
use axum::http::{HeaderMap, HeaderValue};
use serde_json::Value;
use sqlx::Row;

pub(crate) async fn remember(s: &State, c: &Context, app: &honeycomb_core::App) -> Result<()> {
    if c.plane != "production"
        || app.config["visibility"] != "public"
        || app.latest_version.is_none()
    {
        return Ok(());
    }
    let identity = c.admin(&app.org_id)?;
    let token = c.token.as_deref().ok_or_else(Error::unauthorized)?;
    let existing: Option<(i64, String)> = sqlx::query_as(
        "SELECT revision,encrypted_token FROM publication_intents WHERE plane=? AND app_id=?",
    )
    .bind(&c.plane)
    .bind(&app.app_id)
    .fetch_optional(&s.db)
    .await?;
    if let Some((revision, encrypted)) = existing
        && revision == app.revision
        && s.decrypt(&encrypted)? == token
    {
        return Ok(());
    }
    sqlx::query("INSERT INTO publication_intents(plane,app_id,revision,principal_id,encrypted_token,updated_at,next_attempt_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(plane,app_id) DO UPDATE SET revision=excluded.revision,principal_id=excluded.principal_id,encrypted_token=excluded.encrypted_token,updated_at=excluded.updated_at,next_attempt_at=excluded.next_attempt_at,error=NULL WHERE excluded.revision>=publication_intents.revision")
        .bind(&c.plane).bind(&app.app_id).bind(app.revision).bind(&identity.principal_id).bind(s.encrypt(token)?).bind(now()).bind(now()+30).execute(&s.db).await?;
    Ok(())
}
pub(crate) async fn completed(s: &State, app: &str, revision: i64, result: &Value) -> Result<()> {
    if result["id"].is_string() && result["state"] != "awaiting_review_plan" {
        sqlx::query(
            "DELETE FROM publication_intents WHERE plane='production' AND app_id=? AND revision=?",
        )
        .bind(app)
        .bind(revision)
        .execute(&s.db)
        .await?;
    }
    Ok(())
}
pub(crate) async fn tick(s: &State) -> Result<()> {
    sqlx::query("DELETE FROM publication_intents WHERE updated_at<? OR NOT EXISTS(SELECT 1 FROM applications a WHERE a.plane=publication_intents.plane AND a.app_id=publication_intents.app_id AND a.revision=publication_intents.revision AND json_extract(a.config,'$.visibility')='public')")
        .bind(now()-86400).execute(&s.db).await?;
    let jobs = sqlx::query("SELECT * FROM publication_intents WHERE plane='production' AND next_attempt_at<=? ORDER BY next_attempt_at LIMIT 20").bind(now()).fetch_all(&s.db).await?;
    for job in jobs {
        let id: String = job.get("app_id");
        let revision: i64 = job.get("revision");
        let claimed=sqlx::query("UPDATE publication_intents SET next_attempt_at=? WHERE plane='production' AND app_id=? AND revision=? AND next_attempt_at<=?").bind(now()+60).bind(&id).bind(revision).bind(now()).execute(&s.db).await?;
        if claimed.rows_affected() == 0 {
            continue;
        }
        let result = async {
            let token = s.decrypt(&job.get::<String, _>("encrypted_token"))?;
            let mut h = HeaderMap::new();
            h.insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {token}"))
                    .map_err(|_| Error::unauthorized())?,
            );
            let c = crate::api::context(s, &h).await?;
            if c.identity()?.principal_id != job.get::<String, _>("principal_id") {
                return Err(Error::forbidden());
            }
            crate::api::automatic_publication_result(s, &c, &id, Some(revision)).await
        }
        .await;
        match result {
            Ok(value) => completed(s, &id, revision, &value).await?,
            Err(error) => {
                sqlx::query("UPDATE publication_intents SET error=? WHERE plane='production' AND app_id=? AND revision=?").bind(&error.1.message).bind(&id).bind(revision).execute(&s.db).await?;
                if matches!(error.0.as_u16(), 401 | 403) {
                    sqlx::query("DELETE FROM publication_intents WHERE plane='production' AND app_id=? AND revision=?").bind(&id).bind(revision).execute(&s.db).await?;
                }
            }
        }
    }
    Ok(())
}
