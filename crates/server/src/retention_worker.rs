//! Durable retention scheduling. Only exact service receipts complete a cleanup operation.
use crate::{State, error::Result, now};
use serde_json::json;
use sqlx::Row;
use std::collections::BTreeSet;

/// Claim one due action under the same transaction used to calculate its eligibility.
pub async fn schedule(s: &State, id: &str, at: i64) -> Result<Option<String>> {
    if id == "production" {
        return Err(crate::error::Error::forbidden());
    }
    let mut tx = s.db.begin().await?;
    let environment =
        sqlx::query("SELECT state,revision,generation,purge_after FROM environments WHERE id=?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE state='pending' AND ((plane='production' AND resource=? AND kind LIKE 'environment.%') OR plane=?))").bind(id).bind(id).fetch_one(&mut *tx).await?;
    if pending {
        return Ok(None);
    }
    let state: String = environment.get("state");
    let mut targets = Vec::new();
    let action = if state == "deleted"
        && environment
            .get::<Option<i64>, _>("purge_after")
            .is_some_and(|deadline| deadline <= at)
    {
        "purge"
    } else if state == "ready" {
        let plan = crate::retention::plan(&mut tx, id, at).await?;
        if plan["eligible_for_deletion"] == true {
            "delete"
        } else {
            targets = plan["applications"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|app| app["eligible_for_retirement"] == true)
                .filter_map(|app| app["app_id"].as_str().map(str::to_owned))
                .collect();
            if targets.is_empty() {
                return Ok(None);
            }
            "retire"
        }
    } else {
        return Ok(None);
    };
    let expected: i64 = environment.get("revision");
    let op = uuid::Uuid::new_v4().to_string();
    let desired = match action {
        "delete" => "deleting",
        "purge" => "purging",
        _ => "ready",
    };
    let changed = sqlx::query(
        "UPDATE environments SET state=?,revision=revision+1 WHERE id=? AND revision=? AND state=?",
    )
    .bind(desired)
    .bind(id)
    .bind(expected)
    .bind(&state)
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() != 1 {
        return Ok(None);
    }
    let request = json!({"source":"retention","retired_apps":targets});
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,request_json,created_at) VALUES(?,'production','honeycomb-retention',?,?,?,?,?,?,?)")
        .bind(&op).bind(format!("retention:{id}:{expected}:{action}")).bind(format!("environment.{action}")).bind(id).bind(request.to_string()).bind(expected+1).bind(request.to_string()).bind(at).execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO retention_jobs(operation_id,environment_id,next_attempt_at) VALUES(?,?,?)",
    )
    .bind(&op)
    .bind(id)
    .bind(at)
    .execute(&mut *tx)
    .await?;
    // All participants must clear only the target apps' data, including storage they own
    // in another service. Core services and each retired app are always participants.
    let mut participants = BTreeSet::from([s.iam_app_id.clone(), s.app_id.clone()]);
    participants.extend(targets.iter().cloned());
    for app in participants {
        sqlx::query("INSERT OR IGNORE INTO environment_services(environment_id,app_id,source_revision,snapshot,state,operation_id,generation) VALUES(?,?,0,'{}','pending',?,?)")
            .bind(id).bind(&app).bind(&op).bind(environment.get::<i64,_>("generation")).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE environment_services SET state='pending',error=NULL,receipt=NULL,operation_id=? WHERE environment_id=?").bind(&op).bind(id).execute(&mut *tx).await?;
    for app in &targets {
        sqlx::query(
            "INSERT INTO retirement_targets(operation_id,environment_id,app_id) VALUES(?,?,?)",
        )
        .bind(&op)
        .bind(id)
        .bind(app)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,'production','honeycomb-retention',?,?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(format!("environment.{action}.scheduled")).bind(id).bind(at).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(op))
}
/// Execute a bounded batch. Advancing next_attempt_at also prevents duplicate worker hot loops;
/// the lifecycle lease fences remote side effects and durable receipt writes.
pub async fn tick(s: &State) -> Result<()> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM environments WHERE state IN ('ready','deleted') ORDER BY id",
    )
    .fetch_all(&s.db)
    .await?;
    for id in ids {
        if let Err(error) = schedule(s, &id, now()).await {
            tracing::error!(environment=%id,code=%error.1.code,"Environment retention scheduling failed");
        }
    }
    let jobs=sqlx::query("SELECT j.operation_id,j.environment_id,j.attempts FROM retention_jobs j JOIN operations o ON o.id=j.operation_id WHERE o.state='pending' AND j.next_attempt_at<=? ORDER BY j.next_attempt_at,j.operation_id LIMIT 50")
        .bind(now()).fetch_all(&s.db).await?;
    for job in jobs {
        let op: String = job.get("operation_id");
        let attempt: i64 = job.get("attempts");
        let delay = (60_i64.saturating_mul(1_i64 << attempt.min(6))).min(3600);
        let claimed=sqlx::query("UPDATE retention_jobs SET attempts=attempts+1,next_attempt_at=? WHERE operation_id=? AND next_attempt_at<=?")
            .bind(now()+delay).bind(&op).bind(now()).execute(&s.db).await?;
        if claimed.rows_affected() != 1 {
            continue;
        }
        // No user token is invented or persisted. The integration must authorize the
        // dedicated service's scheduled retention action; unavailable transport stays pending.
        if let Err(error) =
            crate::lifecycle::coordinate(s, &job.get::<String, _>("environment_id"), &op, "").await
        {
            sqlx::query("UPDATE operations SET error=? WHERE id=? AND state='pending'")
                .bind(error.1.code)
                .bind(&op)
                .execute(&s.db)
                .await?;
        }
    }
    Ok(())
}
pub async fn run(s: State) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        interval.tick().await;
        if let Err(error) = tick(&s).await {
            tracing::error!(code=%error.1.code,"Retention scheduling failed");
        }
    }
}

pub(crate) async fn ensure_app_available(s: &State, plane: &str, app: &str) -> Result<()> {
    let retiring:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM retirement_targets t JOIN operations o ON o.id=t.operation_id WHERE t.environment_id=? AND t.app_id=? AND o.state='pending')")
        .bind(plane).bind(app).fetch_one(&s.db).await?;
    if retiring {
        return Err(crate::error::Error::conflict(
            "This test application is retiring; inspect the environment's cleanup progress",
        ));
    }
    Ok(())
}
