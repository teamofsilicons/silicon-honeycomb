//! Honeycomb hosts discussions and review workflow; IAM accepts security decisions.
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
use std::collections::{BTreeMap, BTreeSet};

pub async fn retry_plan(
    S(s): S<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    key(&h)?;
    let c = context(&s, &h).await?;
    let request = request(&s, &c, &id).await?;
    let app = find_app(&s, &c, &request.get::<String, _>("app_id"), true).await?;
    if revision(&h)? != app.revision || request.get::<i64, _>("revision") != app.revision {
        return Err(Error::conflict(
            "Application revision changed; submit a new publication request",
        ));
    }
    Ok(Json(plan(&s, &c, &id).await?))
}
async fn request(s: &State, c: &Context, id: &str) -> Result<SqliteRow> {
    sqlx::query("SELECT * FROM publication_requests WHERE id=? AND plane=?")
        .bind(id)
        .bind(&c.plane)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)
}
pub(crate) async fn plan(s: &State, c: &Context, id: &str) -> Result<Value> {
    let row = request(s, c, id).await?;
    let app = find_app(s, c, &row.get::<String, _>("app_id"), true).await?;
    if row.get::<i64, _>("revision") != app.revision {
        return Err(Error::conflict(
            "A newer configuration supersedes this publication request",
        ));
    }
    if row.get::<Option<String>, _>("plan_id").is_some() {
        return Ok(json!({"id":id,"state":row.get::<String,_>("state")}));
    }
    let configuration: Value = serde_json::from_str(
        &row.get::<Option<String>, _>("config_snapshot")
            .unwrap_or_else(|| app.config.to_string()),
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let response=s.management.publication_plan(&json!({"request_id":id,"app_id":app.app_id,"configuration_revision":app.revision,"configuration":configuration,"visibility":"public"}),c.token.as_deref().ok_or_else(Error::unauthorized)?,c.environment.as_deref()).await;
    let response = match response {
        Ok(v) => v,
        Err(_) => {
            sqlx::query("UPDATE publication_requests SET state='awaiting_review_plan',error='IAM review planning is unavailable' WHERE id=? AND plan_id IS NULL").bind(id).execute(&s.db).await?;
            return Ok(
                json!({"id":id,"state":"awaiting_review_plan","error":"IAM must confirm the scope review plan before reviewers can decide"}),
            );
        }
    };
    if response["request_id"] != id
        || response["app_id"] != app.app_id
        || response["configuration_revision"] != app.revision
        || response["state"] != "accepted"
    {
        return Err(Error::unavailable(
            "IAM returned a review plan for another revision",
        ));
    }
    let plan_id = response["plan_id"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 255)
        .ok_or_else(|| Error::unavailable("IAM omitted its review plan ID"))?;
    let gates = response["gates"]
        .as_array()
        .ok_or_else(|| Error::unavailable("IAM omitted the scope review gates"))?;
    let mut required = BTreeMap::<String, Vec<String>>::new();
    for gate in gates {
        let provider = gate["provider"]
            .as_str()
            .ok_or_else(|| Error::unavailable("Invalid review provider"))?;
        if provider == "honeycomb" || required.contains_key(provider) {
            return Err(Error::unavailable("Duplicate or invalid review provider"));
        }
        let scopes: Vec<String> = serde_json::from_value(gate["scopes"].clone())
            .map_err(|_| Error::unavailable("Invalid review scopes"))?;
        if scopes.is_empty() || scopes.iter().collect::<BTreeSet<_>>().len() != scopes.len() {
            return Err(Error::unavailable(
                "Review scopes must be distinct and nonempty",
            ));
        }
        for scope in &scopes {
            let declared = if provider == "iam" {
                configuration["app_scope"]["iam"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(scope)))
            } else {
                configuration["app_scope"]["external"]
                    .as_array()
                    .is_some_and(|a| {
                        a.iter()
                            .any(|v| v["app_id"] == provider && v["endpoint_id"] == *scope)
                    })
            };
            if !declared {
                return Err(Error::unavailable(
                    "IAM requested review of an undeclared scope",
                ));
            }
        }
        required.insert(provider.into(), scopes);
    }
    let state = if required.is_empty() {
        "awaiting_validator"
    } else {
        "awaiting_scope_review"
    };
    required.insert("honeycomb".into(), vec![]);
    let mut tx = s.db.begin().await?;
    let updated=sqlx::query("UPDATE publication_requests SET plan_id=?,state=?,error=NULL,config_snapshot=? WHERE id=? AND plan_id IS NULL AND revision=(SELECT revision FROM applications WHERE plane=? AND app_id=?)")
        .bind(plan_id).bind(state).bind(configuration.to_string()).bind(id).bind(&c.plane).bind(&app.app_id).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(Error::conflict(
            "Publication plan changed while IAM was evaluating it",
        ));
    }
    for (provider, scopes) in required {
        sqlx::query("INSERT INTO review_gates(request_id,provider,scopes) VALUES(?,?,?)")
            .bind(id)
            .bind(&provider)
            .bind(json!(scopes).to_string())
            .execute(&mut *tx)
            .await?;
        if provider != "honeycomb" || state == "awaiting_validator" {
            sqlx::query("INSERT OR IGNORE INTO outbox(id,plane,event_key,kind,payload,created_at) VALUES(?,?,?,'publication.review',?,?)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(&c.plane).bind(format!("review:{id}:{provider}"))
                .bind(json!({"app_id":app.app_id,"review_provider":provider}).to_string()).bind(now()).execute(&mut *tx).await?;
        }
        if provider != "iam" && provider != "honeycomb" {
            let config: Option<String> = sqlx::query_scalar(
                "SELECT effective_config FROM applications WHERE plane=? AND app_id=?",
            )
            .bind(&c.plane)
            .bind(&provider)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
            if let Some(message) = config
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .and_then(|v| v["obo_review_message"].as_str().map(str::to_owned))
                .filter(|m| !m.is_empty())
            {
                sqlx::query("INSERT INTO discussions(id,request_id,actor,provider,message,created_at) VALUES(?,?,'provider-review-guidance',?,?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(id).bind(&provider).bind(message).bind(now()).execute(&mut *tx).await?;
            }
        }
    }
    tx.commit().await?;
    Ok(json!({"id":id,"state":state,"plan_id":plan_id}))
}
async fn can_review(s: &State, c: &Context, provider: &str) -> Result<bool> {
    let identity = c.identity()?;
    if provider == "honeycomb" {
        return Ok(identity.validator);
    }
    if provider == "iam" {
        return s
            .management
            .iam_scope_reviewer(c.token.as_deref().unwrap(), c.environment.as_deref())
            .await;
    }
    let org: Option<String> = sqlx::query_scalar(
        "SELECT org_id FROM applications WHERE plane=? AND app_id=? AND effective_revision>0",
    )
    .bind(&c.plane)
    .bind(provider)
    .fetch_optional(&s.db)
    .await?;
    Ok(org.as_ref().is_some_and(|org| identity.admin(org)))
}
pub async fn inbox(S(s): S<State>, h: HeaderMap) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    c.identity()?;
    let rows=sqlx::query("SELECT p.*,g.provider,g.scopes,g.state AS gate_state FROM publication_requests p JOIN review_gates g ON g.request_id=p.id JOIN applications a ON a.plane=p.plane AND a.app_id=p.app_id AND a.revision=p.revision WHERE p.plane=? AND p.state NOT IN ('published','denied') ORDER BY p.created_at,p.id,g.provider")
        .bind(&c.plane).fetch_all(&s.db).await?;
    let mut items = Vec::new();
    let mut authority = BTreeMap::new();
    for row in rows {
        let provider: String = row.get("provider");
        let authorized = if let Some(value) = authority.get(&provider) {
            *value
        } else {
            let value = can_review(&s, &c, &provider).await?;
            authority.insert(provider.clone(), value);
            value
        };
        if !authorized
            || (provider == "honeycomb" && row.get::<String, _>("state") != "awaiting_validator")
        {
            continue;
        }
        items.push(json!({"id":row.get::<String,_>("id"),"app_id":row.get::<String,_>("app_id"),"revision":row.get::<i64,_>("revision"),"provider":provider,"scopes":serde_json::from_str::<Value>(&row.get::<String,_>("scopes")).unwrap_or(Value::Null),"state":row.get::<String,_>("gate_state"),"configuration":serde_json::from_str::<Value>(&row.get::<String,_>("config_snapshot")).unwrap_or(Value::Null)}));
    }
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
pub struct Decision {
    decision: String,
    #[serde(default)]
    reason: String,
}
pub async fn decide(
    S(s): S<State>,
    h: HeaderMap,
    Path((id, provider)): Path<(String, String)>,
    Json(body): Json<Decision>,
) -> Result<(StatusCode, Json<Value>)> {
    let k = key(&h)?;
    let expected = revision(&h)?;
    let c = context(&s, &h).await?;
    if !["approve", "deny"].contains(&body.decision.as_str())
        || body.reason.len() > 10000
        || (body.decision == "deny" && body.reason.trim().is_empty())
    {
        return Err(Error::bad(
            "Use approve or deny; denial requires a reason of 1–10000 characters",
        ));
    }
    let row = request(&s, &c, &id).await?;
    if !can_review(&s, &c, &provider).await? {
        return Err(Error::forbidden());
    }
    let app_id: String = row.get("app_id");
    let current: i64 =
        sqlx::query_scalar("SELECT revision FROM applications WHERE plane=? AND app_id=?")
            .bind(&c.plane)
            .bind(&app_id)
            .fetch_one(&s.db)
            .await?;
    if expected != current || expected != row.get::<i64, _>("revision") {
        return Err(Error::conflict(
            "This review belongs to an older application revision",
        ));
    }
    let plan_id = row
        .get::<Option<String>, _>("plan_id")
        .ok_or_else(|| Error::conflict("IAM has not accepted the review plan"))?;
    let digest=hex::encode(Sha256::digest(json!({"request":id,"provider":provider,"revision":expected,"decision":body.decision,"reason":body.reason}).to_string()));
    let mut tx = s.db.begin().await?;
    let existing=sqlx::query("SELECT id,request_hash,state,request_json FROM operations WHERE plane=? AND actor=? AND idempotency_key=?").bind(&c.plane).bind(&c.identity()?.principal_id).bind(k).fetch_optional(&mut *tx).await?;
    let (op, operation) = if let Some(existing) = existing {
        if existing.get::<String, _>("request_hash") != digest {
            return Err(Error::conflict(
                "Idempotency key already used for a different decision",
            ));
        }
        if existing.get::<String, _>("state") == "accepted" {
            return Ok((
                StatusCode::OK,
                Json(json!({"id":existing.get::<String,_>("id"),"state":"accepted"})),
            ));
        }
        (
            existing.get::<String, _>("id"),
            serde_json::from_str::<Value>(&existing.get::<String, _>("request_json"))
                .map_err(|e| anyhow::anyhow!(e))?,
        )
    } else {
        if provider == "honeycomb" && row.get::<String, _>("state") != "awaiting_validator" {
            return Err(Error::conflict(
                "All provider approvals must be accepted before Honeycomb validation",
            ));
        }
        let gate = sqlx::query("SELECT * FROM review_gates WHERE request_id=? AND provider=?")
            .bind(&id)
            .bind(&provider)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(Error::missing)?;
        if gate.get::<Option<String>, _>("decision_id").is_some() {
            return Err(Error::conflict(
                "This gate already has a decision; retry its original operation or submit a new application revision",
            ));
        }
        let op = uuid::Uuid::new_v4().to_string();
        let scopes: Value = serde_json::from_str(&gate.get::<String, _>("scopes"))
            .map_err(|e| anyhow::anyhow!(e))?;
        let operation = json!({"operation_id":op,"request_id":id,"plan_id":plan_id,"app_id":app_id,"configuration_revision":expected,"provider":provider,"scopes":scopes,"decision":body.decision,"reason":body.reason});
        sqlx::query("INSERT INTO decisions(id,request_id,actor,provider,scopes,decision,reason,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(&op).bind(&id).bind(&c.identity()?.principal_id).bind(&provider).bind(scopes.to_string()).bind(&body.decision).bind(&body.reason).bind(now()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,request_json,created_at) VALUES(?,?,?,?,'review.decision',?,?,?,?,?)").bind(&op).bind(&c.plane).bind(&c.identity()?.principal_id).bind(k).bind(&app_id).bind(digest).bind(expected).bind(operation.to_string()).bind(now()).execute(&mut *tx).await?;
        sqlx::query("UPDATE review_gates SET decision_id=? WHERE request_id=? AND provider=? AND decision_id IS NULL").bind(&op).bind(&id).bind(&provider).execute(&mut *tx).await?;
        (op, operation)
    };
    tx.commit().await?;
    let result = s
        .management
        .review_decision(
            &operation,
            c.token.as_deref().unwrap(),
            c.environment.as_deref(),
        )
        .await;
    if let Ok(receipt) = result
        && receipt["state"] == "accepted"
        && receipt["operation_id"] == op
        && receipt["request_id"] == id
        && receipt["plan_id"] == plan_id
        && receipt["app_id"] == app_id
        && receipt["configuration_revision"] == expected
        && receipt["provider"] == provider
        && receipt["scopes"] == operation["scopes"]
        && receipt["decision"] == body.decision
    {
        let mut tx = s.db.begin().await?;
        let current: i64 =
            sqlx::query_scalar("SELECT revision FROM applications WHERE plane=? AND app_id=?")
                .bind(&c.plane)
                .bind(&app_id)
                .fetch_one(&mut *tx)
                .await?;
        if current != expected {
            return Err(Error::conflict(
                "A newer configuration superseded this review during IAM acceptance",
            ));
        }
        sqlx::query("UPDATE decisions SET state='accepted' WHERE id=?")
            .bind(&op)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE review_gates SET state=? WHERE request_id=? AND provider=? AND decision_id=?",
        )
        .bind(if body.decision == "approve" {
            "approved"
        } else {
            "denied"
        })
        .bind(&id)
        .bind(&provider)
        .bind(&op)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE operations SET state='accepted',error=NULL WHERE id=?")
            .bind(&op)
            .execute(&mut *tx)
            .await?;
        let denied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM review_gates WHERE request_id=? AND state='denied')",
        )
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
        let remaining:i64=sqlx::query_scalar("SELECT COUNT(*) FROM review_gates WHERE request_id=? AND provider!='honeycomb' AND state!='approved'").bind(&id).fetch_one(&mut *tx).await?;
        let validator:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM review_gates WHERE request_id=? AND provider='honeycomb' AND state='approved')").bind(&id).fetch_one(&mut *tx).await?;
        let state = if denied {
            "denied"
        } else if remaining > 0 {
            "awaiting_scope_review"
        } else if !validator {
            "awaiting_validator"
        } else {
            "awaiting_activation"
        };
        sqlx::query("UPDATE publication_requests SET state=? WHERE id=?")
            .bind(state)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        let org: String =
            sqlx::query_scalar("SELECT org_id FROM applications WHERE plane=? AND app_id=?")
                .bind(&c.plane)
                .bind(&app_id)
                .fetch_one(&mut *tx)
                .await?;
        sqlx::query("INSERT OR IGNORE INTO outbox(id,plane,event_key,kind,payload,created_at) VALUES(?,?,?,'publication.status',?,?)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&c.plane).bind(format!("review-decision:{op}"))
            .bind(json!({"app_id":app_id,"org_id":org,"state":state}).to_string()).bind(now()).execute(&mut *tx).await?;
        if state == "awaiting_validator" {
            sqlx::query("INSERT OR IGNORE INTO outbox(id,plane,event_key,kind,payload,created_at) VALUES(?,?,?,'publication.review',?,?)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(&c.plane).bind(format!("review:{id}:honeycomb"))
                .bind(json!({"app_id":app_id,"review_provider":"honeycomb"}).to_string()).bind(now()).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        return Ok((
            StatusCode::OK,
            Json(json!({"id":op,"state":"accepted","publication_state":state})),
        ));
    }
    sqlx::query("UPDATE operations SET error='Awaiting IAM decision acceptance' WHERE id=? AND state='pending'").bind(&op).execute(&s.db).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(
            json!({"id":op,"state":"pending","idempotency_key":k,"message":"Decision saved; waiting for IAM to accept it"}),
        ),
    ))
}

async fn discussion_access(s: &State, c: &Context, id: &str, provider: &str) -> Result<SqliteRow> {
    let row = request(s, c, id).await?;
    let gate: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM review_gates WHERE request_id=? AND provider=?)",
    )
    .bind(id)
    .bind(provider)
    .fetch_one(&s.db)
    .await?;
    if !gate {
        return Err(Error::missing());
    }
    let org: String =
        sqlx::query_scalar("SELECT org_id FROM applications WHERE plane=? AND app_id=?")
            .bind(&c.plane)
            .bind(row.get::<String, _>("app_id"))
            .fetch_one(&s.db)
            .await?;
    let providers_ready:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM review_gates WHERE request_id=? AND provider!='honeycomb' AND state!='approved')").bind(id).fetch_one(&s.db).await?;
    if !c.identity()?.admin(&org)
        && (!can_review(s, c, provider).await? || (provider == "honeycomb" && !providers_ready))
    {
        return Err(Error::forbidden());
    }
    Ok(row)
}
pub async fn detail(
    S(s): S<State>,
    h: HeaderMap,
    Path((id, provider)): Path<(String, String)>,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    let row = discussion_access(&s, &c, &id, &provider).await?;
    let messages=sqlx::query("SELECT id,actor,message,created_at FROM discussions WHERE request_id=? AND (provider=? OR provider IS NULL) ORDER BY created_at,id").bind(&id).bind(&provider).fetch_all(&s.db).await?;
    let gate = sqlx::query(
        "SELECT scopes,state,decision_id FROM review_gates WHERE request_id=? AND provider=?",
    )
    .bind(&id)
    .bind(&provider)
    .fetch_one(&s.db)
    .await?;
    let messages:Vec<Value>=messages.iter().map(|m|json!({"id":m.get::<String,_>("id"),"actor":m.get::<String,_>("actor"),"message":m.get::<String,_>("message")})).collect();
    let mut result = json!({"id":id,"provider":provider,"app_id":row.get::<String,_>("app_id"),"revision":row.get::<i64,_>("revision"),"configuration":serde_json::from_str::<Value>(&row.get::<String,_>("config_snapshot")).unwrap_or(Value::Null),"state":gate.get::<String,_>("state"),"scopes":serde_json::from_str::<Value>(&gate.get::<String,_>("scopes")).unwrap_or(Value::Null),"messages":messages});
    result["can_decide"] = json!(
        can_review(&s, &c, &provider).await?
            && (provider != "honeycomb" || row.get::<String, _>("state") == "awaiting_validator")
    );
    if let Some(decision_id) = gate.get::<Option<String>, _>("decision_id") {
        let decision=sqlx::query("SELECT d.decision,d.reason,d.state,o.idempotency_key,d.actor FROM decisions d JOIN operations o ON o.id=d.id WHERE d.id=?").bind(decision_id).fetch_one(&s.db).await?;
        result["decision"] = json!({"decision":decision.get::<String,_>("decision"),"reason":decision.get::<String,_>("reason"),"state":decision.get::<String,_>("state")});
        if decision.get::<String, _>("actor") == c.identity()?.principal_id {
            result["decision"]["idempotency_key"] =
                json!(decision.get::<String, _>("idempotency_key"));
        }
    }
    Ok(Json(result))
}
#[derive(Deserialize)]
pub struct Reply {
    message: String,
}
pub async fn message(
    S(s): S<State>,
    h: HeaderMap,
    Path((id, provider)): Path<(String, String)>,
    Json(body): Json<Reply>,
) -> Result<Json<Value>> {
    let k = key(&h)?;
    let c = context(&s, &h).await?;
    let row = discussion_access(&s, &c, &id, &provider).await?;
    if body.message.trim().is_empty() || body.message.len() > 10000 {
        return Err(Error::bad("message must contain 1–10000 characters"));
    }
    let actor = &c.identity()?.principal_id;
    let digest = hex::encode(Sha256::digest(
        json!({"provider":provider,"message":body.message}).to_string(),
    ));
    let mut tx = s.db.begin().await?;
    if let Some(existing)=sqlx::query("SELECT id,request_hash FROM discussions WHERE request_id=? AND actor=? AND idempotency_key=?").bind(&id).bind(actor).bind(k).fetch_optional(&mut *tx).await? {
        if existing.get::<String,_>("request_hash")!=digest {return Err(Error::conflict("Idempotency key already used for a different reply"));}
        return Ok(Json(json!({"id":existing.get::<String,_>("id")})));
    }
    let message = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO discussions(id,request_id,actor,provider,message,created_at,idempotency_key,request_hash) VALUES(?,?,?,?,?,?,?,?)")
        .bind(&message).bind(&id).bind(actor).bind(&provider).bind(&body.message).bind(now()).bind(k).bind(digest).execute(&mut *tx).await?;
    let app_id: String = row.get("app_id");
    let org: String =
        sqlx::query_scalar("SELECT org_id FROM applications WHERE plane=? AND app_id=?")
            .bind(&c.plane)
            .bind(&app_id)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("INSERT INTO outbox(id,plane,event_key,kind,payload,created_at) VALUES(?,?,?,'publication.message',?,?)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&c.plane).bind(format!("discussion:{message}"))
        .bind(json!({"app_id":app_id,"org_id":org,"review_providers":[provider]}).to_string()).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":message})))
}
