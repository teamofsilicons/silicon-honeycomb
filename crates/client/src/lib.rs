//! The stateless Honeycomb Rust client. Session persistence belongs to callers.
//!
//! ```no_run
//! # async fn example() -> anyhow::Result<()> {
//! let client = silicon_honeycomb_client::Client::new("https://backend.honeycomb.teamofsilicons.com")?;
//! let page = client.search("briefcase", 1, false).await?;
//! println!("{} applications", page.total);
//! # Ok(()) }
//! ```
#![forbid(unsafe_code)]
pub use honeycomb_core::*;
pub mod installer;
pub mod maintenance;
/// Stable local context name without exposing tokens or environment root keys.
pub fn context_fingerprint(origin: &str, environment: Option<&str>) -> String {
    hex::encode(sha2::Sha256::digest(format!(
        "{origin}\0{}",
        environment.unwrap_or("production")
    )))[..24]
        .to_owned()
}
use anyhow::{Context, Result, bail};
use reqwest::{Method, Response};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use url::Url;

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
    token: Option<String>,
    environment: Option<String>,
    telemetry: bool,
}
#[derive(Clone, Debug)]
pub struct Mutation {
    pub idempotency_key: String,
    pub revision: Option<i64>,
}
impl Default for Mutation {
    fn default() -> Self {
        Self::new()
    }
}
impl Mutation {
    pub fn new() -> Self {
        Self {
            idempotency_key: uuid::Uuid::new_v4().to_string(),
            revision: None,
        }
    }
    pub fn at_revision(revision: i64) -> Self {
        Self {
            revision: Some(revision),
            ..Self::new()
        }
    }
}
impl Client {
    pub fn new(base: &str) -> Result<Self> {
        let base = Url::parse(base)?;
        if base.scheme() != "https"
            && !(base.scheme() == "http"
                && matches!(base.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
        {
            bail!("Honeycomb requires HTTPS, except for a loopback development server");
        }
        if !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            bail!("API origin must not contain credentials, query or fragment");
        }
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!(
                    "silicon-honeycomb-client/",
                    env!("CARGO_PKG_VERSION")
                ))
                .build()?,
            base,
            token: None,
            environment: None,
            telemetry: true,
        })
    }
    /// Disable server diagnostics for this client, including anonymous requests.
    pub fn with_telemetry(&self, enabled: bool) -> Self {
        Self {
            telemetry: enabled,
            ..self.clone()
        }
    }
    /// Best-effort diagnostics use the authenticated backend; no ingest key is held here.
    pub async fn diagnostic(
        &self,
        source: &str,
        event: &str,
        action: &str,
        duration_ms: u64,
        success: bool,
    ) {
        if !self.telemetry || self.token.is_none() {
            return;
        }
        let Ok(url) = self.url(&["telemetry"]) else {
            return;
        };
        let _=self.request(Method::POST,url,None)
            .timeout(Duration::from_millis(300))
            .json(&json!({"source":source,"event":event,"action":action,"duration_ms":duration_ms.min(86_400_000),"success":success}))
            .send().await;
    }
    pub fn with_token(&self, token: impl Into<String>) -> Self {
        Self {
            token: Some(token.into()),
            ..self.clone()
        }
    }
    pub fn with_environment(&self, key: impl Into<String>) -> Result<Self> {
        let key = key.into();
        if key.len() != 32 || !key.bytes().all(|b| b.is_ascii_alphanumeric()) {
            bail!(
                "Testing key must be exactly 32 alphanumeric characters; use environments key to retrieve it"
            );
        }
        Ok(Self {
            environment: Some(key),
            ..self.clone()
        })
    }
    fn url(&self, segments: &[&str]) -> Result<Url> {
        let mut url = self.base.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("Invalid API origin"))?;
            path.clear().extend(["api", "v1"]).extend(segments);
        }
        Ok(url)
    }
    fn request(
        &self,
        method: Method,
        url: Url,
        mutation: Option<&Mutation>,
    ) -> reqwest::RequestBuilder {
        let mut request = self.http.request(method, url).header(
            "x-honeycomb-telemetry",
            if self.telemetry { "true" } else { "false" },
        );
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(env) = &self.environment {
            request = request.header("x-testing-environment-key", env);
        }
        if let Some(m) = mutation {
            request = request.header("idempotency-key", &m.idempotency_key);
            if let Some(revision) = m.revision {
                request = request.header("if-match", revision.to_string());
            }
        }
        request
    }
    async fn check(response: Response) -> Result<Response> {
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let body = Self::bounded(response, 1024 * 1024).await?;
        let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        let error = &value["error"];
        let details = error["details"]
            .as_array()
            .map(|d| {
                d.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        bail!(
            "HTTP {} {}: {}{}",
            status.as_u16(),
            error["code"].as_str().unwrap_or("request_failed"),
            error["message"]
                .as_str()
                .unwrap_or("Honeycomb returned an unexpected error response"),
            if details.is_empty() {
                String::new()
            } else {
                format!("\n{details}")
            }
        )
    }
    async fn bounded(mut response: Response, limit: usize) -> Result<Vec<u8>> {
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > limit {
                bail!("Response exceeds the {limit}-byte safety limit");
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
    pub async fn get<T: DeserializeOwned>(&self, segments: &[&str]) -> Result<T> {
        let response = Self::check(
            self.request(Method::GET, self.url(segments)?, None)
                .send()
                .await?,
        )
        .await?;
        Ok(serde_json::from_slice(
            &Self::bounded(response, 4 * 1024 * 1024).await?,
        )?)
    }
    pub async fn mutate<T: DeserializeOwned>(
        &self,
        method: Method,
        segments: &[&str],
        body: &impl Serialize,
        mutation: &Mutation,
    ) -> Result<T> {
        let response = Self::check(
            self.request(method, self.url(segments)?, Some(mutation))
                .json(body)
                .send()
                .await?,
        )
        .await?;
        Ok(serde_json::from_slice(
            &Self::bounded(response, 4 * 1024 * 1024).await?,
        )?)
    }
    pub async fn iam(&self) -> Result<Value> {
        self.get(&["iam"]).await
    }
    pub async fn login(&self, slt: &str, m: &Mutation) -> Result<Value> {
        self.mutate(Method::POST, &["auth", "login"], &json!({"slt":slt}), m)
            .await
    }
    pub async fn refresh(&self, token: &str, m: &Mutation) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["auth", "refresh"],
            &json!({"refresh_token":token}),
            m,
        )
        .await
    }
    pub async fn login_status(&self) -> Result<Value> {
        self.get(&["auth", "status"]).await
    }
    /// Revoke an access token. Prefer logout_session when holding a refresh token.
    pub async fn logout(&self, m: &Mutation) -> Result<()> {
        self.logout_session(None, m).await
    }
    /// Revoke the refresh family and access token, including expired sessions.
    pub async fn logout_session(&self, refresh_token: Option<&str>, m: &Mutation) -> Result<()> {
        Self::check(
            self.request(Method::POST, self.url(&["auth", "logout"])?, Some(m))
                .json(&json!({"refresh_token": refresh_token}))
                .send()
                .await?,
        )
        .await?;
        Ok(())
    }
    pub async fn report(
        &self,
        message: &str,
        pr: Option<&str>,
        mutation: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["reports"],
            &json!({"message":message,"pr":pr}),
            mutation,
        )
        .await
    }
    pub async fn search(&self, q: &str, page: u32, managed: bool) -> Result<Page<App>> {
        let mut url = self.url(&["apps"])?;
        url.query_pairs_mut()
            .append_pair("q", q)
            .append_pair("page", &page.to_string())
            .append_pair("managed", if managed { "true" } else { "false" });
        let response = Self::check(self.request(Method::GET, url, None).send().await?).await?;
        Ok(serde_json::from_slice(
            &Self::bounded(response, 4 * 1024 * 1024).await?,
        )?)
    }
    pub async fn app(&self, id: &str) -> Result<App> {
        self.get(&["apps", id]).await
    }
    pub async fn create_app(&self, input: &AppInput, m: &Mutation) -> Result<Value> {
        self.mutate(Method::POST, &["apps"], input, m).await
    }
    /// Upload a publicly viewable logo to Briefcase; save its URL in application configuration.
    pub async fn upload_logo(&self, org: &str, path: &Path, m: &Mutation) -> Result<Value> {
        use tokio::io::AsyncReadExt;
        let file = tokio::fs::File::open(path).await?;
        let mut bytes = Vec::new();
        file.take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.is_empty() || bytes.len() > 2 * 1024 * 1024 {
            bail!("Choose a PNG, JPEG or WebP logo no larger than 2 MiB");
        }
        let response = Self::check(
            self.request(
                Method::POST,
                self.url(&["organizations", org, "logos"])?,
                Some(m),
            )
            .header("content-type", "application/octet-stream")
            .body(bytes)
            .send()
            .await?,
        )
        .await?;
        Ok(serde_json::from_slice(
            &Self::bounded(response, 64 * 1024).await?,
        )?)
    }
    pub async fn update_app(&self, id: &str, input: &AppInput, m: &Mutation) -> Result<Value> {
        self.mutate(Method::PUT, &["apps", id], input, m).await
    }
    pub async fn releases(&self, id: &str) -> Result<Vec<Release>> {
        let value: Value = self.get(&["apps", id, "releases"]).await?;
        Ok(serde_json::from_value(value["items"].clone())?)
    }
    pub async fn upload_release(&self, id: &str, path: &Path, m: &Mutation) -> Result<Value> {
        let v = package::validate(path);
        if !v.valid {
            bail!("Archive validation failed:\n{}", v.errors.join("\n"));
        }
        if v.manifest.context("Missing manifest")?.app_id != id {
            bail!("Archive app_id does not match {id}");
        }
        let bytes = tokio::fs::read(path).await?;
        let response = Self::check(
            self.request(Method::POST, self.url(&["apps", id, "releases"])?, Some(m))
                .header("content-type", "application/gzip")
                .body(bytes)
                .send()
                .await?,
        )
        .await?;
        Ok(serde_json::from_slice(
            &Self::bounded(response, 4 * 1024 * 1024).await?,
        )?)
    }
    pub async fn download(&self, id: &str, release: &Release, destination: &Path) -> Result<()> {
        if release.app_id != id {
            bail!("Release belongs to a different application");
        }
        let mut url = self.url(&["apps", id, "download"])?;
        url.query_pairs_mut()
            .append_pair("version", &release.version);
        let response = Self::check(
            self.request(Method::GET, url, None)
                .header("x-download-receipt", uuid::Uuid::new_v4().to_string())
                .send()
                .await?,
        )
        .await?;
        let bytes = Self::bounded(response, package::MAX_ARCHIVE_BYTES as usize).await?;
        if hex::encode(Sha256::digest(&bytes)) != release.sha256
            || bytes.len() as i64 != release.size
        {
            bail!(
                "Download integrity verification failed; size or SHA-256 differs from the immutable release record"
            );
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(destination)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        let verified = package::validate(destination);
        if !verified.valid
            || verified
                .manifest
                .as_ref()
                .is_none_or(|m| m.app_id != id || m.version != release.version)
        {
            let _ = std::fs::remove_file(destination);
            bail!(
                "Downloaded archive does not match the requested application/version or failed package validation"
            );
        }
        Ok(())
    }
    pub async fn review(&self, id: &str, rating: f64, review: &str, m: &Mutation) -> Result<Value> {
        self.mutate(
            Method::PUT,
            &["apps", id, "reviews"],
            &json!({"rating":rating,"review":review}),
            m,
        )
        .await
    }
    pub async fn star(&self, id: &str, enabled: bool, m: &Mutation) -> Result<Value> {
        self.mutate(
            if enabled { Method::PUT } else { Method::DELETE },
            &["apps", id, "star"],
            &json!({}),
            m,
        )
        .await
    }
    pub async fn rotate_app_secret(
        &self,
        id: &str,
        step_up: Option<&str>,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["apps", id, "secret-rotations"],
            &json!({"step_up_assertion":step_up}),
            m,
        )
        .await
    }
    /// Read IAM's current pending webhook identity (application managers only).
    pub async fn webhook_state(&self, id: &str) -> Result<Value> {
        self.get(&["apps", id, "webhook"]).await
    }
    /// Activate the exact pending destination after IAM step-up verification.
    pub async fn approve_webhook(
        &self,
        id: &str,
        endpoint: &str,
        step_up: Option<&str>,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["apps", id, "webhook"],
            &json!({"action":"approve","pending_endpoint_id":endpoint,"step_up_assertion":step_up}),
            m,
        )
        .await
    }
    /// Replace webhook signing material. The new secret is never returned in a response.
    pub async fn rotate_webhook_secret(
        &self,
        id: &str,
        secret: &str,
        step_up: Option<&str>,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["apps", id, "webhook"],
            &json!({"action":"rotate","webhook_secret":secret,"step_up_assertion":step_up}),
            m,
        )
        .await
    }
    /// Retry the exact durable webhook change, optionally with fresh step-up evidence.
    pub async fn retry_webhook_operation(
        &self,
        id: &str,
        step_up: Option<&str>,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["operations", id, "webhook-retry"],
            &json!({"step_up_assertion":step_up}),
            m,
        )
        .await
    }
    pub async fn recover_operation_secret(&self, id: &str, m: &Mutation) -> Result<Value> {
        self.mutate(Method::POST, &["operations", id, "result"], &json!({}), m)
            .await
    }
    pub async fn request_publication(
        &self,
        id: &str,
        message: &str,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["apps", id, "publication"],
            &json!({"message":message}),
            m,
        )
        .await
    }
    pub async fn reconcile_app(&self, id: &str, m: &Mutation) -> Result<Value> {
        self.mutate(Method::POST, &["apps", id, "reconciliation"], &json!({}), m)
            .await
    }
    pub async fn activate_publication(&self, id: &str, m: &Mutation) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["review-requests", id, "activate"],
            &json!({}),
            m,
        )
        .await
    }
    pub async fn review_inbox(&self) -> Result<Value> {
        self.get(&["review-requests"]).await
    }
    pub async fn review_request(&self, id: &str, provider: &str) -> Result<Value> {
        self.get(&["review-requests", id, provider]).await
    }
    pub async fn retry_review_plan(&self, id: &str, m: &Mutation) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["review-requests", id, "plan"],
            &json!({}),
            m,
        )
        .await
    }
    pub async fn decide_review(
        &self,
        id: &str,
        provider: &str,
        decision: &str,
        reason: &str,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["review-requests", id, provider, "decisions"],
            &json!({"decision":decision,"reason":reason}),
            m,
        )
        .await
    }
    pub async fn reply_review(
        &self,
        id: &str,
        provider: &str,
        message: &str,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["review-requests", id, provider, "messages"],
            &json!({"message":message}),
            m,
        )
        .await
    }
    pub async fn publication(&self, id: &str) -> Result<Value> {
        self.get(&["apps", id, "publication"]).await
    }
    pub async fn publication_message(
        &self,
        id: &str,
        message: &str,
        provider: Option<&str>,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["apps", id, "publication", "messages"],
            &json!({"message":message,"provider":provider}),
            m,
        )
        .await
    }
    pub async fn operation(&self, id: &str) -> Result<Value> {
        self.get(&["operations", id]).await
    }
    pub async fn retry_operation(&self, id: &str, m: &Mutation) -> Result<Value> {
        self.mutate(Method::POST, &["operations", id, "retry"], &json!({}), m)
            .await
    }
    pub async fn drafts(&self, org: &str) -> Result<Value> {
        self.get(&["organizations", org, "drafts"]).await
    }
    pub async fn save_draft(
        &self,
        org: &str,
        id: &str,
        body: &Value,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(Method::PUT, &["organizations", org, "drafts", id], body, m)
            .await
    }
    pub async fn environments(&self) -> Result<Value> {
        self.get(&["environments"]).await
    }
    pub async fn create_environment(
        &self,
        org: &str,
        name: &str,
        description: &str,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["environments"],
            &json!({"org_id":org,"name":name,"description":description}),
            m,
        )
        .await
    }
    pub async fn environment(&self, id: &str) -> Result<Value> {
        self.get(&["environments", id]).await
    }
    /// Inspect per-app activity, protected dependencies and retention deadlines.
    pub async fn environment_retention(&self, id: &str) -> Result<Value> {
        self.get(&["environments", id, "retention"]).await
    }
    pub async fn set_environment_retention(
        &self,
        id: &str,
        days: i64,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::PUT,
            &["environments", id, "retention"],
            &json!({"idle_days":days}),
            m,
        )
        .await
    }
    /// Report actual test use. IAM environment generation/key version must be current.
    pub async fn report_environment_activity(
        &self,
        id: &str,
        app: &str,
        generation: i64,
        key_version: i64,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["environments", id, "apps", app, "activity"],
            &json!({"generation":generation,"key_version":key_version}),
            m,
        )
        .await
    }
    /// Import the complete external-scope dependency graph with explicit configuration pins.
    pub async fn import_application(
        &self,
        id: &str,
        app_id: &str,
        release: Option<&str>,
        refresh: bool,
        m: &Mutation,
    ) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["environments", id, "imports"],
            &json!({"app_id":app_id,"release":release,"refresh":refresh}),
            m,
        )
        .await
    }
    pub async fn environment_key(&self, id: &str, m: &Mutation) -> Result<Value> {
        self.mutate(Method::POST, &["environments", id, "key"], &json!({}), m)
            .await
    }
    pub async fn environment_action(&self, id: &str, action: &str, m: &Mutation) -> Result<Value> {
        self.mutate(
            Method::POST,
            &["environments", id, "actions", action],
            &json!({}),
            m,
        )
        .await
    }
}
