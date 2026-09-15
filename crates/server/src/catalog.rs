//! Organization-scoped permission discovery; private providers remain private.
use crate::{
    State,
    api::{context, find_app},
    error::{Error, Result},
};
use axum::{
    Json,
    extract::{Path, Query, State as S},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
#[derive(Deserialize)]
pub struct ScopeQuery {
    provider: Option<String>,
}
pub async fn scopes(
    S(s): S<State>,
    h: HeaderMap,
    Path(org): Path<String>,
    Query(q): Query<ScopeQuery>,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    c.admin(&org)?;
    if c.plane != "production" {
        return Err(Error::unavailable(
            "IAM's scoped catalog for test applications is not available yet",
        ));
    }
    if let Some(provider) = &q.provider {
        find_app(&s, &c, provider, false).await?;
    }
    let catalog = s
        .management
        .scope_catalog(&org, q.provider.as_deref())
        .await?;
    let items = catalog["items"]
        .as_array()
        .ok_or_else(|| Error::unavailable("IAM omitted its scope catalog"))?;
    let items:Vec<Value>=items.iter().filter(|v|v["scope"].as_str().is_some_and(|scope| match &q.provider {
        Some(provider)=>scope.starts_with(&format!("obo:{provider}:")),None=>!scope.starts_with("obo:")
    })).map(|v|json!({"scope":v["scope"],"description":v["description"],"critical":v["critical"],"eligible":v["eligible"],"reviewer":v["reviewer"]})).collect();
    let rows=sqlx::query("SELECT app_id,org_id,name,effective_config,visibility FROM applications WHERE plane='production' AND state='active' AND effective_revision>0 ORDER BY name,app_id").fetch_all(&s.db).await?;
    let providers: Vec<Value> = rows
        .iter()
        .filter(|r| {
            r.get::<String, _>("visibility") == "public" || c.member(&r.get::<String, _>("org_id"))
        })
        .filter_map(|r| {
            let config: Value =
                serde_json::from_str(&r.get::<String, _>("effective_config")).ok()?;
            if !config["obo_endpoints"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
            {
                return None;
            }
            Some(json!({"app_id":r.get::<String,_>("app_id"),"name":config["name"]}))
        })
        .collect();
    Ok(Json(json!({"items":items,"providers":providers})))
}
