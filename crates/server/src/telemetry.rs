//! Best-effort operational diagnostics. Never serialize requests, identities or errors.
use crate::{
    State,
    api::context,
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    extract::{MatchedPath, Request, State as S},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub trait EventSink: Send + Sync {
    fn record(&self, event: Value);
}
#[cfg(unix)]
impl EventSink for space_station::SpaceClient {
    fn record(&self, event: Value) {
        self.record(event);
    }
}
#[derive(Clone, Default)]
pub struct Recorder(Option<Arc<dyn EventSink>>);
impl Recorder {
    pub fn with_sink(sink: Arc<dyn EventSink>) -> Self {
        Self(Some(sink))
    }
    /// No key or an explicit opt-out leaves the exporter inactive.
    pub fn from_env() -> Self {
        if std::env::var("HONEYCOMB_TELEMETRY").is_ok_and(|v| disabled(&v)) {
            return Self::default();
        }
        let Some(key) = std::env::var("HONEYCOMB_SPACE_STATION_TABLE_KEY")
            .ok()
            .filter(|v| !v.is_empty())
        else {
            return Self::default();
        };
        let url = std::env::var("HONEYCOMB_TELEMETRY_URL")
            .unwrap_or_else(|_| "https://backend.spacestation.teamofsilicons.com".into());
        let home = std::env::var_os("HONEYCOMB_TELEMETRY_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "data/telemetry".into());
        match Self::new(&key, &url, &home) {
            Ok(recorder) => recorder,
            Err(_) => {
                tracing::warn!("Honeycomb telemetry configuration is invalid; export is inactive");
                Self::default()
            }
        }
    }
    pub fn new(
        key: &str,
        origin: &str,
        home: &std::path::Path,
    ) -> std::result::Result<Self, &'static str> {
        if !key
            .strip_prefix("table-siliconhoneycomb-")
            .is_some_and(|suffix| {
                suffix.len() == 32 && suffix.bytes().all(|b| b.is_ascii_hexdigit())
            })
        {
            return Err("Invalid telemetry configuration");
        }
        let url = url::Url::parse(origin).map_err(|_| "Invalid telemetry configuration")?;
        if !(url.scheme() == "https"
            || url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err("Invalid telemetry configuration");
        }
        #[cfg(unix)]
        {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let client = space_station::SpaceClient::builder(key)
                .url(origin)
                .home(home)
                .flush_timeout(Duration::from_millis(100))
                .on_error(|_| {})
                .build()
                .map_err(|_| "Invalid telemetry configuration")?;
            Ok(Self::with_sink(Arc::new(client)))
        }
        #[cfg(not(unix))]
        {
            let _ = home;
            Err("Invalid telemetry configuration")
        }
    }
    pub fn active(&self) -> bool {
        self.0.is_some()
    }
}
fn disabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "false" | "0" | "off" | "no"
    )
}
fn permitted(headers: &HeaderMap) -> bool {
    !headers
        .get("x-honeycomb-telemetry")
        .is_some_and(|v| v.to_str().is_ok_and(disabled))
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Sdk,
    Cli,
    Daemon,
    Library,
    Console,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    CommandCompleted,
    HttpCompleted,
    PageView,
    UpdateCompleted,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    source: Source,
    event: Event,
    action: String,
    duration_ms: u64,
    success: bool,
}
const ACTIONS: &[&str] = &[
    "apps",
    "releases",
    "publication",
    "drafts",
    "environments",
    "operations",
    "search",
    "iam",
    "login",
    "logout",
    "validate",
    "pack",
    "install",
    "update",
    "uninstall",
    "installed",
    "review",
    "report",
    "star",
    "config",
    "self_update",
    "daemon",
    "service",
    "catalog",
    "applications",
    "application",
    "review_inbox",
    "sign_in",
    "unknown",
];

pub async fn record(
    s: &State,
    plane: &str,
    generation: Option<i64>,
    source: &str,
    event: &str,
    fields: Value,
) {
    if !s.telemetry.active() {
        return;
    }
    let value = json!({"schema_version":1,"service":"silicon-honeycomb","version":env!("CARGO_PKG_VERSION"),"source":source,"event":event,"step":event,"progress":1,"recorded_at":now(),"environment":if plane=="production"{"production"}else{"testing"},"context":fields});
    if plane == "production" {
        if let Some(sink) = &s.telemetry.0 {
            sink.record(value);
        }
        return;
    }
    // Capture only in the current ready generation, under the same transaction as trimming.
    // A concurrent clean makes the INSERT select zero rows; test events never use the production key.
    let _=tokio::time::timeout(Duration::from_millis(30),async {
        let mut tx=s.db.begin().await?;
        sqlx::query("INSERT INTO telemetry_events(plane,generation,event,created_at) SELECT id,generation,?,? FROM environments WHERE id=? AND state='ready' AND generation=?").bind(value.to_string()).bind(now()).bind(plane).bind(generation).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM telemetry_events WHERE plane=? AND id NOT IN (SELECT id FROM telemetry_events WHERE plane=? ORDER BY id DESC LIMIT 1000)").bind(plane).bind(plane).execute(&mut *tx).await?;
        tx.commit().await
    }).await;
}
pub async fn ingest(
    S(s): S<State>,
    headers: HeaderMap,
    Json(input): Json<Diagnostic>,
) -> Result<StatusCode> {
    if !permitted(&headers) {
        return Ok(StatusCode::NO_CONTENT);
    }
    let c = context(&s, &headers).await?;
    c.identity()?;
    if !ACTIONS.contains(&input.action.as_str()) || input.duration_ms > 86_400_000 {
        return Err(Error::bad("Unsupported diagnostic action or duration"));
    }
    let source = serde_json::to_value(&input.source).map_err(|e| anyhow::anyhow!(e))?;
    let event = serde_json::to_value(&input.event).map_err(|e| anyhow::anyhow!(e))?;
    record(
        &s,
        &c.plane,
        c.generation,
        source.as_str().unwrap(),
        event.as_str().unwrap(),
        json!({"action":input.action,"duration_ms":input.duration_ms,"success":input.success}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn http(S(s): S<State>, request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned());
    let enabled = permitted(request.headers())
        && !request.headers().contains_key("x-testing-environment-key")
        && route.as_deref() != Some("/api/v1/telemetry")
        && route.as_deref() != Some("/health");
    let method = match request.method().as_str() {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "DELETE" => "DELETE",
        "PATCH" => "PATCH",
        "HEAD" => "HEAD",
        "OPTIONS" => "OPTIONS",
        _ => "OTHER",
    };
    let started = Instant::now();
    let response = next.run(request).await;
    if enabled && let Some(route) = route {
        record(&s,"production",None,"backend","http_completed",json!({"route":route,"method":method,"status":response.status().as_u16(),"success":response.status().is_success(),"duration_ms":started.elapsed().as_millis() as u64,"request_id":uuid::Uuid::new_v4()})).await;
    }
    response
}
