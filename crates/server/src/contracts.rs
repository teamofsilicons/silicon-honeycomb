//! API lifecycle accounting is operational state, independent of optional telemetry.
use crate::{
    State,
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    extract::{Request, State as S},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use sqlx::Row;

pub const VERSION: &str = "v1";
pub const QUIET_PERIOD_SECONDS: i64 = 7 * 24 * 60 * 60;

pub async fn retire_quiet(s: &State) -> Result<()> {
    let timestamp = now();
    sqlx::query("UPDATE api_contracts SET state='retired',retired_at=? WHERE state='deprecated' AND MAX(deprecated_at,COALESCE(last_request_at,deprecated_at))<=?")
        .bind(timestamp).bind(timestamp-QUIET_PERIOD_SECONDS).execute(&s.db).await?;
    Ok(())
}

pub async fn run(s: State) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        interval.tick().await;
        if let Err(error) = retire_quiet(&s).await {
            tracing::error!(error=%error.1.message,"API contract retirement check failed");
        }
    }
}

/// Retirement and request admission share one write transaction. A request admitted
/// before the quiet-period boundary resets it; a retired contract never reactivates.
async fn admit(s: &State, requested: Option<&str>, client: Option<&str>) -> Result<String> {
    if requested.is_some_and(|v| v != VERSION) {
        return Err(Error::new(
            StatusCode::NOT_ACCEPTABLE,
            "api_version_unsupported",
            "This endpoint supports API v1. Read /api/contracts before upgrading.",
        ));
    }
    let client = client
        .map(semver::Version::parse)
        .transpose()
        .map_err(|_| Error::bad("Honeycomb-Client-Version must be a semantic version"))?;
    let mut tx = s.db.begin_with("BEGIN IMMEDIATE").await?;
    let timestamp = now();
    sqlx::query("UPDATE api_contracts SET state='retired',retired_at=? WHERE version=? AND state='deprecated' AND MAX(deprecated_at,COALESCE(last_request_at,deprecated_at))<=?")
        .bind(timestamp).bind(VERSION).bind(timestamp-QUIET_PERIOD_SECONDS).execute(&mut *tx).await?;
    let row = sqlx::query("SELECT state,minimum_client FROM api_contracts WHERE version=?")
        .bind(VERSION)
        .fetch_one(&mut *tx)
        .await?;
    let phase: String = row.get("state");
    if phase == "retired" {
        tx.commit().await?;
        return Err(Error::new(
            StatusCode::GONE,
            "api_version_retired",
            "API v1 has retired after seven days without requests. Read /api/contracts for supported versions.",
        ));
    }
    let minimum = semver::Version::parse(&row.get::<String, _>("minimum_client"))
        .map_err(|_| Error::unavailable("API compatibility configuration is invalid"))?;
    let incompatible = client.is_some_and(|v| v < minimum);
    sqlx::query("UPDATE api_contracts SET last_request_at=? WHERE version=?")
        .bind(timestamp)
        .bind(VERSION)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    if incompatible {
        return Err(Error::new(
            StatusCode::BAD_REQUEST,
            "client_upgrade_required",
            format!("Upgrade the Honeycomb client to {minimum} or newer."),
        ));
    }
    Ok(phase)
}

pub async fn http(S(s): S<State>, request: Request, next: Next) -> Response {
    if !request.uri().path().starts_with("/api/v1/") {
        return next.run(request).await;
    }
    let headers = request.headers();
    let header = |name| crate::api::header_value(headers, name);
    let result = match (
        header("honeycomb-api-version"),
        header("honeycomb-client-version"),
    ) {
        (Ok(version), Ok(client)) => admit(&s, version, client).await,
        (Err(error), _) | (_, Err(error)) => Err(error),
    };
    let (mut response, phase) = match result {
        Ok(phase) => (next.run(request).await, phase),
        Err(error) => {
            let phase = if error.1.code == "api_version_retired" {
                "retired"
            } else {
                "unavailable"
            };
            (error.into_response(), phase.to_owned())
        }
    };
    response
        .headers_mut()
        .insert("honeycomb-api-version", HeaderValue::from_static(VERSION));
    response.headers_mut().insert(
        "honeycomb-contract-state",
        HeaderValue::from_str(&phase).unwrap(),
    );
    response
}

pub async fn discovery(S(s): S<State>) -> Result<Json<Value>> {
    retire_quiet(&s).await?;
    let rows = sqlx::query("SELECT version,state,minimum_client,deprecated_at,retired_at FROM api_contracts ORDER BY version").fetch_all(&s.db).await?;
    let versions: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "version":r.get::<String,_>("version"), "state":r.get::<String,_>("state"),
                "minimum_client":r.get::<String,_>("minimum_client"),
                "deprecated_at":r.get::<Option<i64>,_>("deprecated_at"),
                "retired_at":r.get::<Option<i64>,_>("retired_at")
            })
        })
        .collect();
    let supported: Vec<&str> = versions
        .iter()
        .filter(|v| v["version"] == VERSION && v["state"] != "retired")
        .map(|_| VERSION)
        .collect();
    let minimum = versions
        .iter()
        .find(|v| v["version"] == VERSION)
        .map(|v| v["minimum_client"].clone());
    Ok(Json(
        json!({"selected_version":VERSION,"supported_versions":supported,"minimum_client":minimum,"versions":versions,"retirement_quiet_seconds":QUIET_PERIOD_SECONDS}),
    ))
}
