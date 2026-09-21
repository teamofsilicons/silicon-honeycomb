//! Organization catalog access, including private apps, under current user authority.
use crate::{
    State,
    api::{Context, context, header_value},
    auth::Identity,
    error::{Error, Result},
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State as S},
    http::HeaderMap,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

pub const ENDPOINT_ID: &str = "honeycomb.apps.list";
pub const ENDPOINT_PATH: &str = "/api/v1/obo/apps/list";

/// This is also the definition operators synchronize into Honeycomb's IAM registration.
pub fn endpoint_definition() -> Value {
    json!({"endpoint_id":ENDPOINT_ID,"path":ENDPOINT_PATH,"metadata":{},"critical":true,"ttl_seconds":60,"enabled":true})
}

pub fn router() -> Router<State> {
    Router::new()
        .route(ENDPOINT_PATH, post(delegated_list))
        .route("/api/v1/organizations/{org}/apps", get(member_list))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pagination {
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_limit")]
    limit: u32,
}
fn default_limit() -> u32 {
    100
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListRequest {
    org_id: String,
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_limit")]
    limit: u32,
}

async fn delegated_list(S(s): S<State>, h: HeaderMap, body: Bytes) -> Result<Json<Value>> {
    // A bearer token cannot substitute for an OBO proof on this endpoint. Select
    // the data plane first, rejecting unknown or unavailable test environments.
    let mut context_headers = h.clone();
    context_headers.remove("authorization");
    let c = context(&s, &context_headers).await?;
    let proof = header_value(&h, "x-iam-obo-access-proof")?
        .filter(|p| !p.is_empty() && p.len() <= 4096)
        .ok_or_else(Error::unauthorized)?;
    let input: ListRequest = serde_json::from_slice(&body)
        .map_err(|_| Error::bad("Expected org_id, optional after, and limit (1-100)"))?;
    validate(&input.org_id, input.after.as_deref(), input.limit)?;
    let identity = s
        .identity
        .verify_obo(proof, &body, c.environment.as_deref())
        .await?;
    list(
        &s,
        &c,
        &identity,
        &input.org_id,
        input.after.as_deref(),
        input.limit,
    )
    .await
}

async fn member_list(
    S(s): S<State>,
    Path(org): Path<String>,
    h: HeaderMap,
    Query(page): Query<Pagination>,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    list(
        &s,
        &c,
        c.identity()?,
        &org,
        page.after.as_deref(),
        page.limit,
    )
    .await
}

fn validate(org: &str, after: Option<&str>, limit: u32) -> Result<()> {
    if org.is_empty()
        || org.len() > 255
        || !(1..=100).contains(&limit)
        || after.is_some_and(|a| a.is_empty() || a.len() > 512)
    {
        return Err(Error::bad(
            "org_id is required; limit must be 1-100 and after a returned app_id",
        ));
    }
    Ok(())
}

async fn list(
    s: &State,
    c: &Context,
    identity: &Identity,
    org: &str,
    after: Option<&str>,
    limit: u32,
) -> Result<Json<Value>> {
    validate(org, after, limit)?;
    if !identity.member(org) {
        return Err(Error::forbidden());
    }
    if identity.testing_environment_id.as_deref()
        != (c.plane != "production").then_some(c.plane.as_str())
    {
        return Err(Error::unauthorized());
    }
    // Recheck after network verification, inside the read transaction: a key
    // rotation, cleanup, or generation change cannot switch the request's plane.
    let mut tx = s.db.begin().await?;
    if let Some(key) = c.environment.as_deref() {
        use sha2::{Digest, Sha256};
        let live: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM environments WHERE id=? AND key_hash=? AND state='ready' AND generation=?)")
            .bind(&c.plane).bind(hex::encode(Sha256::digest(key))).bind(c.generation)
            .fetch_one(&mut *tx).await?;
        if !live {
            return Err(Error::unauthorized());
        }
    }
    let rows = sqlx::query("SELECT a.app_id,a.org_id,a.name,a.description,a.visibility,a.state,a.effective_config FROM applications a WHERE a.plane=? AND a.org_id=? AND a.app_id>? AND NOT EXISTS(SELECT 1 FROM retirement_targets t JOIN operations o ON o.id=t.operation_id WHERE t.environment_id=a.plane AND t.app_id=a.app_id AND o.state='pending') ORDER BY a.app_id LIMIT ?")
        .bind(&c.plane).bind(org).bind(after.unwrap_or("")).bind(i64::from(limit) + 1)
        .fetch_all(&mut *tx).await?;
    let has_more = rows.len() > limit as usize;
    let items: Vec<Value> = rows.iter().take(limit as usize).map(|r| {
        let accepted: Value = r.get::<Option<String>, _>("effective_config")
            .and_then(|value| serde_json::from_str(&value).ok()).unwrap_or_default();
        // Explicit projection: never return webhook configuration, app secrets,
        // scope requests, testing credentials, drafts, or the full config blob.
        json!({
            "app_id":r.get::<String,_>("app_id"), "org_id":r.get::<String,_>("org_id"),
            "name":accepted["name"].as_str().unwrap_or(&r.get::<String,_>("name")),
            "description":accepted["description"].as_str().unwrap_or(&r.get::<String,_>("description")),
            "visibility":r.get::<String,_>("visibility"), "state":r.get::<String,_>("state")
        })
    }).collect();
    let next_cursor = has_more.then(|| items.last().unwrap()["app_id"].clone());
    tx.commit().await?;
    Ok(Json(
        json!({"org_id":org,"items":items,"next_cursor":next_cursor}),
    ))
}
