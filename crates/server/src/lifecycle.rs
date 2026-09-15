//! Durable shared-environment coordination. Only protected service responses establish readiness.
use crate::{
    State,
    error::{Error, Result},
    now,
};
use serde_json::{Value, json};
use sqlx::Row;

pub async fn coordinate(
    s: &State,
    id: &str,
    operation_id: &str,
    actor_token: &str,
) -> Result<Value> {
    let lease = uuid::Uuid::new_v4().to_string();
    let acquired=sqlx::query("UPDATE operations SET lease_token=?,lease_until=? WHERE id=? AND resource=? AND plane='production' AND state='pending' AND (lease_token IS NULL OR lease_until<?)")
        .bind(&lease).bind(now()+600).bind(operation_id).bind(id).bind(now()).execute(&s.db).await?;
    if acquired.rows_affected() == 0 {
        let state: String = sqlx::query_scalar("SELECT state FROM environments WHERE id=?")
            .bind(id)
            .fetch_one(&s.db)
            .await?;
        return Ok(
            json!({"operation_id":operation_id,"environment_id":id,"state":state,"message":"Operation is already coordinating or completed; inspect its current service receipts"}),
        );
    }
    let result = coordinate_inner(s, id, operation_id, actor_token, &lease).await;
    sqlx::query(
        "UPDATE operations SET lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
    )
    .bind(operation_id)
    .bind(lease)
    .execute(&s.db)
    .await?;
    result
}
async fn coordinate_inner(
    s: &State,
    id: &str,
    operation_id: &str,
    actor_token: &str,
    lease: &str,
) -> Result<Value> {
    let environment = sqlx::query("SELECT * FROM environments WHERE id=?")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    let operation=sqlx::query("SELECT kind,revision,state FROM operations WHERE id=? AND resource=? AND plane='production'").bind(operation_id).bind(id).fetch_one(&s.db).await?;
    let revision: i64 = environment.get("revision");
    if operation.get::<i64, _>("revision") != revision {
        return Err(Error::conflict(
            "A newer lifecycle operation supersedes this operation",
        ));
    }
    if operation.get::<String, _>("state") == "accepted" {
        return Ok(
            json!({"operation_id":operation_id,"environment_id":id,"state":environment.get::<String,_>("state"),"revision":revision}),
        );
    }
    let kind: String = operation.get("kind");
    let action = kind
        .strip_prefix("environment.")
        .ok_or_else(|| Error::bad("Not an environment lifecycle operation"))?;
    let generation: i64 = environment.get("generation");
    let key_version: i64 = environment.get("key_version");
    let services=sqlx::query("SELECT app_id,snapshot,state FROM environment_services WHERE environment_id=? AND operation_id=? ORDER BY app_id").bind(id).bind(operation_id).fetch_all(&s.db).await?;
    if services.is_empty() {
        return Err(Error::conflict(
            "Lifecycle operation has no participating services",
        ));
    }
    for service in services {
        let held=sqlx::query("UPDATE operations SET lease_until=? WHERE id=? AND lease_token=? AND lease_until>? AND state='pending'")
            .bind(now()+600).bind(operation_id).bind(lease).bind(now()).execute(&s.db).await?;
        if held.rows_affected() != 1 {
            return Err(Error::conflict(
                "Lifecycle coordinator lease expired; retry the current operation",
            ));
        }

        if service.get::<String, _>("state") == "ready" {
            continue;
        }
        let app: String = service.get("app_id");
        let service_action = match action {
            "delete" => "disable",
            other => other,
        };
        let request = json!({"operation_id":operation_id,"action":service_action,"environment_id":id,"org_id":environment.get::<String,_>("org_id"),"environment_revision":revision,"generation":generation,"key_version":key_version,"testing_key":s.decrypt(&environment.get::<String,_>("encrypted_key"))?,"app_id":app,"snapshot":serde_json::from_str::<Value>(&service.get::<String,_>("snapshot")).map_err(|e|anyhow::anyhow!(e))?});
        let result = if app == s.app_id {
            apply_local(s,id,action,operation_id,lease).await.map(|_|json!({"state":"completed","operation_id":operation_id,"environment_id":id,"app_id":app,"environment_revision":revision,"generation":generation,"key_version":key_version}))
        } else {
            s.management
                .service_lifecycle(&app, &request, actor_token)
                .await
        };
        let error=match result {
            Ok(receipt) if receipt["state"]=="completed" && receipt["operation_id"]==operation_id && receipt["environment_id"]==id && receipt["app_id"]==app && receipt["environment_revision"]==revision && receipt["generation"]==generation && receipt["key_version"]==key_version => None,
            Ok(_)=>Some("Service has not confirmed the exact operation, environment revision, generation and key version".to_string()),
            Err(e)=>Some(e.1.message),
        };
        sqlx::query("UPDATE environment_services SET state=?,error=?,generation=? WHERE environment_id=? AND app_id=? AND operation_id=? AND EXISTS(SELECT 1 FROM operations WHERE id=? AND lease_token=? AND lease_until>?)")
            .bind(if error.is_none(){"ready"}else{"failed"}).bind(error).bind(generation).bind(id).bind(&app).bind(operation_id).bind(operation_id).bind(lease).bind(now()).execute(&s.db).await?;
    }
    let pending:i64=sqlx::query_scalar("SELECT COUNT(*) FROM environment_services WHERE environment_id=? AND operation_id=? AND state!='ready'").bind(id).bind(operation_id).fetch_one(&s.db).await?;
    if pending > 0 {
        let error = format!(
            "Waiting for {pending} service receipt(s); inspect environment services and retry this operation"
        );
        sqlx::query("UPDATE operations SET error=? WHERE id=? AND state='pending'")
            .bind(&error)
            .bind(operation_id)
            .execute(&s.db)
            .await?;
        return Ok(
            json!({"operation_id":operation_id,"environment_id":id,"state":environment.get::<String,_>("state"),"revision":revision,"generation":generation,"error":error}),
        );
    }
    let final_state = match action {
        "delete" => "deleted",
        "purge" => "purged",
        _ => "ready",
    };
    let mut tx = s.db.begin().await?;
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE id=? AND lease_token=? AND lease_until>? AND state='pending')").bind(operation_id).bind(lease).bind(now()).fetch_one(&mut *tx).await?;
    if !held {
        return Err(Error::conflict("Lifecycle coordinator lease changed"));
    }
    let updated=sqlx::query("UPDATE environments SET state=?,last_activity=?,deleted_at=?,purge_after=? WHERE id=? AND revision=?")
        .bind(final_state).bind(now()).bind(if action=="delete"{Some(now())}else{None::<i64>}).bind(if action=="delete"{Some(now()+30*86400)}else{None::<i64>}).bind(id).bind(revision).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(Error::conflict(
            "Environment changed during lifecycle coordination",
        ));
    }
    if action == "clean" {
        sqlx::query("DELETE FROM environment_services WHERE environment_id=? AND app_id NOT IN ('tos>iam',?)").bind(id).bind(&s.app_id).execute(&mut *tx).await?;
    }
    if action == "purge" {
        sqlx::query("UPDATE environments SET encrypted_key='',key_hash=?,name='Purged environment',description='' WHERE id=?").bind(format!("purged:{id}")).bind(id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM environment_services WHERE environment_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE operations SET state='accepted',error=NULL WHERE id=?")
        .bind(operation_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,'production','honeycomb-coordinator',?,?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(format!("environment.{action}.completed")).bind(id).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(
        json!({"operation_id":operation_id,"environment_id":id,"state":final_state,"revision":revision,"generation":generation,"key_version":key_version}),
    )
}
async fn apply_local(
    s: &State,
    id: &str,
    action: &str,
    operation_id: &str,
    lease: &str,
) -> Result<()> {
    if !matches!(action, "clean" | "purge") {
        return Ok(());
    }
    if id == "production" {
        return Err(Error::forbidden());
    }
    let mut tx = s.db.begin().await?;
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE id=? AND lease_token=? AND lease_until>? AND state='pending')").bind(operation_id).bind(lease).bind(now()).fetch_one(&mut *tx).await?;
    if !held {
        return Err(Error::conflict("Lifecycle coordinator lease changed"));
    }

    sqlx::query("DELETE FROM decisions WHERE request_id IN (SELECT id FROM publication_requests WHERE plane=?)").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM discussions WHERE request_id IN (SELECT id FROM publication_requests WHERE plane=?)").bind(id).execute(&mut *tx).await?;
    // Fixed SQL literals keep destructive work explicitly limited to one test plane.
    for query in [
        "DELETE FROM publication_requests WHERE plane=?",
        "DELETE FROM releases WHERE plane=?",
        "DELETE FROM reviews WHERE plane=?",
        "DELETE FROM stars WHERE plane=?",
        "DELETE FROM downloads WHERE plane=?",
        "DELETE FROM applications WHERE plane=?",
        "DELETE FROM operations WHERE plane=?",
        "DELETE FROM drafts WHERE plane=?",
        "DELETE FROM webhook_events WHERE plane=?",
        "DELETE FROM reports WHERE plane=?",
        "DELETE FROM outbox WHERE plane=?",
        "DELETE FROM audit WHERE plane=?",
    ] {
        sqlx::query(query).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
