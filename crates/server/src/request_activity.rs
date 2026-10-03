//! Read-only inbox summaries and exact-version, account-isolated read markers.
use crate::{
    State,
    api::{Context, context, find_app, key},
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    extract::{Path, Query, State as S},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;

pub(crate) async fn decorate(
    s: &State,
    c: &Context,
    item: &mut Value,
    view: &str,
    provider: &str,
) -> Result<()> {
    let id = item["id"]
        .as_str()
        .ok_or_else(|| Error::unavailable("Request has no identity"))?
        .to_owned();
    let row=sqlx::query("SELECT p.state,p.error,p.created_at,activity.updated_at,p.activation_operation,a.revision AS current_revision FROM publication_requests p JOIN publication_activity activity ON activity.request_id=p.id JOIN applications a ON a.plane=p.plane AND a.app_id=p.app_id WHERE p.id=? AND p.plane=?")
  .bind(&id).bind(&c.plane).fetch_one(&s.db).await?;
    let messages=sqlx::query("SELECT id,actor,created_at FROM discussions WHERE request_id=? AND (?='sent' OR provider IS NULL OR provider=?) ORDER BY created_at,id")
  .bind(&id).bind(view).bind(provider).fetch_all(&s.db).await?;
    let gates=sqlx::query("SELECT provider,state,decision_id FROM review_gates WHERE request_id=? AND (?='sent' OR provider=?) ORDER BY provider")
  .bind(&id).bind(view).bind(provider).fetch_all(&s.db).await?;
    let activation = if let Some(operation) = row.get::<Option<String>, _>("activation_operation") {
        sqlx::query("SELECT state,error FROM operations WHERE id=?").bind(operation).fetch_optional(&s.db).await?
   .map(|r|json!({"state":r.get::<String,_>("state"),"error":r.get::<Option<String>,_>("error")}))
    } else {
        None
    };
    let fingerprint = json!({"id":id,"view":view,"provider":provider,"current_revision":row.get::<i64,_>("current_revision"),"state":row.get::<String,_>("state"),"error":row.get::<Option<String>,_>("error"),"activation":activation,
 "messages":messages.iter().map(|m|json!([m.get::<String,_>("id"),m.get::<String,_>("actor")])).collect::<Vec<_>>(),
 "gates":gates.iter().map(|g|json!([g.get::<String,_>("provider"),g.get::<String,_>("state"),g.get::<Option<String>,_>("decision_id")])).collect::<Vec<_>>()});
    let version = hex::encode(Sha256::digest(fingerprint.to_string().as_bytes()));
    let read:Option<String>=sqlx::query_scalar("SELECT activity_version FROM request_reads WHERE plane=? AND actor=? AND request_id=? AND view=? AND provider=?")
  .bind(&c.plane).bind(&c.identity()?.principal_id).bind(&id).bind(view).bind(provider).fetch_optional(&s.db).await?;
    item["activity_version"] = json!(version);
    item["unread"] = json!(read.as_deref() != Some(&version));
    item["created_at"] = json!(row.get::<i64, _>("created_at"));
    item["updated_at"] = json!(row.get::<i64, _>("updated_at"));
    item["message_count"] = json!(messages.len());
    Ok(())
}
#[derive(Deserialize)]
pub struct SentQuery {
    #[serde(default = "one")]
    page: u32,
    #[serde(default = "page_size")]
    per_page: u32,
}
fn one() -> u32 {
    1
}
fn page_size() -> u32 {
    50
}
pub async fn sent(S(s): S<State>, h: HeaderMap, Query(q): Query<SentQuery>) -> Result<Json<Value>> {
    if q.page == 0 || !(1..=100).contains(&q.per_page) {
        return Err(Error::bad("Page starts at 1; per_page must be 1–100"));
    }
    let c = context(&s, &h).await?;
    c.identity()?;
    let (items, partial) = sent_items(&s, &c).await?;
    let total = items.len();
    let offset = u64::from(q.page - 1) * u64::from(q.per_page);
    let items = items
        .into_iter()
        .skip(usize::try_from(offset).unwrap_or(usize::MAX))
        .take(q.per_page as usize)
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"items":items,"page":q.page,"per_page":q.per_page,"total":total,"partial":partial}),
    ))
}
async fn sent_items(s: &State, c: &Context) -> Result<(Vec<Value>, bool)> {
    let rows=sqlx::query("SELECT p.id,p.app_id,a.org_id FROM publication_requests p JOIN publication_activity activity ON activity.request_id=p.id JOIN applications a ON a.plane=p.plane AND a.app_id=p.app_id WHERE p.plane=? ORDER BY activity.updated_at DESC,p.created_at DESC,p.id DESC")
  .bind(&c.plane).fetch_all(&s.db).await?;
    let mut apps = std::collections::BTreeMap::new();
    let mut histories = std::collections::BTreeMap::new();
    let mut items = Vec::new();
    let mut partial = false;
    for row in rows {
        let org: String = row.get("org_id");
        if !c.identity()?.admin(&org) {
            continue;
        }
        let app_id: String = row.get("app_id");
        let id: String = row.get("id");
        if !apps.contains_key(&app_id) {
            match find_app(s, c, &app_id, true).await {
                Ok(app) => {
                    apps.insert(
                        app_id.clone(),
                        serde_json::to_value(app).map_err(|e| anyhow::anyhow!(e))?,
                    );
                }
                Err(_) => {
                    partial = true;
                    continue;
                }
            }
            let history = match crate::api::publication_items(s, c, &app_id, false).await {
                Ok(items) => items,
                Err(_) => {
                    partial = true;
                    continue;
                }
            };
            for item in history {
                if let Some(id) = item["id"].as_str() {
                    histories.insert(id.to_owned(), item);
                }
            }
        }
        if let Some(mut item) = histories.remove(&id) {
            item["app"] = apps[&app_id].clone();
            items.push(item);
        }
    }
    Ok((items, partial))
}
pub async fn summary(S(s): S<State>, h: HeaderMap) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    c.identity()?;
    let (received, received_partial) = crate::reviews::inbox_items(&s, &c, true).await?;
    let (sent, sent_partial) = sent_items(&s, &c).await?;
    let pending = received.iter().filter(|r| r["can_decide"] == true).count();
    Ok(Json(
        json!({"received_pending":pending,"received_unread":received.iter().filter(|r|r["unread"]==true).count(),
 "sent_unread":sent.iter().filter(|r|r["unread"]==true).count(),"partial":received_partial||sent_partial}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadInput {
    view: String,
    provider: Option<String>,
    activity_version: String,
}
pub async fn mark_read(
    S(s): S<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ReadInput>,
) -> Result<Json<Value>> {
    key(&h)?;
    let c = context(&s, &h).await?;
    let actor = &c.identity()?.principal_id;
    if !matches!(input.view.as_str(), "sent" | "received")
        || input.activity_version.len() != 64
        || !input
            .activity_version
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err(Error::bad(
            "Select a request view and its current activity_version",
        ));
    }
    let row = sqlx::query("SELECT * FROM publication_requests WHERE id=? AND plane=?")
        .bind(&id)
        .bind(&c.plane)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)?;
    let provider = if input.view == "sent" {
        if input.provider.as_ref().is_some_and(|v| !v.is_empty()) {
            return Err(Error::bad("Sent requests have no provider"));
        }
        find_app(&s, &c, &row.get::<String, _>("app_id"), true).await?;
        String::new()
    } else {
        let provider = input
            .provider
            .filter(|p| !p.is_empty() && p.len() <= 128)
            .ok_or_else(|| Error::bad("A received request requires its provider"))?;
        let gate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM review_gates WHERE request_id=? AND provider=?)",
        )
        .bind(&id)
        .bind(&provider)
        .fetch_one(&s.db)
        .await?;
        if !gate || !crate::reviews::can_review(&s, &c, &row, &provider).await? {
            return Err(Error::forbidden());
        }
        provider
    };
    sqlx::query("INSERT INTO request_reads(plane,actor,request_id,view,provider,activity_version,read_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(plane,actor,request_id,view,provider) DO UPDATE SET activity_version=excluded.activity_version,read_at=excluded.read_at")
  .bind(&c.plane).bind(actor).bind(&id).bind(&input.view).bind(provider).bind(input.activity_version).bind(now()).execute(&s.db).await?;
    Ok(Json(json!({"read":true})))
}
