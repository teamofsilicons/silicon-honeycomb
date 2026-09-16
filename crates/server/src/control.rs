pub(crate) mod application;
use crate::{
    State,
    api::{context, header_value, key, revision},
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    extract::{Path, State as S},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use rand::{Rng, distributions::Alphanumeric};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, sqlite::SqliteRow};

pub async fn drafts(S(s): S<State>, h: HeaderMap, Path(org): Path<String>) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    c.admin(&org)?;
    let rows=sqlx::query("SELECT id,revision,body,updated_at FROM drafts WHERE plane=? AND org_id=? ORDER BY updated_at DESC").bind(&c.plane).bind(org).fetch_all(&s.db).await?;
    let items:Vec<Value>=rows.iter().map(|r|json!({"id":r.get::<String,_>("id"),"revision":r.get::<i64,_>("revision"),"body":serde_json::from_str::<Value>(&r.get::<String,_>("body")).unwrap_or(Value::Null),"updated_at":r.get::<i64,_>("updated_at")})).collect();
    Ok(Json(json!({"items":items})))
}
pub async fn save_draft(
    S(s): S<State>,
    h: HeaderMap,
    Path((org, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    key(&h)?;
    let expected = revision(&h)?;
    let c = context(&s, &h).await?;
    c.admin(&org)?;
    if !body.is_object() || body.to_string().len() > 100000 {
        return Err(Error::bad("Draft must be an object no larger than 100 KiB"));
    }
    if body.get("webhook_secret").is_some() || body.get("app_secret").is_some() {
        return Err(Error::bad(
            "Do not include secrets in draft autosaves; supply webhook_secret only during submission",
        ));
    }
    let mut tx = s.db.begin().await?;
    if expected == 0 {
        sqlx::query(
            "INSERT INTO drafts(plane,org_id,id,revision,body,updated_at) VALUES(?,?,?,1,?,?)",
        )
        .bind(&c.plane)
        .bind(&org)
        .bind(&id)
        .bind(body.to_string())
        .bind(now())
        .execute(&mut *tx)
        .await?;
    } else {
        let updated=sqlx::query("UPDATE drafts SET body=?,revision=revision+1,updated_at=? WHERE plane=? AND org_id=? AND id=? AND revision=?").bind(body.to_string()).bind(now()).bind(&c.plane).bind(&org).bind(&id).bind(expected).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(Error::conflict(
                "Another administrator saved a newer draft; reload before saving",
            ));
        }
    }
    tx.commit().await?;
    Ok(Json(json!({"id":id,"revision":expected+1,"saved":true})))
}
fn environment_value(r: &SqliteRow) -> Value {
    json!({"environment_id":r.get::<String,_>("id"),"org_id":r.get::<String,_>("org_id"),"creator":r.get::<String,_>("creator"),"name":r.get::<String,_>("name"),"description":r.get::<String,_>("description"),"state":r.get::<String,_>("state"),"revision":r.get::<i64,_>("revision"),"generation":r.get::<i64,_>("generation"),"key_version":r.get::<i64,_>("key_version"),"idle_days":r.get::<i64,_>("idle_days"),"last_activity":r.get::<i64,_>("last_activity"),"purge_after":r.get::<Option<i64>,_>("purge_after")})
}
pub(crate) async fn manage(s: &State, h: &HeaderMap, id: &str) -> Result<(SqliteRow, String)> {
    let row = sqlx::query("SELECT * FROM environments WHERE id=?")
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)?;
    if application::presented(h)? {
        let authority = application::authenticate(s, h).await?;
        if !application::owns(&row, &authority) {
            return Err(Error::missing());
        }
        return Ok((row, application::actor(&authority)));
    }
    if let Some(root) = header_value(h, "x-testing-environment-key")? {
        if hex::encode(Sha256::digest(root)) != row.get::<String, _>("key_hash")
            || row.get::<String, _>("state") == "deleted"
        {
            return Err(Error::forbidden());
        }
        return Ok((row, format!("environment-root:{id}")));
    }
    let c = context(s, h).await?;
    let i = c.identity()?;
    let org: String = row.get("org_id");
    if !i.admin(&org) && !(i.member(&org) && i.principal_id == row.get::<String, _>("creator")) {
        return Err(Error::missing());
    }
    Ok((row, i.principal_id.clone()))
}
pub async fn environments(S(s): S<State>, h: HeaderMap) -> Result<Json<Value>> {
    if application::presented(&h)? {
        return application::list(&s, &h).await;
    }
    let c = context(&s, &h).await?;
    let i = c.identity()?;
    if c.plane != "production" {
        return Err(Error::bad(
            "List environments from production; a testing key is confined to its own environment",
        ));
    }
    let rows = sqlx::query("SELECT * FROM environments ORDER BY created_at DESC,id")
        .fetch_all(&s.db)
        .await?;
    let items: Vec<Value> = rows
        .iter()
        .filter(|r| i.member(&r.get::<String, _>("org_id")))
        .map(|r| {
            let mut v = environment_value(r);
            v["can_manage"] = json!(
                i.admin(&r.get::<String, _>("org_id"))
                    || i.principal_id == r.get::<String, _>("creator")
            );
            v
        })
        .collect();
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
pub struct CreateEnvironment {
    #[serde(default)]
    org_id: String,
    name: String,
    #[serde(default)]
    description: String,
    testing_key: Option<String>,
    iam_test_key: Option<String>,
}
impl CreateEnvironment {
    fn attachment_key(&self) -> Result<Option<&str>> {
        if self.testing_key.is_some()
            && self.iam_test_key.is_some()
            && self.testing_key != self.iam_test_key
        {
            return Err(Error::bad("testing_key and iam_test_key must match"));
        }
        let key = self.testing_key.as_deref().or(self.iam_test_key.as_deref());
        if key.is_some_and(|k| k.len() != 32 || !k.bytes().all(|b| b.is_ascii_alphanumeric())) {
            return Err(Error::bad(
                "testing_key must contain exactly 32 alphanumeric characters",
            ));
        }
        Ok(key)
    }
}
pub async fn create_environment(
    S(s): S<State>,
    h: HeaderMap,
    Json(body): Json<CreateEnvironment>,
) -> Result<Response> {
    let (status, value) = create_environment_inner(s, h, body).await?;
    Ok((status, [("cache-control", "no-store")], value).into_response())
}
async fn create_environment_inner(
    s: State,
    h: HeaderMap,
    body: CreateEnvironment,
) -> Result<(StatusCode, Json<Value>)> {
    body.attachment_key()?;
    if application::presented(&h)? {
        return application::create(&s, &h, body).await;
    }
    if body.attachment_key()?.is_some() {
        return Err(Error::bad(
            "Application credentials are required to attach an app",
        ));
    }
    let k = key(&h)?;
    let c = context(&s, &h).await?;
    let actor = c.identity()?;
    if c.plane != "production" || !actor.member(&body.org_id) {
        return Err(Error::forbidden());
    }
    if body.name.trim().is_empty() || body.name.len() > 100 || body.description.len() > 10000 {
        return Err(Error::bad(
            "name requires 1–100 characters; description allows up to 10000",
        ));
    }
    let digest=hex::encode(Sha256::digest(json!({"kind":"environment.create","org":body.org_id,"name":body.name,"description":body.description}).to_string()));
    if let Some(op)=sqlx::query("SELECT request_hash,resource FROM operations WHERE plane='production' AND actor=? AND idempotency_key=?").bind(&actor.principal_id).bind(k).fetch_optional(&s.db).await?{
  if op.get::<String,_>("request_hash")!=digest{return Err(Error::conflict("Idempotency key already used for different input"));}
  let row=sqlx::query("SELECT * FROM environments WHERE id=?").bind(op.get::<String,_>("resource")).fetch_one(&s.db).await?;return Ok((StatusCode::ACCEPTED,Json(environment_value(&row))));
 }
    let id = uuid::Uuid::new_v4().to_string();
    let op = uuid::Uuid::new_v4().to_string();
    let root: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let encrypted = s.encrypt(&root)?;
    let mut tx = s.db.begin().await?;
    sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,created_at,last_activity) VALUES(?,?,?,?,?,?,?,?,?)").bind(&id).bind(&body.org_id).bind(&actor.principal_id).bind(&body.name).bind(&body.description).bind(encrypted).bind(hex::encode(Sha256::digest(&root))).bind(now()).bind(now()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,'production',?,?,'environment.prepare',?,?,1,?)").bind(&op).bind(&actor.principal_id).bind(k).bind(&id).bind(digest).bind(now()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO environment_services(environment_id,app_id,source_revision,snapshot,state,operation_id,generation) VALUES(?, ?,0,'{}','pending',?,1)").bind(&id).bind(&s.iam_app_id).bind(&op).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO environment_services(environment_id,app_id,source_revision,snapshot,state,operation_id,generation) VALUES(?,?,0,'{}','pending',?,1)").bind(&id).bind(&s.app_id).bind(&op).execute(&mut *tx).await?;
    tx.commit().await?;
    let mut result =
        crate::lifecycle::coordinate(&s, &id, &op, c.token.as_deref().unwrap()).await?;
    result["org_id"] = json!(body.org_id);
    result["name"] = json!(body.name);
    Ok((StatusCode::ACCEPTED, Json(result)))
}

pub async fn environment(
    S(s): S<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let (row, _) = manage(&s, &h, &id).await?;
    let mut v = environment_value(&row);
    let services=sqlx::query("SELECT app_id,state,error,operation_id FROM environment_services WHERE environment_id=? ORDER BY app_id").bind(&id).fetch_all(&s.db).await?;
    v["services"]=json!(services.iter().map(|r|json!({"app_id":r.get::<String,_>("app_id"),"state":r.get::<String,_>("state"),"error":r.get::<Option<String>,_>("error"),"operation_id":r.get::<String,_>("operation_id")})).collect::<Vec<_>>());
    let imports=sqlx::query("SELECT app_id,source_revision,snapshot FROM environment_imports WHERE environment_id=? ORDER BY app_id").bind(&id).fetch_all(&s.db).await?;
    v["imports"]=json!(imports.iter().map(|r| {
        let snapshot:Value=serde_json::from_str(&r.get::<String,_>("snapshot")).unwrap_or(Value::Null);
        json!({"app_id":r.get::<String,_>("app_id"),"source_revision":r.get::<i64,_>("source_revision"),"selected_release":snapshot["selected_release"]})
    }).collect::<Vec<_>>());
    v["operation_pending"]=json!(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM operations WHERE plane='production' AND resource=? AND kind LIKE 'environment.%' AND state='pending')").bind(&id).fetch_one(&s.db).await?);
    Ok(Json(v))
}
pub async fn environment_key(
    S(s): S<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    key(&h)?;
    let (row, actor) = manage(&s, &h, &id).await?;
    sqlx::query("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,'production',?,'environment.key.read',?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(actor).bind(&id).bind(now()).execute(&s.db).await?;
    Ok(Json(
        json!({"environment_id":id,"testing_key":s.decrypt(&row.get::<String,_>("encrypted_key"))?}),
    ))
}
pub async fn environment_action(
    S(s): S<State>,
    h: HeaderMap,
    Path((id, action)): Path<(String, String)>,
) -> Result<Response> {
    let (status, value) = environment_action_inner(s, h, id, action).await?;
    Ok((status, [("cache-control", "no-store")], value).into_response())
}
async fn environment_action_inner(
    s: State,
    h: HeaderMap,
    id: String,
    action: String,
) -> Result<(StatusCode, Json<Value>)> {
    let k = key(&h)?;
    let expected = revision(&h)?;
    let (row, actor) = manage(&s, &h, &id).await?;
    if actor.starts_with("environment-root:")
        && matches!(action.as_str(), "delete" | "restore" | "purge")
    {
        return Err(Error::new(
            StatusCode::FORBIDDEN,
            "environment_manager_required",
            "Sign in as an environment manager to delete, restore or permanently remove this environment.",
        ));
    }
    if action == "retry" {
        if row.get::<i64, _>("revision") != expected {
            return Err(Error::conflict("Environment revision changed"));
        }
        let operation:String=sqlx::query_scalar("SELECT id FROM operations WHERE plane='production' AND resource=? AND revision=? AND kind LIKE 'environment.%' ORDER BY created_at DESC LIMIT 1").bind(&id).bind(expected).fetch_one(&s.db).await?;
        return Ok((
            StatusCode::ACCEPTED,
            Json(application::coordinate(&s, &h, &id, &operation).await?),
        ));
    }
    let desired = match action.as_str() {
        "rotate-key" => "rotating",
        "clean" => "cleaning",
        "delete" => "deleting",
        "restore" => "restoring",
        "purge" => "purging",
        _ => {
            return Err(Error::bad(
                "Supported actions: rotate-key, clean, delete, restore, retry; purge is available after retention expires",
            ));
        }
    };
    let digest = hex::encode(Sha256::digest(format!("{id}:{action}:{expected}")));
    if let Some(o)=sqlx::query("SELECT id,request_hash,state FROM operations WHERE plane='production' AND actor=? AND idempotency_key=?").bind(&actor).bind(k).fetch_optional(&s.db).await?{if o.get::<String,_>("request_hash")!=digest{return Err(Error::conflict("Idempotency key already used for different input"));}return Ok((StatusCode::ACCEPTED,Json(json!({"operation_id":o.get::<String,_>("id"),"state":o.get::<String,_>("state")}))));}
    if row.get::<i64, _>("revision") != expected {
        return Err(Error::conflict("Environment revision changed"));
    }
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE plane='production' AND resource=? AND kind LIKE 'environment.%' AND state='pending')")
        .bind(&id).fetch_one(&s.db).await?;
    if pending {
        return Err(Error::conflict(
            "Finish or retry the pending environment operation first",
        ));
    }
    let state: String = row.get("state");
    if action == "restore" {
        if state != "deleted"
            || row
                .get::<Option<i64>, _>("purge_after")
                .is_none_or(|deadline| deadline <= now())
        {
            return Err(Error::conflict(
                "Only a deleted environment within its recovery window can be restored",
            ));
        }
    } else if action == "purge" {
        if state != "deleted"
            || row
                .get::<Option<i64>, _>("purge_after")
                .is_none_or(|deadline| deadline > now())
        {
            return Err(Error::conflict(
                "Permanent purge requires an expired recovery window",
            ));
        }
    } else if state != "ready" {
        return Err(Error::conflict(
            "Wait for the current lifecycle operation to finish before starting another",
        ));
    }
    let op = uuid::Uuid::new_v4().to_string();
    let mut tx = s.db.begin().await?;
    let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE state='pending' AND ((plane='production' AND resource=? AND kind LIKE 'environment.%') OR (plane=? AND kind IN ('configure','secret.rotate','webhook.approve','webhook.rotate'))))")
        .bind(&id).bind(&id).fetch_one(&mut *tx).await?;
    if pending {
        return Err(Error::conflict(
            "Finish or retry pending environment and test application operations first",
        ));
    }
    let rotated: Option<String> = if action == "rotate-key" {
        Some(
            rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(32)
                .map(char::from)
                .collect(),
        )
    } else {
        None
    };
    if let Some(key) = &rotated {
        sqlx::query("UPDATE environments SET encrypted_key=?,key_hash=?,key_version=key_version+1 WHERE id=? AND revision=?")
            .bind(s.encrypt(key)?).bind(hex::encode(Sha256::digest(key))).bind(&id).bind(expected).execute(&mut *tx).await?;
    }
    let updated=sqlx::query("UPDATE environments SET state=?,revision=revision+1,generation=generation+? WHERE id=? AND revision=?").bind(desired).bind(if action=="clean"{1}else{0}).bind(&id).bind(expected).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(Error::conflict("Environment revision changed"));
    }
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,'production',?,?,?,?,?,?,?)").bind(&op).bind(actor).bind(k).bind(format!("environment.{action}")).bind(&id).bind(digest).bind(expected+1).bind(now()).execute(&mut *tx).await?;
    if rotated.is_some() {
        // IAM authorizes rotation with the previous root while participants accept the new root.
        // Keep the old key encrypted so an interrupted operation can replay its original authority.
        sqlx::query("UPDATE operations SET request_json=? WHERE id=?")
            .bind(
                json!({"previous_encrypted_key": row.get::<String, _>("encrypted_key")})
                    .to_string(),
            )
            .bind(&op)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "UPDATE environment_services SET state='pending',operation_id=? WHERE environment_id=?",
    )
    .bind(&op)
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let mut result = application::coordinate(&s, &h, &id, &op).await?;
    if let Some(key) = rotated {
        result["testing_key"] = json!(key);
    }
    Ok((StatusCode::ACCEPTED, Json(result)))
}

pub async fn webhook(S(s): S<State>, h: HeaderMap, body: axum::body::Bytes) -> Result<Json<Value>> {
    use silicon_iam_client::{
        EnvironmentKey, WebhookSecret, WebhookSecretKeyring, WebhookVerifier,
    };
    let verifier = WebhookVerifier::new(
        WebhookSecretKeyring::new(
            1,
            WebhookSecret::new(s.webhook_secret.clone())
                .map_err(|_| Error::unavailable("Webhook signing secret is not configured"))?,
        )
        .map_err(|_| Error::unavailable("Webhook keyring is invalid"))?,
    );
    let verified = verifier
        .verify(&h, &body)
        .map_err(|_| Error::unauthorized())?;
    let event = serde_json::to_value(verified.event()).map_err(|e| anyhow::anyhow!(e))?;
    let plane = if verified.is_testing() {
        let raw: Value =
            serde_json::from_slice(&body).map_err(|_| Error::bad("Invalid webhook body"))?;
        let metadata = &raw["test"]["metadata"];
        // SDK 1.9 accepts additional aggregate fields but rejects extra metadata fields.
        let id = metadata["environment_id"]
            .as_str()
            .or_else(|| metadata["aggregate"]["environment_id"].as_str())
            .ok_or_else(|| Error::bad("Test webhook requires environment_id"))?;
        let row = sqlx::query("SELECT encrypted_key,generation,state FROM environments WHERE id=?")
            .bind(id)
            .fetch_optional(&s.db)
            .await?
            .ok_or_else(Error::missing)?;
        let expected = EnvironmentKey::new(s.decrypt(&row.get::<String, _>("encrypted_key"))?)
            .map_err(|_| Error::bad("Invalid environment key"))?;
        verified
            .verify_testing_environment(&expected)
            .map_err(|_| Error::unauthorized())?;
        if metadata["generation"]
            .as_i64()
            .or_else(|| metadata["aggregate"]["generation"].as_i64())
            != Some(row.get("generation"))
            || row.get::<String, _>("state") != "ready"
        {
            return Err(Error::conflict(
                "Webhook belongs to an inactive or stale testing generation",
            ));
        }
        id.to_owned()
    } else {
        "production".into()
    };
    let inserted = crate::reconciliation::receive(&s, &plane, &event).await?;
    // Authentication is introspected live; no cached membership grant survives logout.
    Ok(Json(json!({"received":true,"duplicate":!inserted})))
}
