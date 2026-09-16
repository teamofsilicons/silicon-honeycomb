//! Explicitly provisioned service authority for application lifecycle participants.
//! Catalog metadata never chooses where control credentials or environment keys are sent.
use crate::error::{Error, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, time::Duration};
use url::Url;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParticipantConfig {
    app_id: String,
    base_url: String,
    token_env: String,
}

#[derive(Default)]
pub struct ParticipantRegistry {
    participants: BTreeMap<String, ParticipantManagement>,
}

impl ParticipantRegistry {
    /// Parse HONEYCOMB_LIFECYCLE_PARTICIPANTS. Tokens are referenced by environment
    /// variable name so the destination inventory contains no control credentials.
    pub fn from_json(
        configuration: &str,
        resolve_secret: impl Fn(&str) -> Option<String>,
    ) -> anyhow::Result<Self> {
        Self::build(configuration, resolve_secret, true)
    }

    fn build(
        configuration: &str,
        resolve_secret: impl Fn(&str) -> Option<String>,
        require_https: bool,
    ) -> anyhow::Result<Self> {
        let entries: Vec<ParticipantConfig> = serde_json::from_str(configuration)
            .map_err(|_| anyhow::anyhow!("Invalid lifecycle participant configuration"))?;
        let mut participants = BTreeMap::new();
        for entry in entries {
            anyhow::ensure!(
                honeycomb_core::valid_app_id(&entry.app_id),
                "Invalid lifecycle participant application ID"
            );
            anyhow::ensure!(
                !participants.contains_key(&entry.app_id),
                "Duplicate lifecycle participant application ID"
            );
            anyhow::ensure!(
                !entry.token_env.is_empty()
                    && entry.token_env.len() <= 128
                    && entry
                        .token_env
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                    && !entry.token_env.as_bytes()[0].is_ascii_digit(),
                "Invalid lifecycle participant token environment variable"
            );
            let token = resolve_secret(&entry.token_env).ok_or_else(|| {
                anyhow::anyhow!("Lifecycle participant credential reference is unavailable")
            })?;
            let client = ParticipantManagement::build(
                entry.app_id.clone(),
                &entry.base_url,
                token,
                require_https,
            )?;
            participants.insert(entry.app_id, client);
        }
        Ok(Self { participants })
    }

    pub fn contains(&self, app_id: &str) -> bool {
        self.participants.contains_key(app_id)
    }

    /// User-driven and scheduled control actions use the same dedicated service
    /// authentication; neither a user's session nor a testing key is a service token.
    pub async fn apply(&self, app_id: &str, operation: &Value) -> Result<Value> {
        self.participants
            .get(app_id)
            .ok_or_else(|| {
                Error::unavailable(
                    "Protected lifecycle transport is not configured for this application",
                )
            })?
            .apply(operation)
            .await
    }
}

struct ParticipantManagement {
    app_id: String,
    base: Url,
    token: SecretString,
    http: reqwest::Client,
}

impl ParticipantManagement {
    fn build(
        app_id: String,
        base: &str,
        token: String,
        require_https: bool,
    ) -> anyhow::Result<Self> {
        let base = Url::parse(base)
            .map_err(|_| anyhow::anyhow!("Invalid lifecycle participant origin"))?;
        anyhow::ensure!(
            (!require_https || base.scheme() == "https")
                && matches!(base.scheme(), "http" | "https")
                && base.host_str().is_some()
                && base.username().is_empty()
                && base.password().is_none()
                && base.query().is_none()
                && base.fragment().is_none()
                && matches!(base.path(), "" | "/"),
            "Lifecycle participant URL must be an HTTPS origin without credentials, path, query or fragment"
        );
        anyhow::ensure!(
            token.len() >= 32 && token.bytes().all(|b| b.is_ascii_graphic()),
            "Lifecycle participant token must contain at least 32 visible ASCII characters"
        );
        Ok(Self {
            app_id,
            base,
            token: SecretString::from(token),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .build()?,
        })
    }

    async fn apply(&self, operation: &Value) -> Result<Value> {
        if operation["app_id"] != self.app_id {
            return Err(Error::bad(
                "Lifecycle operation targets another application",
            ));
        }
        let org = operation["org_id"]
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 128 && !matches!(*v, "." | ".."))
            .ok_or_else(|| Error::bad("Missing lifecycle organization"))?;
        let identity = |field: &str| -> Result<Uuid> {
            operation[field]
                .as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
                .filter(|id| !id.is_nil())
                .ok_or_else(|| Error::bad("Invalid lifecycle identity"))
        };
        let environment = identity("environment_id")?;
        let id = identity("operation_id")?;
        for field in ["environment_revision", "generation", "key_version"] {
            if operation[field].as_u64().is_none_or(|v| v == 0) {
                return Err(Error::bad("Invalid lifecycle version"));
            }
        }
        let mut endpoint = self.base.clone();
        endpoint
            .path_segments_mut()
            .map_err(|_| unavailable())?
            .extend([
                "internal",
                "honeycomb",
                "organizations",
                org,
                "testing-environments",
                &environment.to_string(),
                "operations",
                &id.to_string(),
            ]);
        // A retry keeps the same operation identity and payload. Participants retain
        // their receipts so a lost response cannot repeat a destructive operation.
        // IAM also receives environment metadata and, during rotation, the previous
        // root key as acting authority. Those fields are not part of a participant's
        // lifecycle contract and must not cross this service boundary.
        let payload: serde_json::Map<String, Value> = [
            "operation_id",
            "environment_id",
            "org_id",
            "app_id",
            "environment_revision",
            "generation",
            "key_version",
            "action",
            "testing_key",
            "snapshot",
            "reason",
            "retired_apps",
        ]
        .into_iter()
        .filter_map(|field| {
            operation
                .get(field)
                .map(|value| (field.to_owned(), value.clone()))
        })
        .collect();
        let mut response = self
            .http
            .put(endpoint)
            .bearer_auth(self.token.expose_secret())
            .json(&payload)
            .send()
            .await
            .map_err(|_| unavailable())?;
        let status = response.status();
        const MAX_RECEIPT: usize = 64 * 1024;
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RECEIPT as u64)
        {
            return Err(unavailable());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if bytes.len() + chunk.len() > MAX_RECEIPT {
                return Err(unavailable());
            }
            bytes.extend_from_slice(&chunk);
        }
        let receipt: Value = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
        if !status.is_success() {
            if status == reqwest::StatusCode::CONFLICT
                && receipt["error"]["code"] == "testing_environment_limit_reached"
            {
                return Err(Error::new(
                    axum::http::StatusCode::CONFLICT,
                    "testing_environment_limit_reached",
                    "This service has reached its testing-environment limit. Delete an unused environment through its normal recovery flow, then retry this operation.",
                ));
            }
            return Err(unavailable());
        }
        for field in [
            "operation_id",
            "environment_id",
            "app_id",
            "environment_revision",
            "generation",
            "key_version",
        ] {
            if receipt[field].is_null() || receipt[field] != operation[field] {
                return Err(unavailable());
            }
        }
        if !matches!(
            receipt["state"].as_str(),
            Some("pending" | "completed" | "failed")
        ) || (operation["action"] == "retire-applications"
            && receipt["retired_apps"] != operation["retired_apps"])
        {
            return Err(unavailable());
        }
        // Do not persist arbitrary participant payloads (which could echo a root key).
        let mut confirmed = serde_json::Map::new();
        for field in [
            "state",
            "operation_id",
            "environment_id",
            "app_id",
            "environment_revision",
            "generation",
            "key_version",
            "retired_apps",
        ] {
            if let Some(value) = receipt.get(field) {
                confirmed.insert(field.into(), value.clone());
            }
        }
        Ok(Value::Object(confirmed))
    }
}

fn unavailable() -> Error {
    Error::unavailable(
        "Lifecycle participant could not confirm this operation; retry its original operation.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path},
    };

    const TOKEN: &str = "dedicated-storage-lifecycle-service-token";
    fn configuration(app: &str, base: &str, token: &str) -> Value {
        json!({"app_id": app, "base_url": base, "token_env": token})
    }
    fn operation(app: &str) -> Value {
        json!({"app_id":app,"org_id":"org-1","operation_id":Uuid::new_v4(),
            "environment_id":Uuid::new_v4(),"environment_revision":3,"generation":2,
            "key_version":1,"action":"retire-applications","testing_key":"root-not-service-token",
            "retired_apps":[app],"snapshot":{},"reason":"inactivity"})
    }
    fn endpoint(op: &Value) -> String {
        format!(
            "/internal/honeycomb/organizations/org-1/testing-environments/{}/operations/{}",
            op["environment_id"].as_str().unwrap(),
            op["operation_id"].as_str().unwrap()
        )
    }
    fn receipt(op: &Value, state: &str) -> Value {
        let mut value = op.clone();
        let object = value.as_object_mut().unwrap();
        for key in ["testing_key", "snapshot", "reason", "org_id", "action"] {
            object.remove(key);
        }
        value["state"] = state.into();
        value
    }

    #[test]
    fn registry_requires_explicit_valid_destinations_and_secret_references() {
        let valid = configuration(
            "vendor>storage",
            "https://storage.example",
            "STORAGE_CONTROL_TOKEN",
        );
        let load = |values: Value| {
            ParticipantRegistry::from_json(&values.to_string(), |name| {
                (name == "STORAGE_CONTROL_TOKEN").then(|| TOKEN.into())
            })
        };
        assert!(load(json!([valid])).unwrap().contains("vendor>storage"));
        assert!(load(json!([])).unwrap().participants.is_empty());
        assert!(load(json!([valid, valid])).is_err());
        for base in [
            "http://storage.example",
            "https://user:pass@storage.example",
            "https://storage.example/path",
            "https://storage.example/?query",
            "https://storage.example/#fragment",
        ] {
            let mut entry = valid.clone();
            entry["base_url"] = base.into();
            assert!(load(json!([entry])).is_err());
        }
        for (field, value) in [
            ("app_id", "invalid"),
            ("token_env", "MISSING"),
            ("token_env", "bad-ref"),
            ("token_env", "0_INVALID"),
            ("token_env", ""),
            ("token", TOKEN),
        ] {
            let mut entry = valid.clone();
            entry[field] = value.into();
            assert!(load(json!([entry])).is_err(), "accepted invalid {field}");
        }
        for token in ["short", "a token containing whitespace that is long enough"] {
            assert!(
                ParticipantRegistry::from_json(&json!([valid]).to_string(), |_| Some(token.into()))
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn routes_only_registered_application_with_its_own_credential_and_replays_exactly() {
        let storage = MockServer::start().await;
        let worker = MockServer::start().await;
        let worker_token = "dedicated-worker-lifecycle-service-token";
        let registry = ParticipantRegistry::build(
            &json!([
                configuration("vendor>storage", &storage.uri(), "STORAGE_CONTROL_TOKEN"),
                configuration("another>worker", &worker.uri(), "WORKER_CONTROL_TOKEN"),
            ])
            .to_string(),
            |name| match name {
                "STORAGE_CONTROL_TOKEN" => Some(TOKEN.into()),
                "WORKER_CONTROL_TOKEN" => Some(worker_token.into()),
                _ => None,
            },
            false,
        )
        .unwrap();
        for (app, server, token) in [
            ("vendor>storage", &storage, TOKEN),
            ("another>worker", &worker, worker_token),
        ] {
            let op = operation(app);
            let completed = receipt(&op, "completed");
            Mock::given(method("PUT"))
                .and(path(endpoint(&op)))
                .and(header("authorization", format!("Bearer {token}")))
                .and(body_json(&op))
                .respond_with(ResponseTemplate::new(200).set_body_json(&completed))
                .expect(2)
                .mount(server)
                .await;
            assert_eq!(registry.apply(app, &op).await.unwrap(), completed);
            assert_eq!(registry.apply(app, &op).await.unwrap(), completed);
            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests[0].body, requests[1].body);
            assert!(!requests[0].headers.contains_key("x-honeycomb-actor-token"));
        }
        assert!(
            registry
                .apply("unknown>app", &operation("unknown>app"))
                .await
                .is_err()
        );
        assert!(
            registry
                .apply("vendor>storage", &operation("another>worker"))
                .await
                .is_err()
        );
        assert_eq!(storage.received_requests().await.unwrap().len(), 2);
        assert_eq!(worker.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn participant_payload_excludes_iam_metadata_and_previous_root_authority() {
        let server = MockServer::start().await;
        let registry = ParticipantRegistry::build(
            &json!([configuration(
                "vendor>storage",
                &server.uri(),
                "STORAGE_CONTROL_TOKEN"
            )])
            .to_string(),
            |_| Some(TOKEN.into()),
            false,
        )
        .unwrap();
        let expected = operation("vendor>storage");
        let mut coordinator = expected.clone();
        coordinator["name"] = json!("Shared environment");
        coordinator["description"] = json!("IAM-owned metadata");
        coordinator["authority_testing_key"] = json!("previous-root-must-not-be-forwarded");
        Mock::given(method("PUT"))
            .and(path(endpoint(&expected)))
            .and(body_json(&expected))
            .respond_with(ResponseTemplate::new(200).set_body_json(receipt(&expected, "completed")))
            .expect(2)
            .mount(&server)
            .await;
        registry
            .apply("vendor>storage", &coordinator)
            .await
            .unwrap();
        registry
            .apply("vendor>storage", &coordinator)
            .await
            .unwrap();
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests[0].body, requests[1].body);
        assert!(!String::from_utf8_lossy(&requests[0].body).contains("previous-root"));
    }

    #[tokio::test]
    async fn capacity_failure_has_actionable_guidance_without_echoing_participant_data() {
        let server = MockServer::start().await;
        let registry = ParticipantRegistry::build(
            &json!([configuration(
                "vendor>storage",
                &server.uri(),
                "STORAGE_CONTROL_TOKEN"
            )])
            .to_string(),
            |_| Some(TOKEN.into()),
            false,
        )
        .unwrap();
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(409).set_body_json(json!({"error":{"code":"testing_environment_limit_reached","message":"secret root token","details":{"testing_key":"secret-root"}}})))
            .mount(&server).await;
        let error = registry
            .apply("vendor>storage", &operation("vendor>storage"))
            .await
            .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::CONFLICT);
        assert_eq!(error.1.code, "testing_environment_limit_reached");
        assert!(error.1.message.contains("normal recovery flow"));
        assert!(!error.1.message.contains("secret"));
    }

    #[tokio::test]
    async fn rejects_unmatched_receipts_redirects_and_invalid_operations() {
        let server = MockServer::start().await;
        let redirect = MockServer::start().await;
        let registry = ParticipantRegistry::build(
            &json!([configuration(
                "vendor>storage",
                &server.uri(),
                "STORAGE_CONTROL_TOKEN"
            )])
            .to_string(),
            |_| Some(TOKEN.into()),
            false,
        )
        .unwrap();
        let op = operation("vendor>storage");
        for status in [401, 409, 503, 307] {
            server.reset().await;
            Mock::given(method("PUT"))
                .respond_with(
                    ResponseTemplate::new(status).insert_header("location", redirect.uri()),
                )
                .mount(&server)
                .await;
            let error = registry.apply("vendor>storage", &op).await.unwrap_err();
            assert_eq!(error.0, axum::http::StatusCode::SERVICE_UNAVAILABLE);
            assert!(!error.1.message.contains(TOKEN));
        }
        assert!(redirect.received_requests().await.unwrap().is_empty());
        for field in [
            "operation_id",
            "environment_id",
            "app_id",
            "environment_revision",
            "generation",
            "key_version",
            "retired_apps",
            "state",
        ] {
            server.reset().await;
            let mut wrong = receipt(&op, "completed");
            wrong[field] = Value::Null;
            Mock::given(method("PUT"))
                .respond_with(ResponseTemplate::new(200).set_body_json(wrong))
                .mount(&server)
                .await;
            assert!(
                registry.apply("vendor>storage", &op).await.is_err(),
                "invalid {field} accepted"
            );
        }
        server.reset().await;
        for field in [
            "operation_id",
            "environment_id",
            "environment_revision",
            "generation",
            "key_version",
        ] {
            let mut invalid = op.clone();
            invalid[field] = Value::Null;
            assert!(registry.apply("vendor>storage", &invalid).await.is_err());
        }
        assert!(server.received_requests().await.unwrap().is_empty());
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(65_537)))
            .mount(&server)
            .await;
        assert!(registry.apply("vendor>storage", &op).await.is_err());
    }

    #[tokio::test]
    async fn preserves_pending_and_failed_status_but_never_persists_echoed_secrets() {
        let server = MockServer::start().await;
        let registry = ParticipantRegistry::build(
            &json!([configuration(
                "vendor>storage",
                &server.uri(),
                "STORAGE_CONTROL_TOKEN"
            )])
            .to_string(),
            |_| Some(TOKEN.into()),
            false,
        )
        .unwrap();
        let op = operation("vendor>storage");
        for state in ["pending", "failed", "completed"] {
            server.reset().await;
            let expected = receipt(&op, state);
            let mut echo = expected.clone();
            echo["testing_key"] = op["testing_key"].clone();
            echo["debug"] = json!({"app_secret":"must-not-be-persisted"});
            Mock::given(method("PUT"))
                .respond_with(ResponseTemplate::new(200).set_body_json(echo))
                .mount(&server)
                .await;
            assert_eq!(
                registry.apply("vendor>storage", &op).await.unwrap(),
                expected
            );
        }
    }
}
