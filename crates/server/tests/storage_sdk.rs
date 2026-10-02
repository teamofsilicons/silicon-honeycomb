//! Durable storage consent, isolated credentials, and the real Briefcase transfer adapter.
use async_trait::async_trait;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use silicon_honeycomb_server::{
    auth::{Iam, Identity, IdentityProvider},
    error::{Error, Result},
    integration::ArchiveStorage,
    storage::Briefcase,
    storage_authorizations::{Broker, ENDPOINTS},
};
use silicon_iam_client::{Client, Credential};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};
const ENV: &str = "12345678-1234-1234-1234-123456789abc";
const KEY: &str = "12345678901234567890123456789012";
const REQUEST: &str = "11111111-1111-1111-1111-111111111111";
#[derive(Default)]
struct IdentityMock {
    calls: Mutex<Vec<(String, Value, String)>>,
    fail_refresh: Mutex<bool>,
}
fn pair(endpoint: &str, testing: bool) -> Value {
    let mut value = json!({"grant_id":format!("22222222-2222-2222-2222-{:012}",ENDPOINTS.iter().position(|e|*e==endpoint).unwrap()+1),"token_id":"33333333-3333-3333-3333-333333333333","access_token":format!("oba_{}","a".repeat(43)),"refresh_token":format!("obr_{}","r".repeat(43)),"token_type":"Bearer","expires_at":"2099-01-01T00:00:00Z","expires_in":60,"audience":"briefcase","endpoint_id":endpoint,"org_id":"selected","scope":format!("obo:briefcase:{endpoint}")});
    if testing {
        value["testing_context"] =
            json!({"app_id":"briefcase","app_secret":"ask_testonly","iam_test_key":KEY});
    }
    value
}
#[async_trait]
impl IdentityProvider for IdentityMock {
    async fn login(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn refresh(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn revoke(&self, _: &str, _: &str, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    async fn authenticate(&self, token: &str, env: Option<&str>) -> Result<Identity> {
        Ok(Identity {
            principal_id: token.into(),
            actor_type: Some("carbon".into()),
            organizations: BTreeMap::from([("acme".into(), Some("org_owner".into()))]),
            testing_environment_id: env.map(|_| ENV.into()),
            validator: false,
        })
    }
    async fn obo_operation(
        &self,
        op: &str,
        body: Value,
        key: &str,
        env: Option<&str>,
    ) -> Result<Value> {
        self.calls
            .lock()
            .unwrap()
            .push((op.into(), body.clone(), key.into()));
        match op {
            "authorize" => Ok(
                json!({"id":REQUEST,"authorization_url":format!("https://iam.example/obo/consent?request={REQUEST}")}),
            ),
            "exchange" => Ok(
                json!({"items":ENDPOINTS.iter().map(|e|pair(e,env.is_some())).collect::<Vec<_>>()}),
            ),
            "refresh" => {
                if *self.fail_refresh.lock().unwrap() {
                    return Err(Error::unavailable("Temporary network failure"));
                }
                Ok(json!({"items":[pair(ENDPOINTS[0],env.is_some())]}))
            }
            _ => unreachable!(),
        }
    }
}
async fn broker(testing: bool) -> (Arc<Broker>, Arc<IdentityMock>) {
    let db = silicon_honeycomb_server::database("sqlite::memory:")
        .await
        .unwrap();
    if testing {
        sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,state,created_at,last_activity) VALUES(?,'acme','c:owner','Test','','encrypted',?,'ready',1,1)").bind(ENV).bind(hex::encode(Sha256::digest(KEY))).execute(&db).await.unwrap();
    }
    let identity = Arc::new(IdentityMock::default());
    (
        Arc::new(Broker {
            db,
            identity: identity.clone(),
            encryption_key: [3; 32],
            audience: "briefcase".into(),
            allowed_origins: vec!["https://console.example".into()],
            iam_origin: "https://iam.example".into(),
        }),
        identity,
    )
}
async fn approve(broker: &Broker, testing: bool) -> Value {
    let environment = testing.then_some(KEY);
    let started = broker
        .start(
            "acme",
            Some("https://console.example/storage-authorization"),
            "c:owner",
            environment,
            "same-storage-request-key",
        )
        .await
        .unwrap();
    broker
        .complete(
            REQUEST.parse().unwrap(),
            "obc_authorization_code",
            started["state"].as_str().unwrap(),
            "c:owner",
            environment,
        )
        .await
        .unwrap();
    started
}
#[tokio::test]
async fn consent_is_bound_to_account_state_origin_and_keeps_credentials_encrypted() {
    let (broker, identity) = broker(false).await;
    for uri in [
        "https://evil.example/storage-authorization",
        "https://console.example/wrong",
        "https://console.example/storage-authorization#fragment",
        "http://console.example/storage-authorization",
    ] {
        assert!(
            broker
                .start("acme", Some(uri), "c:owner", None, "storage-request-key-2")
                .await
                .is_err()
        );
    }
    assert!(identity.calls.lock().unwrap().is_empty());
    let started = broker
        .start("acme", None, "c:owner", None, "manual-storage-request")
        .await
        .unwrap();
    assert!(
        identity.calls.lock().unwrap()[0]
            .1
            .get("redirect_uri")
            .is_none()
    );
    assert!(
        broker
            .complete(
                REQUEST.parse().unwrap(),
                "code",
                "wrong-state",
                "c:owner",
                None
            )
            .await
            .is_err()
    );
    assert!(
        broker
            .complete(
                REQUEST.parse().unwrap(),
                "code",
                started["state"].as_str().unwrap(),
                "c:other",
                None
            )
            .await
            .is_err()
    );
    assert_eq!(identity.calls.lock().unwrap().len(), 1);
    broker
        .complete(
            REQUEST.parse().unwrap(),
            "code",
            started["state"].as_str().unwrap(),
            "c:owner",
            None,
        )
        .await
        .unwrap();
    broker
        .complete(
            REQUEST.parse().unwrap(),
            "code",
            started["state"].as_str().unwrap(),
            "c:owner",
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        identity
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(op, _, _)| op == "exchange")
            .count(),
        1
    );
    let rows: Vec<String> = sqlx::query_scalar("SELECT encrypted_pair FROM storage_grants")
        .fetch_all(&broker.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    assert!(
        rows.iter()
            .all(|s| !s.contains("obr_") && !s.contains("oba_"))
    );
    assert_eq!(
        broker
            .token(ENDPOINTS[0], "c:other", None, "acme")
            .await
            .unwrap_err()
            .1
            .code,
        "storage_authorization_required"
    );
}
#[tokio::test]
async fn refresh_uncertainty_reuses_key_and_testing_generation_invalidates_cached_grant() {
    let (broker, identity) = broker(true).await;
    approve(&broker, true).await;
    sqlx::query("UPDATE storage_grants SET expires_at=0")
        .execute(&broker.db)
        .await
        .unwrap();
    *identity.fail_refresh.lock().unwrap() = true;
    assert!(
        broker
            .token(ENDPOINTS[0], "c:owner", Some(KEY), "acme")
            .await
            .is_err()
    );
    *identity.fail_refresh.lock().unwrap() = false;
    broker
        .token(ENDPOINTS[0], "c:owner", Some(KEY), "acme")
        .await
        .unwrap();
    let calls = identity.calls.lock().unwrap().clone();
    let keys: Vec<_> = calls
        .iter()
        .filter(|(op, _, _)| op == "refresh")
        .map(|(_, _, k)| k)
        .collect();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], keys[1]);
    sqlx::query("UPDATE environments SET generation=generation+1 WHERE id=?")
        .bind(ENV)
        .execute(&broker.db)
        .await
        .unwrap();
    assert_eq!(
        broker
            .token(ENDPOINTS[0], "c:owner", Some(KEY), "acme")
            .await
            .unwrap_err()
            .1
            .code,
        "storage_authorization_required"
    );
}
async fn logo_roundtrip(testing: bool) {
    let (broker, _) = broker(testing).await;
    approve(&broker, testing).await;
    let storage = MockServer::start().await;
    let access = format!("oba_{}", "a".repeat(43));
    for (endpoint, result) in [
        (
            "reserve",
            json!({"state":"reserved","upload_id":"upload","capability":"byte-capability"}),
        ),
        (
            "commit",
            json!({"state":"committed","published_entry_id":"entry"}),
        ),
    ] {
        Mock::given(method("POST"))
            .and(path(format!("/api/v1/obo/uploads/{endpoint}")))
            .and(header("x-iam-obo-access-token", &access))
            .and(header("x-org-id", "selected"))
            .respond_with(ResponseTemplate::new(200).set_body_json(result))
            .expect(1)
            .mount(&storage)
            .await;
    }
    Mock::given(method("PUT"))
        .and(path("/api/v1/obo/uploads/upload/content"))
        .and(header("x-org-id", "selected"))
        .and(header("x-briefcase-upload-capability", "byte-capability"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&storage)
        .await;
    let mut url = "https://briefcase.example/org/selected/logo.png".to_owned();
    if testing {
        url.push_str(&format!("?test_environment={ENV}"));
    }
    Mock::given(method("POST"))
        .and(path("/api/v1/obo/link-access"))
        .and(header("x-iam-obo-access-token", &access))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"effective":true,"url":url})))
        .expect(1)
        .mount(&storage)
        .await;
    let adapter = Briefcase {
        grants: Some(broker),
        iam: Client::builder("https://iam.example")
            .unwrap()
            .credential(Credential::application("honeycomb", "unused"))
            .build()
            .unwrap(),
        http: reqwest::Client::new(),
        base_url: storage.uri(),
        app_id: "honeycomb".into(),
        audience: "briefcase".into(),
    };
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("logo.png");
    std::fs::write(&file, b"normalized-pixels").unwrap();
    let result = adapter
        .upload_logo(
            "acme",
            &file,
            "c:owner",
            testing.then_some(KEY),
            "44444444-4444-4444-4444-444444444444",
        )
        .await
        .unwrap();
    assert!(result.contains("/api/v1/public/selected/logo.png"));
    for req in storage.received_requests().await.unwrap() {
        assert_eq!(req.headers.contains_key("x-briefcase-app-secret"), testing);
        assert!(!req.headers.contains_key("x-iam-obo-access-proof"));
        if req.method == "PUT" {
            assert!(!req.headers.contains_key("x-iam-obo-access-token"));
        }
    }
}
#[tokio::test]
async fn reusable_storage_token_keeps_selected_org_for_upload_and_publish() {
    logo_roundtrip(false).await;
}
#[tokio::test]
async fn reusable_storage_token_keeps_test_credentials_and_public_link_plane() {
    logo_roundtrip(true).await;
}
async fn exchange_failure(status: u16, code: &str, request_id: &str) -> Error {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/api/v1/obo-access/tokens")).respond_with(ResponseTemplate::new(status).set_body_json(json!({"error":{"code":code,"message":"SECRET_UPSTREAM_MESSAGE","details":{"subject_token":"SECRET_UPSTREAM_DETAILS"},"request_id":request_id}}))).mount(&server).await;
    let iam = Iam::new(&server.uri(), "honeycomb", "SECRET_APP_CREDENTIAL").unwrap();
    let error = iam
        .obo_operation(
            "refresh",
            json!({"refresh_token":"SECRET_REFRESH"}),
            "test-refresh-key-123",
            None,
        )
        .await
        .unwrap_err();
    assert!(!serde_json::to_string(&error.1).unwrap().contains("SECRET_"));
    error
}
#[tokio::test]
async fn iam_server_failure_keeps_safe_diagnostics_without_consent_advice() {
    let id = "01a0a904-aee3-75c3-b60c-ad97b466c388";
    let error = exchange_failure(500, "internal_error", id).await;
    assert_eq!(error.0, axum::http::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error.1.code, "integration_unavailable");
    assert!(!error.1.message.contains("consent"));
    assert!(!error.1.message.contains("scopes"));
    assert!(
        error
            .1
            .details
            .contains(&"upstream_code=internal_error".to_owned())
    );
    assert!(
        error
            .1
            .details
            .contains(&format!("upstream_request_id={id}"))
    );
}

#[tokio::test]
async fn iam_denial_retains_authorization_recovery_guidance() {
    let error = exchange_failure(
        403,
        "obo_authority_revoked",
        "01a0a904-aee3-75c3-b60c-ad97b466c388",
    )
    .await;
    assert_eq!(error.0, axum::http::StatusCode::FORBIDDEN);
    assert_eq!(error.1.code, "storage_authorization_required");
    assert!(error.1.message.contains("consent"));
    assert!(error.1.message.contains("same operation"));
}

#[tokio::test]
async fn iam_unknown_codes_and_malformed_correlation_ids_are_not_exposed() {
    let error = exchange_failure(500, "SECRET_UNKNOWN_CODE", "SECRET_REQUEST_ID").await;
    assert_eq!(error.1.details, vec!["upstream_status=500"]);
}
