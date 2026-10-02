//! Application-to-application verification management through IAM's authority.
use crate::{
    State,
    api::{context, find_app, key},
    error::{Error, Result},
};
use axum::{
    Json, Router,
    extract::{Path, State as S},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use sqlx::Row;

pub fn router() -> Router<State> {
    Router::new()
        .route("/api/v1/ata-verifications", get(list_mine))
        .route(
            "/api/v1/apps/{app}/ata-verifications",
            get(list).post(create),
        )
        .route(
            "/api/v1/apps/{app}/ata-verifications/preview",
            post(preview),
        )
        .route(
            "/api/v1/apps/{app}/ata-verifications/{id}/revoke",
            post(revoke),
        )
}

/// The personal workspace does not change ATA authority: every upstream read
/// still carries the actor token and is authorized for its originating app.
async fn list_mine(S(s): S<State>, h: HeaderMap) -> Result<Response> {
    let c = context(&s, &h).await?;
    let identity = c.identity()?;
    if c.environment.is_some() {
        return Err(Error::bad(
            "Manage ATA verifications from the production console",
        ));
    }
    let actor = c.token.as_deref().ok_or_else(Error::unauthorized)?;
    let rows = sqlx::query("SELECT app_id,org_id,name,iam_revision,state FROM applications WHERE plane='production' ORDER BY name,app_id")
        .fetch_all(&s.db).await?;
    let mut applications = Vec::new();
    let mut pending = std::collections::VecDeque::new();
    for row in rows {
        let org: String = row.get("org_id");
        if !identity.admin(&org) {
            continue;
        }
        let app: String = row.get("app_id");
        let ready =
            row.get::<i64, _>("iam_revision") > 0 && row.get::<String, _>("state") != "disabled";
        applications.push(json!({"app_id":app,"org_id":org,"name":row.get::<String,_>("name"),"can_create":ready}));
        // A locally pending revision is not proof that IAM has no records.
        // Consult IAM and report unavailable apps rather than silently omitting them.
        pending.push_back(app);
    }
    let mut tasks = tokio::task::JoinSet::new();
    let mut items = Vec::new();
    let mut failures = Vec::new();
    loop {
        while tasks.len() < 4 {
            let Some(app) = pending.pop_front() else {
                break;
            };
            let state = s.clone();
            let actor = actor.to_owned();
            let principal = identity.principal_id.clone();
            tasks.spawn(async move {
                let result = async {
                    crate::retention_worker::ensure_app_available(&state, "production", &app)
                        .await?;
                    let response = state
                        .management
                        .ata_verification(&app, "list", None, &json!({}), &actor, None)
                        .await?;
                    personal_records(response, &app, &principal)
                }
                .await;
                (app, result)
            });
        }
        let Some(result) = tasks.join_next().await else {
            break;
        };
        let (app, result) = result.map_err(|_| {
            Error::unavailable("Verifications could not be loaded. Retry the page.")
        })?;
        match result {
            Ok(records) => items.extend(records),
            Err(error) => failures.push(json!({"app_id":app,"message":error.1.message})),
        }
    }
    items.sort_by(|a, b| {
        b["created_at"]
            .as_str()
            .cmp(&a["created_at"].as_str())
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    failures.sort_by(|a, b| a["app_id"].as_str().cmp(&b["app_id"].as_str()));
    Ok(response(
        json!({"items":items,"applications":applications,"failures":failures,"signing_principal":{"id":identity.principal_id,"kind":identity.actor_type},"organizations":identity.organizations.keys().collect::<Vec<_>>()}),
    ))
}

fn personal_records(response: Value, app: &str, principal: &str) -> Result<Vec<Value>> {
    let records = response["items"].as_array().ok_or_else(|| {
        Error::unavailable("IAM returned an incomplete verification list. Retry this application.")
    })?;
    let mut result = Vec::new();
    for record in records {
        if record["app_id"].as_str() != Some(app)
            || record["signing_principal"]["id"].as_str().is_none()
            || record["id"].as_str().is_none()
        {
            return Err(Error::unavailable(
                "IAM returned an incomplete verification list. Retry this application.",
            ));
        }
        if record["signing_principal"]["id"].as_str() == Some(principal) {
            let mut metadata = record.clone();
            if let Some(object) = metadata.as_object_mut() {
                object.remove("refresh_token");
            }
            result.push(metadata);
        }
    }
    Ok(result)
}
fn response(body: Value) -> Response {
    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}
async fn call(
    s: &State,
    h: &HeaderMap,
    app: &str,
    action: &str,
    id: Option<uuid::Uuid>,
    mut body: Value,
) -> Result<Response> {
    let c = context(s, h).await?;
    find_app(s, &c, app, true).await?;
    if c.environment.is_some() {
        return Err(Error::bad(
            "Manage ATA verifications from the production application console",
        ));
    }
    let actor = c.token.as_deref().ok_or_else(Error::unauthorized)?;
    let operation = if matches!(action, "create" | "revoke") {
        let id = uuid::Uuid::parse_str(key(h)?)
            .map_err(|_| Error::bad("Use a UUID Idempotency-Key and retain it when retrying"))?;
        if !body.is_object() {
            return Err(Error::bad("Expected an object"));
        }
        if body.get("operation_id").is_some() {
            return Err(Error::bad(
                "Operation identity is supplied through Idempotency-Key",
            ));
        }
        body["operation_id"] = serde_json::json!(id);
        Some(id)
    } else {
        None
    };
    Ok(response(
        s.management
            .ata_verification(app, action, id, &body, actor, operation)
            .await?,
    ))
}
async fn list(S(s): S<State>, h: HeaderMap, Path(app): Path<String>) -> Result<Response> {
    call(&s, &h, &app, "list", None, serde_json::json!({})).await
}
async fn preview(
    S(s): S<State>,
    h: HeaderMap,
    Path(app): Path<String>,
    Json(body): Json<Value>,
) -> Result<Response> {
    call(&s, &h, &app, "preview", None, body).await
}
async fn create(
    S(s): S<State>,
    h: HeaderMap,
    Path(app): Path<String>,
    Json(body): Json<Value>,
) -> Result<Response> {
    call(&s, &h, &app, "create", None, body).await
}
async fn revoke(
    S(s): S<State>,
    h: HeaderMap,
    Path((app, id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<Value>,
) -> Result<Response> {
    call(&s, &h, &app, "revoke", Some(id), body).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_list_rejects_missing_or_cross_application_metadata() {
        assert!(personal_records(json!({}), "ting", "c:owner").is_err());
        assert!(personal_records(json!({"items":[{"id":"one","app_id":"another","signing_principal":{"id":"c:owner"}}]}), "ting", "c:owner").is_err());
        assert!(
            personal_records(
                json!({"items":[{"id":"one","app_id":"ting"}]}),
                "ting",
                "c:owner"
            )
            .is_err()
        );
    }
}
