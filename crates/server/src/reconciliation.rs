//! Signed management notifications trigger authoritative reads, never direct grants.
use crate::{
    State,
    error::{Error, Result},
    now,
};
use serde_json::{Value, json};
use sqlx::Row;

/// Called only after the IAM SDK has verified the raw signed envelope and test plane.
/// This management event name and projection are part of the proposed IAM contract.
pub async fn receive(s: &State, plane: &str, event: &Value) -> Result<bool> {
    let id = event["event_id"]
        .as_str()
        .ok_or_else(|| Error::bad("Missing event ID"))?;
    let aggregate = &event["aggregate"];
    let management = event["event_type"] == "honeycomb.application.changed.v1";
    let resource = if management {
        event["data"]["app_id"].as_str().unwrap_or("")
    } else {
        aggregate["id"].as_str().unwrap_or("iam")
    };
    let revision = aggregate["version"].as_i64().unwrap_or(0);
    if management
        && (aggregate["type"] != "application"
            || !honeycomb_core::valid_app_id(resource)
            || revision < 1)
    {
        return Err(Error::bad(
            "Management event requires an application and positive IAM revision",
        ));
    }
    let mut tx = s.db.begin().await?;
    let inserted = sqlx::query("INSERT OR IGNORE INTO webhook_events(plane,id,resource,revision,created_at) VALUES(?,?,?,?,?)")
        .bind(plane).bind(id).bind(resource).bind(revision).bind(now()).execute(&mut *tx).await?;
    if inserted.rows_affected() == 0 {
        return Ok(false);
    }
    if management {
        let current: Option<i64> =
            sqlx::query_scalar("SELECT iam_revision FROM applications WHERE plane=? AND app_id=?")
                .bind(plane)
                .bind(resource)
                .fetch_optional(&mut *tx)
                .await?;
        if current.is_some_and(|current| revision > current) {
            sqlx::query("INSERT INTO application_reconciliation(plane,app_id,target_revision,updated_at) VALUES(?,?,?,?) ON CONFLICT(plane,app_id) DO UPDATE SET target_revision=excluded.target_revision,state='pending',retry_at=0,error=NULL,updated_at=excluded.updated_at WHERE excluded.target_revision>application_reconciliation.target_revision")
                .bind(plane).bind(resource).bind(revision).bind(now()).execute(&mut *tx).await?;
            // Public visibility is restored only after an authoritative read
            // confirms IAM state and publication or legacy-adoption provenance.
            sqlx::query("UPDATE applications SET visibility='private' WHERE plane=? AND app_id=?")
                .bind(plane)
                .bind(resource)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(true)
}

pub async fn reconcile(s: &State, plane: &str, app_id: &str) -> Result<()> {
    let queued = sqlx::query("SELECT target_revision FROM application_reconciliation WHERE plane=? AND app_id=? AND state='pending'")
        .bind(plane).bind(app_id).fetch_optional(&s.db).await?;
    let Some(queued) = queued else {
        return Ok(());
    };
    let target: i64 = queued.get("target_revision");
    let snapshot = s
        .management
        .application_snapshot(app_id, (plane != "production").then_some(plane))
        .await?;
    let revision = snapshot["iam_revision"]
        .as_i64()
        .filter(|v| *v >= target)
        .ok_or_else(|| Error::unavailable("IAM snapshot is older than its notification"))?;
    let configuration_revision = snapshot["configuration_revision"]
        .as_i64()
        .filter(|v| *v >= 0)
        .ok_or_else(|| Error::unavailable("IAM omitted its accepted configuration revision"))?;
    let visibility = snapshot["visibility"]
        .as_str()
        .filter(|v| matches!(*v, "public" | "private"))
        .ok_or_else(|| Error::unavailable("IAM omitted its current visibility"))?;
    let availability = snapshot["availability"]
        .as_str()
        .filter(|v| matches!(*v, "active" | "disabled"))
        .ok_or_else(|| Error::unavailable("IAM omitted its current availability"))?;
    if snapshot["app_id"] != app_id {
        return Err(Error::unavailable("IAM returned a different application"));
    }
    let mut tx = s.db.begin().await?;
    let app = sqlx::query("SELECT revision,effective_revision,iam_revision,config,effective_config FROM applications WHERE plane=? AND app_id=?")
        .bind(plane).bind(app_id).fetch_one(&mut *tx).await?;
    let desired: Value =
        serde_json::from_str(&app.get::<String, _>("config")).map_err(|e| anyhow::anyhow!(e))?;
    // IAM applications predating Honeycomb have accepted configuration revision
    // zero. Only an explicitly seeded, accepted legacy projection can use zero;
    // it must never accept an ordinary pending creation or roll back revision 1.
    if configuration_revision == 0
        && (app.get::<i64, _>("iam_revision") <= 0
            || app.get::<Option<String>, _>("effective_config").is_none())
    {
        return Err(Error::conflict("No accepted legacy configuration exists"));
    }
    if configuration_revision > app.get::<i64, _>("revision")
        || configuration_revision < app.get::<i64, _>("effective_revision")
        || revision < app.get::<i64, _>("iam_revision")
    {
        return Err(Error::conflict(
            "IAM snapshot cannot replace a newer or unknown configuration",
        ));
    }
    let mut effective = snapshot["effective_configuration"]
        .as_object()
        .cloned()
        .ok_or_else(|| Error::unavailable("IAM omitted its effective configuration"))?;
    if effective.get("org_id") != desired.get("org_id")
        || effective.get("app_id") != desired.get("app_id")
    {
        return Err(Error::unavailable(
            "IAM returned a different configuration identity",
        ));
    }
    effective.retain(|field, _| {
        desired.get(field).is_some() && !matches!(field.as_str(), "app_secret" | "webhook_secret")
    });
    let published: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM publication_requests p JOIN operations o ON o.id=p.activation_operation WHERE p.plane=? AND p.app_id=? AND p.id=? AND p.revision=? AND p.state='published' AND o.state='accepted')")
        .bind(plane).bind(app_id).bind(snapshot["publication_request_id"].as_str().unwrap_or(""))
        .bind(configuration_revision).fetch_one(&mut *tx).await?;
    // An operator may adopt an already public, verified legacy IAM identity
    // without disabling its existing login while the first catalog release is
    // prepared. This provenance cannot approve any new configuration: it applies
    // only while IAM's accepted configuration is still the original revision 0.
    let adopted_public = if plane == "production" && configuration_revision == 0 {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM operations WHERE plane='production' AND resource=? AND kind='iam.adopt' AND state='accepted' AND revision=0 AND json_extract(result,'$.preserved_public_visibility')=1 AND json_extract(result,'$.visibility')='public' AND json_extract(result,'$.app_id')=? AND json_extract(result,'$.configuration_revision')=0 AND json_extract(result,'$.iam_revision')>0 AND json_extract(result,'$.iam_revision')<=?)")
            .bind(app_id).bind(app_id).bind(revision).fetch_one(&mut *tx).await?
    } else {
        false
    };
    let visible =
        if visibility == "public" && availability == "active" && (published || adopted_public) {
            "public"
        } else {
            "private"
        };
    // A newer event arriving during the read wins and remains pending.
    let saved = sqlx::query("UPDATE application_reconciliation SET state='accepted',error=NULL,updated_at=? WHERE plane=? AND app_id=? AND target_revision<=?")
        .bind(now()).bind(plane).bind(app_id).bind(revision).execute(&mut *tx).await?;
    if saved.rows_affected() != 1 {
        return Err(Error::conflict(
            "A newer IAM notification needs reconciliation",
        ));
    }
    sqlx::query("UPDATE applications SET iam_revision=?,effective_revision=?,effective_config=?,visibility=?,state=?,updated_at=? WHERE plane=? AND app_id=?")
        .bind(revision).bind(configuration_revision).bind(Value::Object(effective).to_string()).bind(visible)
        .bind(if availability == "active" { "active" } else { "disabled" }).bind(now()).bind(plane).bind(app_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE operations SET state='accepted',error=NULL,result=? WHERE plane=? AND resource=? AND kind='configure' AND revision=? AND state='pending'")
        .bind(json!({"reconciled_iam_revision":revision}).to_string()).bind(plane).bind(app_id).bind(configuration_revision).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Recheck known production apps even when a management notification was lost.
/// Claim a bounded batch durably; preserve pending notification targets and retries.
async fn schedule_checks(s: &State) -> Result<()> {
    let mut tx = s.db.begin().await?;
    let apps = sqlx::query("SELECT app_id,iam_revision FROM applications WHERE plane='production' AND iam_revision>0 AND iam_check_after<=? ORDER BY iam_check_after,app_id LIMIT 50")
        .bind(now()).fetch_all(&mut *tx).await?;
    for app in apps {
        let id: String = app.get("app_id");
        sqlx::query("INSERT INTO application_reconciliation(plane,app_id,target_revision,updated_at) VALUES('production',?,?,?) ON CONFLICT(plane,app_id) DO UPDATE SET target_revision=MAX(target_revision,excluded.target_revision),state='pending',retry_at=0,error=NULL,updated_at=excluded.updated_at WHERE application_reconciliation.state='accepted'")
            .bind(&id).bind(app.get::<i64,_>("iam_revision")).bind(now()).execute(&mut *tx).await?;
        sqlx::query(
            "UPDATE applications SET iam_check_after=? WHERE plane='production' AND app_id=?",
        )
        .bind(now() + 300)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn tick(s: &State) -> Result<()> {
    schedule_checks(s).await?;
    let jobs = sqlx::query("SELECT plane,app_id,target_revision FROM application_reconciliation WHERE state='pending' AND retry_at<=? ORDER BY updated_at LIMIT 50")
        .bind(now()).fetch_all(&s.db).await?;
    for job in jobs {
        let plane: String = job.get("plane");
        let app: String = job.get("app_id");
        if let Err(error) = reconcile(s, &plane, &app).await {
            sqlx::query("UPDATE application_reconciliation SET attempts=attempts+1,retry_at=?,error=? WHERE plane=? AND app_id=? AND state='pending' AND target_revision=?")
                .bind(now()+60).bind(error.1.message).bind(plane).bind(app).bind(job.get::<i64,_>("target_revision")).execute(&s.db).await?;
        }
    }
    Ok(())
}
pub async fn run(s: State) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
    loop {
        interval.tick().await;
        if let Err(error) = tick(&s).await {
            tracing::error!(error=%error.1.message,"Application reconciliation failed");
        }
    }
}

pub async fn status(
    axum::extract::State(s): axum::extract::State<State>,
    h: axum::http::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<Value>> {
    let c = crate::api::context(&s, &h).await?;
    crate::api::find_app(&s, &c, &id, true).await?;
    Ok(axum::Json(progress(&s, &c.plane, &id).await?))
}
async fn progress(s: &State, plane: &str, id: &str) -> Result<Value> {
    let row = sqlx::query("SELECT target_revision,state,attempts,error,updated_at FROM application_reconciliation WHERE plane=? AND app_id=?")
        .bind(plane).bind(id).fetch_optional(&s.db).await?;
    Ok(row.map(|r|json!({"state":r.get::<String,_>("state"),"target_revision":r.get::<i64,_>("target_revision"),"attempts":r.get::<i64,_>("attempts"),"error":r.get::<Option<String>,_>("error"),"updated_at":r.get::<i64,_>("updated_at")})).unwrap_or(json!({"state":"idle"})))
}
pub async fn refresh(
    axum::extract::State(s): axum::extract::State<State>,
    h: axum::http::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<(axum::http::StatusCode, axum::Json<Value>)> {
    crate::api::key(&h)?;
    let c = crate::api::context(&s, &h).await?;
    let app = crate::api::find_app(&s, &c, &id, true).await?;
    sqlx::query("INSERT INTO application_reconciliation(plane,app_id,target_revision,updated_at) VALUES(?,?,?,?) ON CONFLICT(plane,app_id) DO UPDATE SET target_revision=MAX(target_revision,excluded.target_revision),state='pending',retry_at=0,error=NULL,updated_at=excluded.updated_at")
        .bind(&c.plane).bind(&id).bind(app.iam_revision).bind(now()).execute(&s.db).await?;
    if let Err(error) = reconcile(&s, &c.plane, &id).await {
        sqlx::query("UPDATE application_reconciliation SET error=?,attempts=attempts+1,retry_at=? WHERE plane=? AND app_id=? AND state='pending'")
            .bind(error.1.message).bind(now()+60).bind(&c.plane).bind(&id).execute(&s.db).await?;
    }
    Ok((
        axum::http::StatusCode::ACCEPTED,
        axum::Json(progress(&s, &c.plane, &id).await?),
    ))
}
