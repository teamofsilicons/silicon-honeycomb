//! Server-timed activity and dependency-aware retention. Root authority stays in one environment.
use crate::{
    State,
    api::{key, revision},
    control::manage,
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    extract::{Path, State as S},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    generation: i64,
    key_version: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retention {
    idle_days: i64,
}

pub async fn report(
    S(s): S<State>,
    h: HeaderMap,
    Path((id, app)): Path<(String, String)>,
    Json(body): Json<Activity>,
) -> Result<Json<Value>> {
    let k = key(&h)?;
    let (authorized, actor) = manage(&s, &h, &id).await?;
    let mut tx = s.db.begin().await?;
    let environment =
        sqlx::query("SELECT state,generation,key_version FROM environments WHERE id=?")
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
    if environment.get::<String, _>("state") != "ready"
        || environment.get::<i64, _>("generation") != body.generation
        || environment.get::<i64, _>("key_version") != body.key_version
        || authorized.get::<i64, _>("key_version") != body.key_version
    {
        return Err(Error::conflict(
            "Activity requires a ready environment with its current generation and key version",
        ));
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM applications WHERE plane=? AND app_id=? AND state='active')",
    )
    .bind(&id)
    .bind(&app)
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(Error::missing());
    }
    if let Some(event) = sqlx::query("SELECT app_id,generation,key_version,received_at FROM environment_activity_events WHERE environment_id=? AND actor=? AND idempotency_key=?").bind(&id).bind(&actor).bind(k).fetch_optional(&mut *tx).await? {
        if event.get::<String,_>("app_id") != app || event.get::<i64,_>("generation") != body.generation || event.get::<i64,_>("key_version") != body.key_version { return Err(Error::conflict("Activity idempotency key was used for another report")); }
        return Ok(Json(json!({"environment_id":id,"app_id":app,"last_activity":event.get::<i64,_>("received_at"),"replayed":true})));
    }
    let received = now();
    sqlx::query("INSERT INTO environment_activity_events(environment_id,actor,idempotency_key,app_id,generation,key_version,received_at) VALUES(?,?,?,?,?,?,?)")
        .bind(&id).bind(&actor).bind(k).bind(&app).bind(body.generation).bind(body.key_version).bind(received).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO environment_app_activity(environment_id,app_id,last_activity) VALUES(?,?,?) ON CONFLICT(environment_id,app_id) DO UPDATE SET last_activity=MAX(last_activity,excluded.last_activity)")
        .bind(&id).bind(&app).bind(received).execute(&mut *tx).await?;
    sqlx::query("UPDATE environment_imports SET last_activity=MAX(last_activity,?) WHERE environment_id=? AND app_id=?").bind(received).bind(&id).bind(&app).execute(&mut *tx).await?;
    sqlx::query("UPDATE environments SET last_activity=MAX(last_activity,?) WHERE id=?")
        .bind(received)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"environment_id":id,"app_id":app,"last_activity":received,"replayed":false}),
    ))
}

pub async fn update(
    S(s): S<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Retention>,
) -> Result<Json<Value>> {
    let k = key(&h)?;
    let expected = revision(&h)?;
    if !(1..=36500).contains(&body.idle_days) {
        return Err(Error::bad("idle_days must be between 1 and 36500"));
    }
    let (_, actor) = manage(&s, &h, &id).await?;
    let digest = format!("{id}:{expected}:{}", body.idle_days);
    let mut tx = s.db.begin().await?;
    if let Some(row) = sqlx::query("SELECT request_hash,result FROM operations WHERE plane='production' AND actor=? AND idempotency_key=?").bind(&actor).bind(k).fetch_optional(&mut *tx).await? {
        if row.get::<String,_>("request_hash") != digest { return Err(Error::conflict("Idempotency key was used for another operation")); }
        return Ok(Json(serde_json::from_str(&row.get::<String,_>("result")).map_err(|e|anyhow::anyhow!(e))?));
    }
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE plane='production' AND resource=? AND kind LIKE 'environment.%' AND state='pending')").bind(&id).fetch_one(&mut *tx).await?;
    if pending {
        return Err(Error::conflict(
            "Finish the pending environment operation before changing retention",
        ));
    }
    let changed=sqlx::query("UPDATE environments SET idle_days=?,revision=revision+1 WHERE id=? AND revision=? AND state='ready'").bind(body.idle_days).bind(&id).bind(expected).execute(&mut *tx).await?;
    if changed.rows_affected() != 1 {
        return Err(Error::conflict(
            "Retention requires a ready environment and its current revision",
        ));
    }
    let response = json!({"environment_id":id,"revision":expected+1,"idle_days":body.idle_days});
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,state,result,created_at) VALUES(?,'production',?,?,'retention.configure',?,?,?,'accepted',?,?)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&actor).bind(k).bind(&id).bind(digest).bind(expected+1).bind(response.to_string()).bind(now()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,'production',?,'environment.retention.configure',?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(actor).bind(&id).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(response))
}
pub async fn status(S(s): S<State>, h: HeaderMap, Path(id): Path<String>) -> Result<Json<Value>> {
    manage(&s, &h, &id).await?;
    let mut tx = s.db.begin().await?;
    let result = plan(&mut tx, &id, now()).await?;
    tx.commit().await?;
    Ok(Json(result))
}

/// A consistent plan for UI and the lifecycle scheduler. Reading does not extend activity.
pub async fn plan(db: &mut SqliteConnection, id: &str, at: i64) -> Result<Value> {
    let environment = sqlx::query(
        "SELECT state,idle_days,last_activity,purge_after,revision FROM environments WHERE id=?",
    )
    .bind(id)
    .fetch_one(&mut *db)
    .await?;
    let apps=sqlx::query("SELECT a.app_id,a.created_at,a.effective_config,a.state,p.effective_config AS production_config,t.last_activity FROM applications a LEFT JOIN applications p ON p.plane='production' AND p.app_id=a.app_id LEFT JOIN environment_app_activity t ON t.environment_id=a.plane AND t.app_id=a.app_id WHERE a.plane=? ORDER BY a.app_id").bind(id).fetch_all(&mut *db).await?;
    let mut items = BTreeMap::<String, Value>::new();
    let mut edges = BTreeMap::<String, Vec<String>>::new();
    let mut protected = BTreeSet::new();
    let mut queue = VecDeque::new();
    let mut delete_after = environment
        .get::<i64, _>("last_activity")
        .saturating_add(environment.get::<i64, _>("idle_days").saturating_mul(86400));
    for app in apps {
        let name: String = app.get("app_id");
        let local: Value = serde_json::from_str(
            app.get::<Option<String>, _>("effective_config")
                .as_deref()
                .unwrap_or("{}"),
        )
        .map_err(|e| anyhow::anyhow!(e))?;
        let production: Value = serde_json::from_str(
            app.get::<Option<String>, _>("production_config")
                .as_deref()
                .unwrap_or("{}"),
        )
        .map_err(|e| anyhow::anyhow!(e))?;
        // An imported app's retention belongs to its production org, not the importing tester.
        let idle = production["testing_idle_days"]
            .as_i64()
            .or_else(|| local["testing_idle_days"].as_i64())
            .unwrap_or(30)
            .clamp(1, 36500);
        let last = app
            .get::<Option<i64>, _>("last_activity")
            .unwrap_or(app.get("created_at"));
        let deadline = last.saturating_add(idle.saturating_mul(86400));
        delete_after = delete_after.max(deadline);
        let dependencies = local["app_scope"]["external"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["app_id"].as_str().map(str::to_owned))
            .collect();
        edges.insert(name.clone(), dependencies);
        if deadline > at {
            queue.push_back(name.clone());
        }
        items.insert(name.clone(),json!({"app_id":name,"last_activity":last,"idle_days":idle,"idle_after":deadline,"state":app.get::<String,_>("state")}));
    }
    while let Some(app) = queue.pop_front() {
        if !protected.insert(app.clone()) {
            continue;
        }
        if let Some(dependencies) = edges.get(&app) {
            queue.extend(
                dependencies
                    .iter()
                    .filter(|d| items.contains_key(*d))
                    .cloned(),
            );
        }
    }
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE plane='production' AND resource=? AND kind LIKE 'environment.%' AND state='pending')").bind(id).fetch_one(&mut *db).await?;
    let ready = environment.get::<String, _>("state") == "ready" && !pending;
    for (name, item) in &mut items {
        item["required_by_active_app"] =
            json!(protected.contains(name) && item["idle_after"].as_i64().unwrap() <= at);
        item["eligible_for_retirement"] = json!(
            ready
                && !protected.contains(name)
                && item["idle_after"].as_i64().unwrap() <= at
                && item["state"] == "active"
        );
    }
    Ok(
        json!({"environment_id":id,"revision":environment.get::<i64,_>("revision"),"idle_days":environment.get::<i64,_>("idle_days"),"last_activity":environment.get::<i64,_>("last_activity"),"delete_after":delete_after,"eligible_for_deletion":ready && at>=delete_after && protected.is_empty(),"purge_after":environment.get::<Option<i64>,_>("purge_after"),"operation_pending":pending,"applications":items.into_values().collect::<Vec<_>>()}),
    )
}
