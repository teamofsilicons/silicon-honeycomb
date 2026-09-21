//! Explicit, revision-pinned dependency imports. Root keys confer no production membership.
use crate::{
    State,
    api::{context, key, revision},
    control::manage,
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    extract::{Path, State as S},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::{BTreeMap, VecDeque};

#[derive(Deserialize, Serialize)]
pub struct Import {
    pub(crate) app_id: String,
    #[serde(default)]
    pub(crate) release: Option<String>,
    #[serde(default)]
    pub(crate) refresh: bool,
}

pub async fn import(
    S(s): S<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Import>,
) -> Result<Response> {
    let (status, value) = import_request(s, h, id, body).await?;
    Ok((status, [("cache-control", "no-store")], value).into_response())
}
async fn import_request(
    s: State,
    h: HeaderMap,
    id: String,
    body: Import,
) -> Result<(StatusCode, Json<Value>)> {
    if crate::control::application::presented(&h)? {
        let authority = crate::control::application::authenticate(&s, &h).await?;
        return import_application(s, h, id, body, &authority).await;
    }
    import_inner(s, h, id, body, None).await
}
pub(crate) async fn import_application(
    s: State,
    h: HeaderMap,
    id: String,
    body: Import,
    authority: &crate::integration::TestingApplicationAuthority,
) -> Result<(StatusCode, Json<Value>)> {
    if body.app_id != authority.identity.app_id {
        return Err(Error::forbidden());
    }
    import_inner(s, h, id, body, Some(authority)).await
}
async fn import_inner(
    s: State,
    h: HeaderMap,
    id: String,
    body: Import,
    application: Option<&crate::integration::TestingApplicationAuthority>,
) -> Result<(StatusCode, Json<Value>)> {
    let operation_key = key(&h)?;
    let expected = revision(&h)?;
    let (environment, actor) = if let Some(authority) = application {
        let environment = sqlx::query("SELECT * FROM environments WHERE id=?")
            .bind(&id)
            .fetch_optional(&s.db)
            .await?
            .ok_or_else(Error::missing)?;
        let attachment = authority.attachment_key.as_deref().is_some_and(|key| {
            hex::encode(Sha256::digest(key)) == environment.get::<String, _>("key_hash")
                && environment.get::<String, _>("state") == "ready"
        });
        if !crate::control::application::owns(&environment, authority) && !attachment {
            return Err(Error::forbidden());
        }
        (environment, crate::control::application::actor(authority))
    } else {
        manage(&s, &h, &id).await?
    };
    // Authenticate any supplied user token in production. A root key is only
    // environment authority and cannot make a private dependency discoverable.
    let mut production_headers = h.clone();
    production_headers.remove("x-testing-environment-key");
    let c = if application.is_none() {
        Some(context(&s, &production_headers).await?)
    } else {
        None
    };
    let digest = hex::encode(Sha256::digest(
        json!({"kind":"environment.import","environment":id,"revision":if application.is_some() {None} else {Some(expected)},"body":body})
            .to_string(),
    ));
    let mut tx = s.db.begin().await?;
    let configuring: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE plane=? AND state='pending' AND kind IN ('configure','secret.rotate'))")
        .bind(&id).fetch_one(&mut *tx).await?;
    if configuring {
        return Err(Error::conflict(
            "Finish pending test application configuration or credential operations before importing applications",
        ));
    }
    if let Some(op)=sqlx::query("SELECT id,request_hash,state FROM operations WHERE plane='production' AND actor=? AND idempotency_key=?")
        .bind(&actor).bind(operation_key).fetch_optional(&mut *tx).await? {
        if op.get::<String,_>("request_hash")!=digest {return Err(Error::conflict("Idempotency key already used for different input"));}
        if let Some(authority) = application {
            let operation: String = op.get("id");
            drop(tx);
            let result = crate::lifecycle::coordinate_application(&s, &id, &operation, authority).await?;
            return Ok((StatusCode::ACCEPTED, Json(result)));
        }
        return Ok((StatusCode::ACCEPTED,Json(json!({"operation_id":op.get::<String,_>("id"),"state":op.get::<String,_>("state")}))));
    }
    if environment.get::<i64, _>("revision") != expected
        || environment.get::<String, _>("state") != "ready"
    {
        return Err(Error::conflict(
            "Import requires a ready environment and its current revision",
        ));
    }
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE plane='production' AND resource=? AND kind LIKE 'environment.%' AND state='pending')")
        .bind(&id).fetch_one(&mut *tx).await?;
    if pending {
        return Err(Error::conflict(
            "Finish or retry the pending environment operation first",
        ));
    }
    let mut queue = VecDeque::from([body.app_id.clone()]);
    let mut visited = BTreeMap::<String, Value>::new();
    let mut changes = BTreeMap::<String, Value>::new();
    while let Some(app) = queue.pop_front() {
        if visited.contains_key(&app) {
            continue;
        }
        let source =
            sqlx::query("SELECT * FROM applications WHERE plane='production' AND app_id=?")
                .bind(&app)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(Error::missing)?;
        if source.get::<String, _>("visibility") != "public"
            && !application.map_or_else(
                || {
                    c.as_ref()
                        .is_some_and(|c| c.member(&source.get::<String, _>("org_id")))
                },
                |a| a.identity.org_id == source.get::<String, _>("org_id"),
            )
        {
            return Err(Error::missing());
        }
        if source.get::<i64, _>("effective_revision") <= 0 {
            return Err(Error::conflict(
                "An import dependency has no accepted configuration",
            ));
        }
        let existing=sqlx::query("SELECT source_revision,snapshot FROM environment_imports WHERE environment_id=? AND app_id=?")
            .bind(&id).bind(&app).fetch_optional(&mut *tx).await?;
        let pinned = existing.filter(|r| r.get::<i64, _>("source_revision") > 0);
        let snapshot = if let Some(pinned) = pinned.as_ref().filter(|_| !body.refresh) {
            let snapshot: Value = serde_json::from_str(&pinned.get::<String, _>("snapshot"))
                .map_err(|e| anyhow::anyhow!(e))?;
            if app == body.app_id
                && body
                    .release
                    .as_ref()
                    .is_some_and(|v| snapshot["selected_release"] != *v)
            {
                return Err(Error::conflict(
                    "Changing a pinned release requires refresh=true",
                ));
            }
            snapshot
        } else {
            let configuration: Value = serde_json::from_str(
                &source
                    .get::<Option<String>, _>("effective_config")
                    .ok_or_else(|| Error::conflict("Accepted configuration is missing"))?,
            )
            .map_err(|e| anyhow::anyhow!(e))?;
            let versions: Vec<String> = sqlx::query_scalar(
                "SELECT version FROM releases WHERE plane='production' AND channel='prod' AND app_id=?",
            )
            .bind(&app)
            .fetch_all(&mut *tx)
            .await?;
            let selected = if app == body.app_id && body.release.is_some() {
                let selected = body.release.clone().unwrap();
                if !versions.contains(&selected) {
                    return Err(Error::bad(
                        "Selected release does not exist for the imported application",
                    ));
                }
                Some(selected)
            } else {
                versions
                    .into_iter()
                    .filter_map(|v| semver::Version::parse(&v).ok())
                    .max()
                    .map(|v| v.to_string())
            };
            let local: Option<i64> =
                sqlx::query_scalar("SELECT revision FROM applications WHERE plane=? AND app_id=?")
                    .bind(&id)
                    .bind(&app)
                    .fetch_optional(&mut *tx)
                    .await?;
            if local.is_some() && pinned.is_none() {
                return Err(Error::conflict(
                    "An existing test-only application cannot be replaced by an import",
                ));
            }
            let snapshot = json!({"app_id":app,"org_id":source.get::<String,_>("org_id"),"source_revision":source.get::<i64,_>("effective_revision"),"source_iam_revision":source.get::<i64,_>("iam_revision"),"source_visibility":source.get::<String,_>("visibility"),"configuration_revision":local.unwrap_or(0)+1,"configuration":configuration,"visibility":"private","selected_release":selected});
            changes.insert(app.clone(), snapshot.clone());
            snapshot
        };
        if let Some(dependencies) = snapshot["configuration"]["app_scope"]["external"].as_array() {
            for dependency in dependencies {
                let target = dependency["app_id"].as_str().ok_or_else(|| {
                    Error::conflict("Accepted dependency definition has no app_id")
                })?;
                queue.push_back(target.to_owned());
            }
        }
        visited.insert(app, snapshot);
    }
    if changes.is_empty() {
        drop(tx);
        if let Some(authority) = application {
            crate::control::application::link(&s, &id, authority, "ready").await?;
        }
        return Ok((
            StatusCode::OK,
            Json(
                json!({"environment_id":id,"state":"ready","revision":expected,"imports":visited.keys().collect::<Vec<_>>(),"unchanged":true}),
            ),
        ));
    }
    let operation = uuid::Uuid::new_v4().to_string();
    // Preserve the ready state: importing additional apps must not disable existing apps.
    let updated=sqlx::query("UPDATE environments SET revision=revision+1,last_activity=? WHERE id=? AND revision=? AND state='ready'")
        .bind(now()).bind(&id).bind(expected).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(Error::conflict("Environment revision changed"));
    }
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,'production',?,?,'environment.import',?,?,?,?)")
        .bind(&operation).bind(&actor).bind(operation_key).bind(&id).bind(digest).bind(expected+1).bind(now()).execute(&mut *tx).await?;
    let imports: Vec<Value> = changes.values().cloned().collect();
    // IAM prepares all isolated authentication records before application services.
    let mut participants = changes;
    let graph: Vec<Value> = visited.values().cloned().collect();
    let refresh_app_ids: Vec<String> = if body.refresh {
        participants.keys().cloned().collect()
    } else {
        vec![]
    };
    participants.insert(s.iam_app_id.clone(), json!({"imports":imports,"graph":graph,"root_app_id":body.app_id,"refresh_app_ids":refresh_app_ids}));
    participants
        .entry(s.app_id.clone())
        .or_insert_with(|| json!({}));
    for (app, snapshot) in participants {
        sqlx::query("INSERT INTO environment_services(environment_id,app_id,source_revision,snapshot,state,operation_id,generation) VALUES(?,?,?,?,'pending',?,?) ON CONFLICT(environment_id,app_id) DO UPDATE SET source_revision=excluded.source_revision,snapshot=excluded.snapshot,state='pending',operation_id=excluded.operation_id,error=NULL,receipt=NULL")
            .bind(&id).bind(app).bind(snapshot["source_revision"].as_i64().unwrap_or(0)).bind(snapshot.to_string()).bind(&operation).bind(environment.get::<i64,_>("generation")).execute(&mut *tx).await?;
    }
    if let Some(authority) = application {
        sqlx::query("INSERT INTO environment_application_links(environment_id,application_id,app_id,last_activity,state) VALUES(?,?,?,?,'pending') ON CONFLICT(environment_id,application_id) DO UPDATE SET last_activity=excluded.last_activity,state='pending'")
            .bind(&id).bind(&authority.identity.application_id).bind(&authority.identity.app_id).bind(now()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    let result = if let Some(authority) = application {
        crate::lifecycle::coordinate_application(&s, &id, &operation, authority).await?
    } else {
        crate::lifecycle::coordinate(
            &s,
            &id,
            &operation,
            c.as_ref().and_then(|c| c.token.as_deref()).unwrap_or(""),
        )
        .await?
    };
    Ok((StatusCode::ACCEPTED, Json(result)))
}
