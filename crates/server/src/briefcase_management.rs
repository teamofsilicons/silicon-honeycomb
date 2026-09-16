//! Dedicated service authority for Briefcase's retry-safe lifecycle participant.
use crate::error::{Error, Result};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

pub struct BriefcaseManagement {
    base: Url,
    token: SecretString,
    http: reqwest::Client,
}

impl BriefcaseManagement {
    #[cfg(test)]
    pub(crate) fn test_client(base: &str, token: String) -> anyhow::Result<Self> {
        Self::build(base, token, false)
    }
    pub fn new(base: &str, token: String) -> anyhow::Result<Self> {
        Self::build(base, token, true)
    }

    fn build(base: &str, token: String, require_https: bool) -> anyhow::Result<Self> {
        let base = Url::parse(base)?;
        anyhow::ensure!(
            (!require_https || base.scheme() == "https")
                && matches!(base.scheme(), "http" | "https")
                && base.host_str().is_some()
                && base.username().is_empty()
                && base.password().is_none()
                && base.query().is_none()
                && base.fragment().is_none()
                && matches!(base.path(), "" | "/"),
            "BRIEFCASE_BASE_URL must be an HTTPS origin without credentials, path, query or fragment"
        );
        anyhow::ensure!(
            token.len() >= 32 && token.bytes().all(|b| b.is_ascii_graphic()),
            "BRIEFCASE_HONEYCOMB_SERVICE_TOKEN must contain at least 32 visible ASCII characters"
        );
        Ok(Self {
            base,
            token: SecretString::from(token),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .build()?,
        })
    }

    pub async fn apply(&self, operation: &Value) -> Result<Value> {
        if operation["app_id"] != "tos>briefcase" {
            return Err(Error::bad(
                "Briefcase lifecycle requires app_id tos>briefcase",
            ));
        }
        let org = operation["org_id"]
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 128)
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
        // PUT retries retain the exact coordinator operation and payload. The participant
        // persists replay receipts, including after a response is lost.
        let mut response = self
            .http
            .put(endpoint)
            .bearer_auth(self.token.expose_secret())
            .json(operation)
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
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
        Ok(receipt)
    }
}

fn unavailable() -> Error {
    Error::unavailable(
        "Briefcase lifecycle could not confirm this operation; retry its original operation.",
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

    const TOKEN: &str = "dedicated-briefcase-lifecycle-service-token";
    fn operation() -> Value {
        json!({"app_id":"tos>briefcase","org_id":"org-1","operation_id":Uuid::new_v4(),
            "environment_id":Uuid::new_v4(),"environment_revision":3,"generation":2,
            "key_version":1,"action":"retire-applications","testing_key":"root-not-service-token",
            "retired_apps":["tos>briefcase"],"snapshot":{},"reason":"inactivity"})
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
    fn protects_credential_destination() {
        for base in [
            "http://localhost:1234",
            "https://user:pass@example.com",
            "https://example.com/path",
            "https://example.com/?query",
            "https://example.com/#fragment",
        ] {
            assert!(BriefcaseManagement::new(base, TOKEN.into()).is_err());
        }
        assert!(BriefcaseManagement::new("https://example.com", "short".into()).is_err());
        assert!(BriefcaseManagement::new("https://example.com", TOKEN.into()).is_ok());
    }
    #[tokio::test]
    async fn exact_contract_pending_completion_and_replay() {
        let server = MockServer::start().await;
        let client = BriefcaseManagement::build(&server.uri(), TOKEN.into(), false).unwrap();
        let op = operation();
        let pending = receipt(&op, "pending");
        Mock::given(method("PUT"))
            .and(path(endpoint(&op)))
            .and(header("authorization", format!("Bearer {TOKEN}")))
            .and(body_json(&op))
            .respond_with(ResponseTemplate::new(200).set_body_json(&pending))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(client.apply(&op).await.unwrap(), pending);
        server.verify().await;
        server.reset().await;
        let completed = receipt(&op, "completed");
        Mock::given(method("PUT"))
            .and(path(endpoint(&op)))
            .and(body_json(&op))
            .respond_with(ResponseTemplate::new(200).set_body_json(&completed))
            .expect(2)
            .mount(&server)
            .await;
        assert_eq!(client.apply(&op).await.unwrap(), completed);
        assert_eq!(client.apply(&op).await.unwrap(), completed);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests[0].body, requests[1].body);
        assert!(!requests[0].headers.contains_key("x-honeycomb-actor-token"));
    }
    #[tokio::test]
    async fn failed_transport_and_wrong_receipts_never_confirm_success() {
        let server = MockServer::start().await;
        let redirect = MockServer::start().await;
        let client = BriefcaseManagement::build(&server.uri(), TOKEN.into(), false).unwrap();
        let op = operation();
        for status in [401, 409, 503, 307] {
            server.reset().await;
            Mock::given(method("PUT"))
                .respond_with(
                    ResponseTemplate::new(status).insert_header("location", redirect.uri()),
                )
                .mount(&server)
                .await;
            let error = client.apply(&op).await.unwrap_err();
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
            assert!(client.apply(&op).await.is_err(), "invalid {field} accepted");
        }
        server.reset().await;
        let failed = receipt(&op, "failed");
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&failed))
            .mount(&server)
            .await;
        assert_eq!(client.apply(&op).await.unwrap(), failed);
        server.reset().await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(65_537)))
            .mount(&server)
            .await;
        assert!(client.apply(&op).await.is_err());
    }
}
