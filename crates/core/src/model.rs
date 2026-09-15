use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppScope {
    #[serde(default)]
    pub iam: Vec<String>,
    #[serde(default)]
    pub external: Vec<ExternalScope>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalScope {
    pub app_id: String,
    pub endpoint_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OboEndpoint {
    pub endpoint_id: String,
    pub path: String,
    #[serde(default)]
    pub metadata: Value,
    pub critical: bool,
    #[serde(default = "default_ttl")]
    pub ttl_seconds: u64,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
fn default_ttl() -> u64 {
    300
}
fn enabled() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppInput {
    pub org_id: String,
    pub local_app_id: String,
    pub name: String,
    pub description: String,
    #[serde(default = "default_idle_days")]
    pub testing_idle_days: u32,
    #[serde(default)]
    pub logo_url: Option<String>,
    #[serde(default)]
    pub website_url: Option<String>,
    #[serde(default)]
    pub docs_url: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    pub webhook_url: String,
    /// Empty on updates retains the existing secret; creation requires a new secret.
    #[serde(default)]
    pub webhook_secret: String,
    #[serde(default)]
    pub webhook_scope: Vec<String>,
    #[serde(default)]
    pub app_scope: AppScope,
    #[serde(default)]
    pub obo_endpoints: Vec<OboEndpoint>,
    #[serde(default)]
    pub obo_review_message: Option<String>,
}
fn default_idle_days() -> u32 {
    30
}
impl AppInput {
    pub fn app_id(&self) -> String {
        format!("{}>{}", self.org_id, self.local_app_id)
    }
    pub fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if !(1..=36500).contains(&self.testing_idle_days) {
            errors.push("testing_idle_days: must be between 1 and 36500".into());
        }
        for (field, value) in [
            ("org_id", &self.org_id),
            ("local_app_id", &self.local_app_id),
        ] {
            if !valid_handle(value) {
                errors.push(format!("{field}: use 1–64 lowercase letters, numbers or hyphens, starting with a letter or number"));
            }
        }
        if self.name.trim().is_empty() || self.name.len() > 100 {
            errors.push("name: must contain 1–100 characters".into());
        }
        let words = self.description.split_whitespace().count();
        if !(50..=1000).contains(&words) {
            errors.push(format!(
                "description: expected 50–1000 words; received {words}"
            ));
        }
        for (field, value) in [
            ("webhook_url", Some(&self.webhook_url)),
            ("logo_url", self.logo_url.as_ref()),
            ("website_url", self.website_url.as_ref()),
            ("docs_url", self.docs_url.as_ref()),
            ("base_url", self.base_url.as_ref()),
        ] {
            if let Some(value) = value {
                match url::Url::parse(value) {
                    Ok(u)
                        if u.scheme() == "https"
                            && u.host_str().is_some()
                            && u.username().is_empty()
                            && u.password().is_none() => {}
                    _ => errors.push(format!("{field}: must be an HTTPS URL without credentials")),
                }
            }
        }
        if self.webhook_secret.len() < 32 {
            errors.push("webhook_secret: use at least 32 characters".into());
        }
        if self
            .webhook_scope
            .iter()
            .any(|s| !["membership", "updates", "trust", "full"].contains(&s.as_str()))
        {
            errors.push(
                "webhook_scope: allowed categories are membership, updates, trust and full".into(),
            );
        }
        if !self.obo_endpoints.is_empty() && self.base_url.is_none() {
            errors.push("base_url: required when exposing OBO endpoints".into());
        }
        if let Some(base) = &self.base_url
            && url::Url::parse(base).is_ok_and(|u| u.origin().ascii_serialization() != *base)
        {
            errors.push(
                "base_url: use a backend origin without a path, query or trailing slash".into(),
            );
        }
        let mut ids = std::collections::HashSet::new();
        for e in &self.obo_endpoints {
            if !ids.insert(&e.endpoint_id) {
                errors.push(format!(
                    "obo_endpoints: duplicate endpoint_id {}",
                    e.endpoint_id
                ));
            }
            if e.endpoint_id.is_empty()
                || e.endpoint_id.len() > 128
                || !e
                    .endpoint_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            {
                errors
                    .push("endpoint_id: use letters, numbers, dots, underscores or hyphens".into());
            }
            if !e.path.starts_with('/')
                || e.path.starts_with("//")
                || e.path.contains('?')
                || e.path.contains('#')
                || e.path.split('/').any(|p| p == ".." || p == ".")
            {
                errors.push(format!(
                    "{}: path must be an absolute endpoint path",
                    e.endpoint_id
                ));
            }
            if e.ttl_seconds == 0 {
                errors.push(format!("{}: ttl_seconds must be positive", e.endpoint_id));
            }
        }
        for e in &self.app_scope.external {
            if !valid_app_id(&e.app_id) || e.endpoint_id.is_empty() {
                errors.push(
                    "app_scope.external: each scope requires an org>app identifier and endpoint_id"
                        .into(),
                );
            }
        }
        errors
    }
    /// Only this projection may appear in ordinary records; webhook secrets are encrypted separately.
    pub fn public_config(&self) -> Value {
        let mut value = serde_json::to_value(self).expect("serializable AppInput");
        value.as_object_mut().unwrap().remove("webhook_secret");
        value
    }
}
pub fn valid_handle(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub fn valid_app_id(s: &str) -> bool {
    s.split_once('>')
        .is_some_and(|(a, b)| valid_handle(a) && valid_handle(b))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct App {
    pub app_id: String,
    pub org_id: String,
    pub name: String,
    pub description: String,
    pub visibility: String,
    pub state: String,
    pub revision: i64,
    pub iam_revision: i64,
    #[serde(default)]
    pub effective_revision: i64,
    pub config: Value,
    #[serde(default)]
    pub effective_config: Value,
    pub latest_version: Option<String>,
    pub rating: f64,
    pub reviews: i64,
    pub stars: i64,
    pub installs: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub app_id: String,
    pub version: String,
    pub sha256: String,
    pub size: i64,
    pub created_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub per_page: u32,
    pub total: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: Vec<String>,
}
