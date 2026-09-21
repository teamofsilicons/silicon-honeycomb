//! Delegated catalog requests through the real IAM SDK and HTTP router.
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use silicon_honeycomb_server::{
    State,
    auth::Iam,
    error::{Error, Result},
    integration::{ArchiveStorage, Management},
    obo::{ENDPOINT_ID, ENDPOINT_PATH},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tower::ServiceExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const ENV: &str = "44444444-4444-4444-4444-444444444444";
const KEY: &str = "testenvironmentkey12345678901234";
const TEST_SECRET: &str = "ask_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
struct Manager;
#[async_trait]
impl Management for Manager {
    async fn configure(&self, _: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::forbidden())
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::forbidden())
    }
    async fn recover_testing_application_credential(&self, id: &str, _: &str) -> Result<Value> {
        Ok(json!({"environment_id":id,"app_id":"tos>honeycomb","app_secret":TEST_SECRET}))
    }
}
struct Store;
#[async_trait]
impl ArchiveStorage for Store {
    async fn put(
        &self,
        _: &str,
        _: &str,
        _: &std::path::Path,
        _: &str,
        _: Option<&str>,
        _: &str,
    ) -> Result<String> {
        Err(Error::forbidden())
    }
    async fn read(&self, _: &str, _: &str, _: Option<&str>, _: Option<&str>) -> Result<Vec<u8>> {
        Err(Error::forbidden())
    }
    async fn publish(&self, _: &str, _: &str, _: &str, _: Option<&str>, _: &str) -> Result<String> {
        Err(Error::forbidden())
    }
}
async fn setup(server: &MockServer) -> State {
    let db = silicon_honeycomb_server::database("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,state,created_at,last_activity) VALUES(?,'tos','carbon','Test','','unused',?,'ready',1,1)")
        .bind(ENV).bind(hex::encode(Sha256::digest(KEY))).execute(&db).await.unwrap();
    let management = Arc::new(Manager);
    State {
        telemetry: Default::default(),
        identity: Arc::new(
            Iam::new(&server.uri(), "tos>honeycomb", "production-secret")
                .unwrap()
                .with_testing_context(db.clone(), management.clone()),
        ),
        db,
        management,
        storage: Arc::new(Store),
        app_id: "tos>honeycomb".into(),
        iam_app_id: "tos>iam".into(),
        iam_login_url: server.uri(),
        encryption_key: [1; 32],
        webhook_secret: "unused".into(),
    }
}
async fn app(s: &State, plane: &str, id: &str, org: &str, visibility: &str) {
    let config = json!({"name":"Accepted name","description":"Accepted description","webhook_secret":"never-expose-webhook","app_secret":"never-expose-app-secret","testing_key":"never-expose-testing-key","app_scope":{"iam":["secret-scope"]}}).to_string();
    sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,visibility,state,config,effective_config,webhook_secret,created_at,updated_at) VALUES(?,?,?,'Draft name','Draft description',?,'active',?,?,'never-expose-webhook',1,1)")
        .bind(plane).bind(id).bind(org).bind(visibility).bind(&config).bind(&config).execute(&s.db).await.unwrap();
}
fn proof(env: Option<&str>) -> Value {
    json!({"valid":true,"proof_id":"11111111-1111-4111-8111-111111111111","issuer_app_id":"caller>app","audience":"tos>honeycomb","actor":{"type":"carbon","public_id":"carbon-user"},"authorization":{"actor_type":"carbon","public_id":"carbon-user","organization_id":"22222222-2222-4222-8222-222222222222","org_id":"tos","membership_id":"33333333-3333-4333-8333-333333333333","membership_version":1,"authorization_epoch":1,"audience":"tos>honeycomb","testing_environment_id":env,"scopes":["obo:tos>honeycomb:honeycomb.apps.list"],"org_role":null,"tags":null},"org_id":"tos","endpoint":{"endpoint_id":ENDPOINT_ID,"path":ENDPOINT_PATH},"metadata":{},"expires_at":"2099-01-01T00:00:00Z","consumed_at":"2026-09-22T00:00:00Z"})
}
async fn wire(server: &MockServer, response: Value) {
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/obo-access/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(server)
        .await;
}
async fn send(
    s: &State,
    body: &str,
    proof: Option<&str>,
    env: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::post(ENDPOINT_PATH).header("content-type", "application/json");
    if let Some(proof) = proof {
        request = request.header("x-iam-obo-access-proof", proof);
    }
    if let Some(env) = env {
        request = request.header("x-testing-environment-key", env);
    }
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn delegates_current_org_private_catalog_with_exact_request_binding_and_safe_projection() {
    let server = MockServer::start().await;
    let s = setup(&server).await;
    app(&s, "production", "tos>private", "tos", "private").await;
    app(&s, "production", "tos>public", "tos", "public").await;
    app(&s, "production", "other>private", "other", "private").await;
    app(&s, "production", "other>public", "other", "public").await;
    app(&s, ENV, "tos>isolated", "tos", "private").await;
    wire(&server, proof(None)).await;
    let body = "{ \"org_id\": \"tos\", \"limit\": 1 }";
    let (status, first) = send(&s, body, Some("opaque-proof"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(first["items"][0]["app_id"], "tos>private");
    assert_eq!(first["items"][0]["name"], "Accepted name");
    assert_eq!(first["next_cursor"], "tos>private");
    assert_eq!(first["items"][0].as_object().unwrap().len(), 6);
    assert!(!first.to_string().contains("never-expose"));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let verified: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        verified,
        json!({"access_proof":"opaque-proof","request":{"method":"POST","path":ENDPOINT_PATH,"body_sha256":hex::encode(Sha256::digest(body.as_bytes()))}})
    );
    assert!(
        !requests[0]
            .headers
            .contains_key("x-testing-environment-key")
    );
    let (_, next) = send(
        &s,
        r#"{"org_id":"tos","after":"tos>private","limit":1}"#,
        Some("next-proof"),
        None,
    )
    .await;
    assert_eq!(next["items"][0]["app_id"], "tos>public");
    assert!(next["next_cursor"].is_null());
    let (status, _) = send(&s, r#"{"org_id":"other"}"#, Some("other-proof"), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rejects_missing_consumed_revoked_and_invalid_proofs_without_catalog_disclosure() {
    let server = MockServer::start().await;
    let s = setup(&server).await;
    let body = r#"{"org_id":"tos"}"#;
    assert_eq!(send(&s, body, None, None).await.0, StatusCode::UNAUTHORIZED);
    assert!(server.received_requests().await.unwrap().is_empty());
    let consumed = Arc::new(AtomicBool::new(false));
    Mock::given(method("POST")).and(path("/api/v1/obo-access/verify"))
        .respond_with(move |_: &wiremock::Request| {
            if consumed.swap(true, Ordering::SeqCst) {
                ResponseTemplate::new(409).set_body_json(json!({"error":{"code":"obo_proof_consumed","message":"sensitive upstream text","details":null}}))
            } else { ResponseTemplate::new(200).set_body_json(proof(None)) }
        }).mount(&server).await;
    assert_eq!(
        send(&s, body, Some("single-use"), None).await.0,
        StatusCode::OK
    );
    let (status, error) = send(&s, body, Some("single-use"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!error.to_string().contains("sensitive"));
    for code in [
        "obo_proof_expired",
        "obo_authority_revoked",
        "obo_proof_revoked",
        "obo_request_binding_mismatch",
    ] {
        server.reset().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/obo-access/verify"))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(
                    json!({"error":{"code":code,"message":"denied","details":null}}),
                ),
            )
            .mount(&server)
            .await;
        assert_eq!(
            send(&s, body, Some("invalid"), None).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn validates_verified_audience_actor_endpoint_org_metadata_and_environment() {
    let server = MockServer::start().await;
    let s = setup(&server).await;
    for (pointer, wrong) in [
        ("/valid", json!(false)),
        ("/authorization/scopes", json!([])),
        ("/audience", json!("other>app")),
        ("/authorization/audience", json!("other>app")),
        ("/authorization/public_id", json!("other-user")),
        ("/authorization/actor_type", json!("silicon")),
        ("/authorization/org_id", json!("other")),
        ("/authorization/testing_environment_id", json!(ENV)),
        ("/endpoint/endpoint_id", json!("another.endpoint")),
        ("/endpoint/path", json!("/another-path")),
        ("/metadata", json!({"unregistered":"data"})),
        ("/expires_at", json!("2000-01-01T00:00:00Z")),
    ] {
        let mut result = proof(None);
        *result.pointer_mut(pointer).unwrap() = wrong;
        wire(&server, result).await;
        assert_eq!(
            send(&s, r#"{"org_id":"tos"}"#, Some("proof"), None).await.0,
            StatusCode::UNAUTHORIZED,
            "{pointer}"
        );
    }
}

#[tokio::test]
async fn testing_uses_recovered_recipient_credential_and_never_falls_back_to_production() {
    use base64::Engine as _;
    let server = MockServer::start().await;
    let s = setup(&server).await;
    app(&s, ENV, "tos>test-only", "tos", "private").await;
    app(&s, "production", "tos>production-only", "tos", "private").await;
    wire(&server, proof(Some(ENV))).await;
    let (status, result) = send(&s, r#"{"org_id":"tos"}"#, Some("test-proof"), Some(KEY)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["items"][0]["app_id"], "tos>test-only");
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests[0].headers["x-testing-environment-key"], KEY);
    assert_eq!(
        requests[0].headers["authorization"],
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD
                .encode(format!("tos>honeycomb:{TEST_SECRET}"))
        )
    );
    wire(&server, proof(None)).await;
    assert_eq!(
        send(&s, r#"{"org_id":"tos"}"#, Some("prod-proof"), Some(KEY))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let mut mismatch = proof(Some(ENV));
    mismatch["authorization"]["testing_environment_id"] =
        json!("55555555-5555-4555-8555-555555555555");
    wire(&server, mismatch).await;
    assert_eq!(
        send(&s, r#"{"org_id":"tos"}"#, Some("wrong-env"), Some(KEY))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    server.reset().await;
    assert_eq!(
        send(
            &s,
            r#"{"org_id":"tos"}"#,
            Some("proof"),
            Some("invalid-key")
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    sqlx::query("UPDATE environments SET state='deleted' WHERE id=?")
        .bind(ENV)
        .execute(&s.db)
        .await
        .unwrap();
    assert_eq!(
        send(&s, r#"{"org_id":"tos"}"#, Some("proof"), Some(KEY))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn direct_user_catalog_requires_current_membership_and_publishes_critical_definition() {
    let server = MockServer::start().await;
    let s = setup(&server).await;
    app(&s, "production", "tos>private", "tos", "private").await;
    let mut snapshot = proof(None)["authorization"].clone();
    snapshot["org_role"] = json!("member");
    Mock::given(method("POST")).and(path("/api/v1/oauth/introspect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"active":true,"public_id":"carbon-user","client_id":"tos>honeycomb","actor_type":"carbon","authorizations":[snapshot]}))).mount(&server).await;
    for (org, token, expected) in [
        ("tos", None, StatusCode::UNAUTHORIZED),
        ("tos", Some("oat_member"), StatusCode::OK),
        ("other", Some("oat_member"), StatusCode::FORBIDDEN),
    ] {
        let mut request = Request::get(format!("/api/v1/organizations/{org}/apps"));
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let response = silicon_honeycomb_server::api::router(s.clone())
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/oauth/introspect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"active":false})))
        .mount(&server)
        .await;
    let request = Request::get("/api/v1/organizations/tos/apps")
        .header("authorization", "Bearer oat_member")
        .body(Body::empty())
        .unwrap();
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let contract = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(Request::get("/api/contracts").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let contract: Value =
        serde_json::from_slice(&contract.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        contract["obo_endpoints"][0],
        silicon_honeycomb_server::obo::endpoint_definition()
    );
    assert_eq!(
        silicon_honeycomb_server::obo::endpoint_definition()["critical"],
        true
    );
    let definition: Value =
        serde_json::from_str(include_str!("../../../deploy/honeycomb-obo.json")).unwrap();
    assert_eq!(
        definition["obo_endpoints"][0],
        silicon_honeycomb_server::obo::endpoint_definition()
    );
}

#[tokio::test]
async fn silicon_delegation_accepts_undisclosed_identity_and_role_with_current_obo_scope() {
    let server = MockServer::start().await;
    let s = setup(&server).await;
    app(&s, "production", "tos>private", "tos", "private").await;
    let mut result = proof(None);
    result["actor"] = json!({"type":"silicon","public_id":"silicon-user"});
    result["authorization"]
        .as_object_mut()
        .unwrap()
        .remove("actor_type");
    result["authorization"]
        .as_object_mut()
        .unwrap()
        .remove("public_id");
    wire(&server, result).await;
    assert_eq!(
        send(&s, r#"{"org_id":"tos"}"#, Some("silicon-proof"), None)
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn invalid_list_inputs_do_not_consume_proofs_and_verifier_outages_fail_closed() {
    let server = MockServer::start().await;
    let s = setup(&server).await;
    for body in [
        r#"{}"#,
        r#"{"org_id":""}"#,
        r#"{"org_id":"tos","limit":0}"#,
        r#"{"org_id":"tos","limit":101}"#,
        r#"{"org_id":"tos","unbound_field":true}"#,
        r#"{"org_id":"tos","after":""}"#,
        "not json",
    ] {
        assert_eq!(
            send(&s, body, Some("proof"), None).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    Mock::given(method("POST")).and(path("/api/v1/obo-access/verify"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"error":{"code":"service_down","message":"sensitive upstream reason","details":null}}))).mount(&server).await;
    let (status, error) = send(&s, r#"{"org_id":"tos"}"#, Some("proof"), None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(!error.to_string().contains("sensitive"));
    assert!(error.get("items").is_none());
}

#[tokio::test]
async fn key_rotation_during_proof_verification_rejects_the_old_request_context() {
    let server = MockServer::start().await;
    let s = setup(&server).await;
    let started = Arc::new(tokio::sync::Notify::new());
    let started_on_call = started.clone();
    Mock::given(method("POST"))
        .and(path("/api/v1/obo-access/verify"))
        .respond_with(move |_: &wiremock::Request| {
            started_on_call.notify_one();
            ResponseTemplate::new(200)
                .set_body_json(proof(Some(ENV)))
                .set_delay(std::time::Duration::from_millis(300))
        })
        .mount(&server)
        .await;
    let state = s.clone();
    let task =
        tokio::spawn(
            async move { send(&state, r#"{"org_id":"tos"}"#, Some("proof"), Some(KEY)).await },
        );
    tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
        .await
        .unwrap();
    sqlx::query("UPDATE environments SET key_hash='rotated',key_version=key_version+1 WHERE id=?")
        .bind(ENV)
        .execute(&s.db)
        .await
        .unwrap();
    assert_eq!(task.await.unwrap().0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rust_client_preserves_signed_json_and_supports_direct_and_testing_inventories() {
    let server = MockServer::start().await;
    let s = setup(&server).await;
    app(&s, "production", "tos>private", "tos", "private").await;
    app(&s, ENV, "tos>testing", "tos", "private").await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client =
        honeycomb_client::Client::new(&format!("http://{}", listener.local_addr().unwrap()))
            .unwrap()
            .with_telemetry(false);
    let router = silicon_honeycomb_server::api::router(s.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let request = json!({"org_id":"tos","limit":1});
    let signed_digest = hex::encode(Sha256::digest(serde_json::to_vec(&request).unwrap()));
    Mock::given(method("POST")).and(path("/api/v1/obo-access/verify"))
        .and(wiremock::matchers::body_json(json!({"access_proof":"client-proof","request":{"method":"POST","path":ENDPOINT_PATH,"body_sha256":signed_digest}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(proof(None))).expect(1).mount(&server).await;
    // A caller's own bearer token is not a Honeycomb login; only the proof is used.
    let result = client
        .with_token("issuer-token")
        .organization_apps_obo(&request, "client-proof")
        .await
        .unwrap();
    assert_eq!(result["items"][0]["app_id"], "tos>private");
    let mut snapshot = proof(None)["authorization"].clone();
    snapshot["org_role"] = json!("member");
    Mock::given(method("POST")).and(path("/api/v1/oauth/introspect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"active":true,"public_id":"carbon-user","client_id":"tos>honeycomb","actor_type":"carbon","authorizations":[snapshot]}))).expect(2).mount(&server).await;
    let member = client.with_token("oat_member");
    assert_eq!(
        member
            .organization_apps("tos", None, Some(1))
            .await
            .unwrap()["items"][0]["app_id"],
        "tos>private"
    );
    assert!(
        member
            .organization_apps("tos", Some("tos>private"), Some(1))
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    server.verify().await;
    wire(&server, proof(Some(ENV))).await;
    let result = client
        .with_environment(KEY)
        .unwrap()
        .organization_apps_obo(&request, "test-proof")
        .await
        .unwrap();
    assert_eq!(result["items"][0]["app_id"], "tos>testing");
    task.abort();
}
