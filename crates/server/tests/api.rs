use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use silicon_honeycomb_server::{
    State,
    auth::{Identity, IdentityProvider},
    error::{Error, Result},
    integration::{ArchiveStorage, Management},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tower::ServiceExt;

struct Idp {
    revoked: Arc<AtomicBool>,
}
#[async_trait]
impl IdentityProvider for Idp {
    async fn login(&self, slt: &str, _: &str, _: Option<&str>) -> Result<Value> {
        if slt != "valid-slt" {
            return Err(Error::unauthorized());
        }
        Ok(json!({"access_token":"admin","refresh_token":"refresh","expires_in":3600}))
    }
    async fn refresh(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        Ok(json!({"access_token":"admin"}))
    }
    async fn revoke(&self, _: &str, _: &str, _: Option<&str>) -> Result<()> {
        self.revoked.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn authenticate(&self, token: &str, _: Option<&str>) -> Result<Identity> {
        if self.revoked.load(Ordering::SeqCst) || !["admin", "member", "outsider"].contains(&token)
        {
            return Err(Error::unauthorized());
        }
        Ok(Identity {
            principal_id: token.into(),
            actor_type: Some("carbon".into()),
            organizations: BTreeMap::from([(
                if token == "outsider" { "other" } else { "tos" }.into(),
                Some(
                    if token == "member" {
                        "org_member"
                    } else {
                        "org_admin"
                    }
                    .into(),
                ),
            )]),
            testing_environment_id: None,
            validator: false,
        })
    }
}
struct Manager {
    accept: bool,
}
#[async_trait]
impl Management for Manager {
    async fn configure(&self, o: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        if !self.accept {
            return Err(Error::unavailable("IAM integration pending"));
        }
        Ok(
            json!({"state":"accepted","configuration_revision":o["configuration_revision"],"iam_revision":o["expected_iam_revision"].as_i64().unwrap()+1,"effective_configuration":o["configuration"]}),
        )
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::unavailable("pending"))
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
        Ok("entry-id".into())
    }
    async fn read(&self, _: &str, _: Option<&str>, _: Option<&str>) -> Result<Vec<u8>> {
        Ok(vec![])
    }
    async fn publish(&self, reference: &str, _: &str, _: Option<&str>, _: &str) -> Result<String> {
        Ok(reference.into())
    }
}
async fn setup(accept: bool) -> (State, Arc<AtomicBool>) {
    let revoked = Arc::new(AtomicBool::new(false));
    let s = State {
        telemetry: Default::default(),
        db: silicon_honeycomb_server::database("sqlite::memory:")
            .await
            .unwrap(),
        identity: Arc::new(Idp {
            revoked: revoked.clone(),
        }),
        management: Arc::new(Manager { accept }),
        storage: Arc::new(Store),
        app_id: "tos>honeycomb".into(),
        iam_login_url: "https://iam.example.com".into(),
        encryption_key: [7; 32],
        webhook_secret: "test-secret".into(),
    };
    (s, revoked)
}

#[tokio::test]
async fn contracts_negotiate_and_retire_only_after_deprecation_and_seven_idle_days() {
    use silicon_honeycomb_server::{contracts, now};
    let (s, _) = setup(true).await;
    let request = |version: &str, client: &str| {
        Request::builder()
            .uri("/api/v1/iam")
            .header("honeycomb-api-version", version)
            .header("honeycomb-client-version", client)
            .body(Body::empty())
            .unwrap()
    };
    let router = silicon_honeycomb_server::api::router(s.clone());
    assert_eq!(
        router
            .clone()
            .oneshot(request("v2", "0.1.0"))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_ACCEPTABLE
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("v1", "invalid"))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("v1", "0.0.9"))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    // Active contracts never retire just because the service is quiet.
    sqlx::query("UPDATE api_contracts SET last_request_at=1")
        .execute(&s.db)
        .await
        .unwrap();
    contracts::retire_quiet(&s).await.unwrap();
    let response = router
        .clone()
        .oneshot(request("v1", "0.1.0"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["honeycomb-contract-state"], "active");
    let old = now() - contracts::QUIET_PERIOD_SECONDS - 60;
    // Recent traffic postpones retirement even when deprecation is older.
    sqlx::query("UPDATE api_contracts SET state='deprecated',deprecated_at=?")
        .bind(old)
        .execute(&s.db)
        .await
        .unwrap();
    let response = router
        .clone()
        .oneshot(request("v1", "0.1.0"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["honeycomb-contract-state"], "deprecated");
    // A newer deprecation also starts a fresh seven-day interval.
    sqlx::query("UPDATE api_contracts SET deprecated_at=?,last_request_at=?")
        .bind(now())
        .bind(old)
        .execute(&s.db)
        .await
        .unwrap();
    contracts::retire_quiet(&s).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM api_contracts")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        "deprecated"
    );
    sqlx::query("UPDATE api_contracts SET deprecated_at=?,last_request_at=?")
        .bind(old)
        .bind(old)
        .execute(&s.db)
        .await
        .unwrap();
    assert_eq!(
        router
            .clone()
            .oneshot(request("v1", "0.1.0"))
            .await
            .unwrap()
            .status(),
        StatusCode::GONE
    );
    // Discovery remains available, and requests cannot silently resurrect v1.
    let (_, discovery) = call(&s, "GET", "/api/contracts", None, Value::Null, "", None).await;
    assert_eq!(discovery["supported_versions"], json!([]));
    assert_eq!(discovery["versions"][0]["state"], "retired");
    assert_eq!(
        router
            .oneshot(request("v1", "99.0.0"))
            .await
            .unwrap()
            .status(),
        StatusCode::GONE
    );
}

#[tokio::test]
async fn rust_client_consumes_v1_discovery_catalog_and_auth_contracts() {
    let (s, _) = setup(true).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, silicon_honeycomb_server::api::router(s))
            .await
            .unwrap();
    });
    let client = honeycomb_client::Client::new(&origin)
        .unwrap()
        .with_telemetry(false);
    let contract: Value = client.get(&["contract"]).await.unwrap();
    assert_eq!(contract["selected_version"], "v1");
    assert_eq!(contract["minimum_client"], "0.1.0");
    let iam = client.iam().await.unwrap();
    assert_eq!(iam["app_id"], "tos>honeycomb");
    let catalog = client.search("", 1, false).await.unwrap();
    assert_eq!(catalog.total, 0);
    let status = client.with_token("member").login_status().await.unwrap();
    assert_eq!(status["authenticated"], true);
    server.abort();
}
fn input(id: &str) -> Value {
    json!({"org_id":"tos","local_app_id":id,"name":"Honeycomb Test","description":"A useful application for the Silicon ecosystem. ".repeat(8),"webhook_url":"https://example.com/webhook/","webhook_secret":"secret-never-returned-in-catalog-123456","webhook_scope":["membership"],"app_scope":{"iam":["self.identity.read"],"external":[]}})
}
async fn call(
    s: &State,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
    key: &str,
    revision: Option<i64>,
) -> (StatusCode, Value) {
    let mut r = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("idempotency-key", key);
    if let Some(t) = token {
        r = r.header("authorization", format!("Bearer {t}"));
    }
    if let Some(v) = revision {
        r = r.header("if-match", v.to_string());
    }
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(r.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
#[tokio::test]
async fn private_visibility_and_current_membership() {
    let (s, revoked) = setup(true).await;
    let (status, _) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("private"),
        "create-private-0001",
        None,
    )
    .await;
    assert_eq!(status, 202);
    for token in [None, Some("outsider")] {
        let (status, _) = call(
            &s,
            "GET",
            "/api/v1/apps/tos%3Eprivate",
            token,
            Value::Null,
            "unused-unused-0000",
            None,
        )
        .await;
        assert_eq!(status, 404);
        let (_, search) = call(
            &s,
            "GET",
            "/api/v1/apps?q=Honey",
            token,
            Value::Null,
            "unused-unused-0000",
            None,
        )
        .await;
        assert_eq!(search["total"], 0);
    }
    let (status, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Eprivate",
        Some("member"),
        Value::Null,
        "unused-unused-0000",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(!app.to_string().contains("secret-never"));
    assert!(app["config"].get("webhook_secret").is_none());
    revoked.store(true, Ordering::SeqCst);
    assert_eq!(
        call(
            &s,
            "GET",
            "/api/v1/apps/tos%3Eprivate",
            Some("admin"),
            Value::Null,
            "unused-unused-0000",
            None
        )
        .await
        .0,
        401
    );
}
#[tokio::test]
async fn creation_requires_current_org_admin() {
    let (s, _) = setup(true).await;
    for token in [None, Some("member"), Some("outsider")] {
        let (status, _) = call(
            &s,
            "POST",
            "/api/v1/apps",
            token,
            input("a"),
            "create-no-auth-0001",
            None,
        )
        .await;
        assert!(status == 401 || status == 403);
    }
}
#[tokio::test]
async fn idempotency_replays_and_rejects_changed_body() {
    let (s, _) = setup(true).await;
    let (_, first) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("a"),
        "create-replay-0001",
        None,
    )
    .await;
    let (_, again) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("a"),
        "create-replay-0001",
        None,
    )
    .await;
    assert_eq!(first["id"], again["id"]);
    assert_eq!(again["state"], "accepted");
    let (status, _) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("b"),
        "create-replay-0001",
        None,
    )
    .await;
    assert_eq!(status, 409);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM applications")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
#[tokio::test]
async fn pending_iam_does_not_activate_or_publish() {
    let (s, _) = setup(false).await;
    let (_, op) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("pending"),
        "pending-config-0001",
        None,
    )
    .await;
    assert_eq!(op["state"], "pending");
    let (_, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Epending",
        Some("admin"),
        Value::Null,
        "unused-unused-0000",
        None,
    )
    .await;
    assert_eq!(app["iam_revision"], 0);
    assert_eq!(app["visibility"], "private");
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/apps/tos%3Epending/publication",
            Some("admin"),
            json!({"message":"Publish this"}),
            "publish-pending-0001",
            Some(1)
        )
        .await
        .0,
        409
    );
    assert_eq!(
        call(
            &s,
            "GET",
            "/api/v1/apps/tos%3Epending",
            Some("member"),
            Value::Null,
            "unused-unused-0000",
            None
        )
        .await
        .0,
        404
    );
}
#[tokio::test]
async fn stale_revision_cannot_replace_configuration() {
    let (s, _) = setup(true).await;
    call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("a"),
        "create-revision-0001",
        None,
    )
    .await;
    let (status, _) = call(
        &s,
        "PUT",
        "/api/v1/apps/tos%3Ea",
        Some("admin"),
        input("a"),
        "update-revision-0001",
        Some(0),
    )
    .await;
    assert_eq!(status, 409);
    let (status, _) = call(
        &s,
        "PUT",
        "/api/v1/apps/tos%3Ea",
        Some("admin"),
        input("a"),
        "update-revision-0002",
        Some(1),
    )
    .await;
    assert_eq!(status, 202);
}
#[tokio::test]
async fn invalid_test_key_never_falls_back_to_public_catalog() {
    let (s, _) = setup(true).await;
    let request = Request::builder()
        .uri("/api/v1/apps")
        .header("x-testing-environment-key", "invalid")
        .body(Body::empty())
        .unwrap();
    let response = silicon_honeycomb_server::api::router(s)
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}
#[tokio::test]
async fn reviews_are_per_actor_and_stars_are_idempotent() {
    let (s, _) = setup(true).await;
    call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("a"),
        "create-metrics-0001",
        None,
    )
    .await;
    for rating in [3.0, 4.7] {
        assert_eq!(
            call(
                &s,
                "PUT",
                "/api/v1/apps/tos%3Ea/reviews",
                Some("member"),
                json!({"rating":rating,"review":"Useful package"}),
                "review-metrics-0001",
                None
            )
            .await
            .0,
            200
        );
    }
    for _ in 0..2 {
        call(
            &s,
            "PUT",
            "/api/v1/apps/tos%3Ea/star",
            Some("member"),
            Value::Null,
            "stars-metrics-0001",
            None,
        )
        .await;
    }
    let (_, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Ea",
        Some("member"),
        Value::Null,
        "unused-unused-0000",
        None,
    )
    .await;
    assert_eq!(app["rating"], 4.7);
    assert_eq!(app["reviews"], 1);
    assert_eq!(app["stars"], 1);
}

#[tokio::test]
async fn environments_remain_provisioning_and_never_leak_keys_in_lists() {
    let (s, _) = setup(false).await;
    let (status, env) = call(
        &s,
        "POST",
        "/api/v1/environments",
        Some("member"),
        json!({"org_id":"tos","name":"Integration testing"}),
        "create-env-0000001",
        None,
    )
    .await;
    assert_eq!(status, 202);
    assert_eq!(env["state"], "provisioning");
    assert!(env.get("testing_key").is_none());
    let (_, list) = call(
        &s,
        "GET",
        "/api/v1/environments",
        Some("member"),
        Value::Null,
        "unused-unused-0000",
        None,
    )
    .await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert!(!list.to_string().contains("encrypted_key"));
    assert!(!list.to_string().contains("testing_key"));
    let id = env["environment_id"].as_str().unwrap();
    assert_eq!(
        call(
            &s,
            "GET",
            &format!("/api/v1/environments/{id}"),
            Some("outsider"),
            Value::Null,
            "unused-unused-0000",
            None
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(
            &s,
            "POST",
            &format!("/api/v1/environments/{id}/actions/clean"),
            Some("member"),
            Value::Null,
            "clean-env-00000001",
            Some(1)
        )
        .await
        .0,
        409
    );
}
#[tokio::test]
async fn drafts_require_admin_and_detect_concurrent_edits() {
    let (s, _) = setup(false).await;
    let path = "/api/v1/organizations/tos/drafts/first";
    assert_eq!(
        call(
            &s,
            "PUT",
            path,
            Some("member"),
            json!({"name":"First"}),
            "save-draft-000001",
            Some(0)
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            &s,
            "PUT",
            path,
            Some("admin"),
            json!({"name":"First"}),
            "save-draft-000002",
            Some(0)
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(
            &s,
            "PUT",
            path,
            Some("admin"),
            json!({"name":"Second"}),
            "save-draft-000003",
            Some(1)
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(
            &s,
            "PUT",
            path,
            Some("admin"),
            json!({"name":"Stale"}),
            "save-draft-000004",
            Some(1)
        )
        .await
        .0,
        409
    );
    assert_eq!(
        call(
            &s,
            "PUT",
            path,
            Some("admin"),
            json!({"webhook_secret":"sensitive"}),
            "save-draft-000005",
            Some(2)
        )
        .await
        .0,
        400
    );
}

struct RestrictedManager;
#[async_trait]
impl Management for RestrictedManager {
    async fn configure(&self, o: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        let mut effective = o["configuration"].clone();
        effective["name"] = json!("Accepted name");
        effective["app_scope"]["iam"] = json!(["self.identity.read"]);
        effective["app_secret"] = json!("must-not-leak");
        effective["webhook_secret"] = json!("must-not-leak");
        Ok(
            json!({"state":"accepted","configuration_revision":o["configuration_revision"],"iam_revision":1,"effective_configuration":effective}),
        )
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        unreachable!()
    }
}
#[tokio::test]
async fn catalog_uses_iam_accepted_fields_and_excludes_returned_secrets() {
    let (mut s, _) = setup(true).await;
    s.management = Arc::new(RestrictedManager);
    let mut requested = input("accepted-fields");
    requested["app_scope"]["iam"] = json!(["self.identity.read", "self.profile.read"]);
    let (status, _) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        requested,
        "accepted-config-0001",
        None,
    )
    .await;
    assert_eq!(status, 202);
    let (_, visible) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Eaccepted-fields",
        Some("member"),
        Value::Null,
        "unused-unused-0001",
        None,
    )
    .await;
    assert_eq!(visible["name"], "Accepted name");
    assert_eq!(
        visible["config"]["app_scope"]["iam"],
        json!(["self.identity.read"])
    );
    assert!(!visible.to_string().contains("must-not-leak"));
    let (_, desired) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Eaccepted-fields",
        Some("admin"),
        Value::Null,
        "unused-unused-0001",
        None,
    )
    .await;
    assert_eq!(desired["name"], "Honeycomb Test");
}

struct RevocationIdp(std::sync::Mutex<Vec<String>>);
#[async_trait]
impl IdentityProvider for RevocationIdp {
    async fn login(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn refresh(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn authenticate(&self, _: &str, _: Option<&str>) -> Result<Identity> {
        Err(Error::unauthorized())
    }
    async fn revoke(&self, token: &str, _: &str, _: Option<&str>) -> Result<()> {
        self.0.lock().unwrap().push(token.into());
        Ok(())
    }
}
#[tokio::test]
async fn logout_revokes_refresh_family_even_when_access_has_expired() {
    let (mut s, _) = setup(true).await;
    let identity = Arc::new(RevocationIdp(std::sync::Mutex::new(vec![])));
    s.identity = identity.clone();
    let (status, _) = call(
        &s,
        "POST",
        "/api/v1/auth/logout",
        Some("expired-access"),
        json!({"refresh_token":"session-refresh"}),
        "logout-expired-0001",
        None,
    )
    .await;
    assert_eq!(status, 204);
    assert_eq!(
        *identity.0.lock().unwrap(),
        ["session-refresh", "expired-access"]
    );
}

#[tokio::test]
async fn weighted_search_prioritizes_names_over_description_and_tolerates_query_punctuation() {
    let (s, _) = setup(true).await;
    for (id, name, description) in [
        (
            "name-hit",
            "Comet browser",
            "A useful application for teams. ".repeat(12),
        ),
        (
            "description-hit",
            "Another tool",
            "Comet browser helps organize applications for your team. ".repeat(8),
        ),
    ] {
        let mut body = input(id);
        body["name"] = json!(name);
        body["description"] = json!(description);
        assert_eq!(
            call(
                &s,
                "POST",
                "/api/v1/apps",
                Some("admin"),
                body,
                &format!("search-create-{id}"),
                None
            )
            .await
            .0,
            202
        );
    }
    call(
        &s,
        "PUT",
        "/api/v1/apps/tos%3Edescription-hit/reviews",
        Some("member"),
        json!({"rating":5.0,"review":"Excellent"}),
        "search-rating-0001",
        None,
    )
    .await;
    let (status, result) = call(
        &s,
        "GET",
        "/api/v1/apps?q=comet",
        Some("member"),
        Value::Null,
        "unused-unused-0001",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(result["total"], 2);
    assert_eq!(result["items"][0]["app_id"], "tos>name-hit");
    let (_, typo) = call(
        &s,
        "GET",
        "/api/v1/apps?q=comte%20browser",
        Some("member"),
        Value::Null,
        "unused-unused-0001",
        None,
    )
    .await;
    assert_eq!(typo["items"][0]["app_id"], "tos>name-hit");
    assert_eq!(
        call(
            &s,
            "GET",
            "/api/v1/apps?q=%22%20OR%20%2A%20%3A",
            Some("member"),
            Value::Null,
            "unused-unused-0001",
            None
        )
        .await
        .0,
        200
    );
}

#[derive(Default)]
struct Mailbox(std::sync::Mutex<Vec<silicon_honeycomb_server::notifications::Email>>);
#[async_trait]
impl silicon_honeycomb_server::notifications::Mailer for Mailbox {
    async fn send(
        &self,
        email: &silicon_honeycomb_server::notifications::Email,
    ) -> silicon_honeycomb_server::notifications::Delivery {
        self.0.lock().unwrap().push(email.clone());
        silicon_honeycomb_server::notifications::Delivery::Sent("fixture-postmark-receipt".into())
    }
}
#[tokio::test]
async fn reports_queue_once_and_test_notifications_never_leave_the_environment() {
    use silicon_honeycomb_server::notifications::dispatch_once;
    let (s, _) = setup(true).await;
    let body = json!({"message":"The install command fails with this reproduction.","pr":"https://github.com/teamofsilicons/silicon-honeycomb/pull/1"});
    let (status, report) = call(
        &s,
        "POST",
        "/api/v1/reports",
        None,
        body.clone(),
        "report-fixture-0001",
        None,
    )
    .await;
    assert_eq!(status, 202);
    assert_eq!(report["notification"], "pending");
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/reports",
            None,
            body,
            "report-fixture-0001",
            None
        )
        .await
        .1["id"],
        report["id"]
    );
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/reports",
            None,
            json!({"message":"A different report message"}),
            "report-fixture-0001",
            None
        )
        .await
        .0,
        409
    );
    let mailbox = Mailbox::default();
    assert!(dispatch_once(&s, Some(&mailbox)).await.unwrap());
    assert!(!dispatch_once(&s, Some(&mailbox)).await.unwrap());
    assert_eq!(mailbox.0.lock().unwrap().len(), 1);
    assert!(mailbox.0.lock().unwrap()[0].text.contains("/pull/1"));
    sqlx::query("INSERT INTO outbox(id,plane,event_key,kind,payload,created_at) VALUES('test-mail','test-environment','test-mail','report.created','{}',1)").execute(&s.db).await.unwrap();
    assert!(dispatch_once(&s, Some(&mailbox)).await.unwrap());
    let state: String = sqlx::query_scalar("SELECT state FROM outbox WHERE id='test-mail'")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(state, "captured");
    assert_eq!(mailbox.0.lock().unwrap().len(), 1);
}
struct LostMail;
#[async_trait]
impl silicon_honeycomb_server::notifications::Mailer for LostMail {
    async fn send(
        &self,
        _: &silicon_honeycomb_server::notifications::Email,
    ) -> silicon_honeycomb_server::notifications::Delivery {
        silicon_honeycomb_server::notifications::Delivery::Uncertain("Lost receipt".into())
    }
}
#[tokio::test]
async fn uncertain_email_receipts_are_preserved_without_blind_duplicate_delivery() {
    use silicon_honeycomb_server::notifications::dispatch_once;
    let (s, _) = setup(true).await;
    let (_, report) = call(
        &s,
        "POST",
        "/api/v1/reports",
        None,
        json!({"message":"A report with a lost mail delivery response"}),
        "report-lost-0001",
        None,
    )
    .await;
    dispatch_once(&s, Some(&LostMail)).await.unwrap();
    assert!(!dispatch_once(&s, Some(&LostMail)).await.unwrap());
    let state: String = sqlx::query_scalar("SELECT state FROM outbox WHERE id=?")
        .bind(report["id"].as_str().unwrap())
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(state, "uncertain");
}

struct Services {
    fail_dependency: AtomicBool,
    iam_calls: std::sync::atomic::AtomicUsize,
}
#[async_trait]
impl Management for Services {
    async fn configure(&self, o: &Value, t: &str, e: Option<&str>) -> Result<Value> {
        Manager { accept: true }.configure(o, t, e).await
    }
    async fn lifecycle(&self, o: &Value, t: &str) -> Result<Value> {
        self.service_lifecycle("tos>iam", o, t).await
    }
    async fn service_lifecycle(&self, app: &str, o: &Value, _: &str) -> Result<Value> {
        if app == "tos>iam" {
            self.iam_calls.fetch_add(1, Ordering::SeqCst);
        }
        if app == "tos>dependency" && self.fail_dependency.load(Ordering::SeqCst) {
            return Err(Error::unavailable("Dependency is temporarily offline"));
        }
        Ok(
            json!({"state":"completed","operation_id":o["operation_id"],"environment_id":o["environment_id"],"app_id":app,"environment_revision":o["environment_revision"],"generation":o["generation"],"key_version":o["key_version"]}),
        )
    }
}
#[tokio::test]
async fn lifecycle_requires_all_services_and_retries_only_incomplete_steps() {
    let (mut s, _) = setup(true).await;
    let services = Arc::new(Services {
        fail_dependency: AtomicBool::new(true),
        iam_calls: std::sync::atomic::AtomicUsize::new(0),
    });
    s.management = services.clone();
    let (_, env) = call(
        &s,
        "POST",
        "/api/v1/environments",
        Some("member"),
        json!({"org_id":"tos","name":"Lifecycle"}),
        "lifecycle-create-0001",
        None,
    )
    .await;
    assert_eq!(env["state"], "ready");
    let id = env["environment_id"].as_str().unwrap();
    let op = env["operation_id"].as_str().unwrap();
    sqlx::query("UPDATE operations SET state='pending' WHERE id=?")
        .bind(op)
        .execute(&s.db)
        .await
        .unwrap();
    sqlx::query("UPDATE environments SET state='provisioning' WHERE id=?")
        .bind(id)
        .execute(&s.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO environment_services(environment_id,app_id,source_revision,snapshot,state,operation_id,generation) VALUES(?,'tos>dependency',1,'{}','pending',?,1)").bind(id).bind(op).execute(&s.db).await.unwrap();
    let pending = silicon_honeycomb_server::lifecycle::coordinate(&s, id, op, "member")
        .await
        .unwrap();
    assert_eq!(pending["state"], "provisioning");
    assert_eq!(services.iam_calls.load(Ordering::SeqCst), 1);
    services.fail_dependency.store(false, Ordering::SeqCst);
    let (_, ready) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/actions/retry"),
        Some("member"),
        Value::Null,
        "lifecycle-retry-0001",
        Some(1),
    )
    .await;
    assert_eq!(ready["state"], "ready");
    assert_eq!(services.iam_calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn lifecycle_clean_rotation_restore_and_retention_purge_are_isolated() {
    let (mut s, _) = setup(true).await;
    s.management = Arc::new(Services {
        fail_dependency: AtomicBool::new(false),
        iam_calls: std::sync::atomic::AtomicUsize::new(0),
    });
    call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("production-survivor"),
        "lifecycle-prod-app-0001",
        None,
    )
    .await;
    let (_, env) = call(
        &s,
        "POST",
        "/api/v1/environments",
        Some("member"),
        json!({"org_id":"tos","name":"Lifecycle"}),
        "lifecycle-create-0002",
        None,
    )
    .await;
    let id = env["environment_id"].as_str().unwrap();
    let key_path = format!("/api/v1/environments/{id}/key");
    let (_, key) = call(
        &s,
        "POST",
        &key_path,
        Some("member"),
        Value::Null,
        "lifecycle-read-key-0001",
        None,
    )
    .await;
    let original = key["testing_key"].as_str().unwrap();
    sqlx::query(
        "INSERT INTO telemetry_events(plane,generation,event,created_at) VALUES(?,1,'{}',1)",
    )
    .bind(id)
    .execute(&s.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,config,effective_config,webhook_secret,state,iam_revision,created_at,updated_at) SELECT ?,app_id,org_id,name,description,config,effective_config,webhook_secret,state,iam_revision,created_at,updated_at FROM applications WHERE plane='production'").bind(id).execute(&s.db).await.unwrap();
    for (action, revision, expected_state) in [
        ("clean", 1, "ready"),
        ("rotate-key", 2, "ready"),
        ("delete", 3, "deleted"),
        ("restore", 4, "ready"),
        ("delete", 5, "deleted"),
    ] {
        let (status, result) = call(
            &s,
            "POST",
            &format!("/api/v1/environments/{id}/actions/{action}"),
            Some("member"),
            Value::Null,
            &format!("lifecycle-{action}-{revision:04}"),
            Some(revision),
        )
        .await;
        assert_eq!(status, 202, "{result}");
        assert_eq!(result["state"], expected_state);
        if action == "clean" {
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM telemetry_events WHERE plane=?")
                    .bind(id)
                    .fetch_one(&s.db)
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(result["generation"], 2);
            let test_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM applications WHERE plane=?")
                    .bind(id)
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            assert_eq!(test_count, 0);
            let count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM applications WHERE plane='production'")
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            assert_eq!(count, 1);
        }
        if action == "rotate-key" {
            assert_ne!(result["testing_key"], original);
            assert_eq!(result["key_version"], 2);
            let request = Request::builder()
                .uri("/api/v1/apps")
                .header("x-testing-environment-key", original)
                .body(Body::empty())
                .unwrap();
            let response = silicon_honeycomb_server::api::router(s.clone())
                .oneshot(request)
                .await
                .unwrap();
            assert_eq!(response.status(), 400);
        }
    }
    let path = format!("/api/v1/environments/{id}/actions/purge");
    assert_eq!(
        call(
            &s,
            "POST",
            &path,
            Some("member"),
            Value::Null,
            "lifecycle-purge-early-0001",
            Some(6)
        )
        .await
        .0,
        409
    );
    sqlx::query("UPDATE environments SET purge_after=1 WHERE id=?")
        .bind(id)
        .execute(&s.db)
        .await
        .unwrap();
    assert_eq!(
        call(
            &s,
            "POST",
            &path,
            Some("member"),
            Value::Null,
            "lifecycle-purge-0001",
            Some(6)
        )
        .await
        .1["state"],
        "purged"
    );
    let encrypted: String = sqlx::query_scalar("SELECT encrypted_key FROM environments WHERE id=?")
        .bind(id)
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert!(encrypted.is_empty());
}

#[derive(Default)]
struct ImportServices {
    malformed: AtomicBool,
    offline: AtomicBool,
    calls: std::sync::Mutex<Vec<String>>,
}
#[async_trait]
impl Management for ImportServices {
    async fn configure(&self, o: &Value, t: &str, e: Option<&str>) -> Result<Value> {
        Manager { accept: true }.configure(o, t, e).await
    }
    async fn lifecycle(&self, o: &Value, t: &str) -> Result<Value> {
        self.service_lifecycle("tos>iam", o, t).await
    }
    async fn service_lifecycle(&self, app: &str, o: &Value, _: &str) -> Result<Value> {
        self.calls.lock().unwrap().push(app.into());
        if app == "other>dependency" && self.offline.load(Ordering::SeqCst) {
            return Err(Error::unavailable("Service offline"));
        }
        let imports:Vec<Value>=o["snapshot"]["imports"].as_array().into_iter().flatten().map(|source|json!({"app_id":source["app_id"],"source_revision":source["source_revision"],"configuration_revision":source["configuration_revision"],"iam_revision":source["configuration_revision"],"effective_configuration":source["configuration"],"visibility":"private","app_secret":"never-persist-this-test-secret"})).collect();
        Ok(
            json!({"state":"completed","operation_id":o["operation_id"],"environment_id":o["environment_id"],"app_id":app,"environment_revision":o["environment_revision"],"generation":if self.malformed.load(Ordering::SeqCst){json!(999)}else{o["generation"].clone()},"key_version":o["key_version"],"imports":imports}),
        )
    }
}
async fn import_source(s: &State, id: &str, deps: &[&str], public: bool) {
    let (org, local) = id.split_once('>').unwrap();
    let mut config = input(local);
    config["org_id"] = json!(org);
    config["app_scope"]["external"] = json!(
        deps.iter()
            .map(|app| json!({"app_id":app,"endpoint_id":"files.read"}))
            .collect::<Vec<_>>()
    );
    let (status, response) = call(
        s,
        "POST",
        "/api/v1/apps",
        Some(if org == "tos" { "admin" } else { "outsider" }),
        config,
        &format!("source-create-{id}-0001"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{response}");
    if public {
        sqlx::query(
            "UPDATE applications SET visibility='public' WHERE app_id=? AND plane='production'",
        )
        .bind(id)
        .execute(&s.db)
        .await
        .unwrap();
    }
}
#[tokio::test]
async fn imports_pin_accepted_configs_follow_cycles_and_preserve_organizations() {
    let (mut s, _) = setup(true).await;
    let services = Arc::new(ImportServices::default());
    s.management = services.clone();
    import_source(&s, "tos>root", &["other>dependency", "tos>leaf"], false).await;
    import_source(&s, "other>dependency", &["tos>leaf"], true).await;
    import_source(&s, "tos>leaf", &["tos>root"], false).await;
    let (_, env) = call(
        &s,
        "POST",
        "/api/v1/environments",
        Some("member"),
        json!({"org_id":"tos","name":"Imports"}),
        "imports-create-0001",
        None,
    )
    .await;
    let id = env["environment_id"].as_str().unwrap();
    let path = format!("/api/v1/environments/{id}/imports");
    services.calls.lock().unwrap().clear();
    let (status, result) = call(
        &s,
        "POST",
        &path,
        Some("member"),
        json!({"app_id":"tos>root"}),
        "imports-root-0001",
        Some(1),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{result}");
    assert_eq!(result["operation_state"], "accepted", "{result}");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM environment_imports WHERE environment_id=?")
            .bind(id)
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(count, 3);
    let calls = services.calls.lock().unwrap().clone();
    assert_eq!(calls[0], "tos>iam");
    assert_eq!(calls.iter().filter(|s| *s == "tos>leaf").count(), 1);
    let org: String = sqlx::query_scalar(
        "SELECT org_id FROM applications WHERE plane=? AND app_id='other>dependency'",
    )
    .bind(id)
    .fetch_one(&s.db)
    .await
    .unwrap();
    assert_eq!(org, "other");
    let key: String = sqlx::query_scalar(
        "SELECT webhook_secret FROM applications WHERE plane=? AND app_id='tos>root'",
    )
    .bind(id)
    .fetch_one(&s.db)
    .await
    .unwrap();
    assert!(key.is_empty());
    let receipt: String = sqlx::query_scalar(
        "SELECT receipt FROM environment_services WHERE environment_id=? AND app_id='tos>iam'",
    )
    .bind(id)
    .fetch_one(&s.db)
    .await
    .unwrap();
    assert!(!receipt.contains("never-persist"));
    // A requested production revision must not silently change an accepted import.
    sqlx::query("UPDATE applications SET revision=revision+1,config=json_set(config,'$.name','Unaccepted title') WHERE plane='production' AND app_id='tos>root'").execute(&s.db).await.unwrap();
    let (_, unchanged) = call(
        &s,
        "POST",
        &path,
        Some("member"),
        json!({"app_id":"tos>root"}),
        "imports-root-0002",
        Some(2),
    )
    .await;
    assert_eq!(unchanged["unchanged"], true);
    let (_, refreshed) = call(
        &s,
        "POST",
        &path,
        Some("member"),
        json!({"app_id":"tos>root","refresh":true}),
        "imports-refresh-0001",
        Some(2),
    )
    .await;
    assert_eq!(refreshed["operation_state"], "accepted", "{refreshed}");
    let source:i64=sqlx::query_scalar("SELECT source_revision FROM environment_imports WHERE environment_id=? AND app_id='tos>root'").bind(id).fetch_one(&s.db).await.unwrap();
    assert_eq!(source, 1);
    let name: String =
        sqlx::query_scalar("SELECT name FROM applications WHERE plane=? AND app_id='tos>root'")
            .bind(id)
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_ne!(name, "Unaccepted title");
    let (_, replay) = call(
        &s,
        "POST",
        &path,
        Some("member"),
        json!({"app_id":"tos>root","refresh":true}),
        "imports-refresh-0001",
        Some(2),
    )
    .await;
    assert_eq!(replay["operation_id"], refreshed["operation_id"]);
    let (status, _) = call(
        &s,
        "POST",
        &path,
        Some("member"),
        json!({"app_id":"tos>leaf","refresh":true}),
        "imports-refresh-0001",
        Some(2),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}
#[tokio::test]
async fn imports_reject_private_dependencies_and_gate_new_records_on_exact_receipts() {
    let (mut s, _) = setup(true).await;
    let services = Arc::new(ImportServices::default());
    s.management = services.clone();
    import_source(&s, "tos>root", &["other>dependency"], true).await;
    import_source(&s, "other>dependency", &[], false).await;
    let (_, env) = call(
        &s,
        "POST",
        "/api/v1/environments",
        Some("member"),
        json!({"org_id":"tos","name":"Import gates"}),
        "imports-create-0002",
        None,
    )
    .await;
    let id = env["environment_id"].as_str().unwrap();
    let path = format!("/api/v1/environments/{id}/imports");
    let (status, _) = call(
        &s,
        "POST",
        &path,
        Some("member"),
        json!({"app_id":"tos>root"}),
        "imports-private-0001",
        Some(1),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM environment_imports WHERE environment_id=?")
            .bind(id)
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(count, 0);
    sqlx::query("UPDATE applications SET visibility='public' WHERE plane='production' AND app_id='other>dependency'").execute(&s.db).await.unwrap();
    services.malformed.store(true, Ordering::SeqCst);
    services.calls.lock().unwrap().clear();
    let (_, pending) = call(
        &s,
        "POST",
        &path,
        Some("member"),
        json!({"app_id":"tos>root"}),
        "imports-private-0002",
        Some(1),
    )
    .await;
    assert_eq!(pending["operation_state"], "pending");
    assert_eq!(pending["state"], "ready");
    assert_eq!(*services.calls.lock().unwrap(), vec!["tos>iam"]);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM applications WHERE plane=?")
        .bind(id)
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    services.malformed.store(false, Ordering::SeqCst);
    services.offline.store(true, Ordering::SeqCst);
    let (_, pending) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/actions/retry"),
        Some("member"),
        Value::Null,
        "imports-retry-0001",
        Some(2),
    )
    .await;
    assert_eq!(pending["operation_state"], "pending");
    let (status, _) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/actions/clean"),
        Some("member"),
        Value::Null,
        "imports-clean-0001",
        Some(2),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    services.offline.store(false, Ordering::SeqCst);
    services.calls.lock().unwrap().clear();
    let (_, ready) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/actions/retry"),
        Some("member"),
        Value::Null,
        "imports-retry-0002",
        Some(2),
    )
    .await;
    assert_eq!(ready["operation_state"], "accepted", "{ready}");
    assert_eq!(*services.calls.lock().unwrap(), vec!["other>dependency"]);
}

#[tokio::test]
async fn import_graph_has_no_fixed_depth_limit_and_root_key_grants_no_production_membership() {
    let (mut s, _) = setup(true).await;
    s.management = Arc::new(ImportServices::default());
    for n in 0..40 {
        let id = format!("tos>node-{n}");
        let dependency = format!("tos>node-{}", (n + 1) % 40);
        import_source(&s, &id, &[&dependency], true).await;
    }
    import_source(&s, "tos>hidden", &[], false).await;
    let (_, env) = call(
        &s,
        "POST",
        "/api/v1/environments",
        Some("member"),
        json!({"org_id":"tos","name":"Root import graph"}),
        "imports-create-0003",
        None,
    )
    .await;
    let id = env["environment_id"].as_str().unwrap();
    let encrypted: String = sqlx::query_scalar("SELECT encrypted_key FROM environments WHERE id=?")
        .bind(id)
        .fetch_one(&s.db)
        .await
        .unwrap();
    let root = s.decrypt(&encrypted).unwrap();
    for (app, status) in [
        ("tos>hidden", StatusCode::NOT_FOUND),
        ("tos>node-0", StatusCode::ACCEPTED),
    ] {
        let response = silicon_honeycomb_server::api::router(s.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/environments/{id}/imports"))
                    .header("content-type", "application/json")
                    .header("x-testing-environment-key", &root)
                    .header("if-match", "1")
                    .header("idempotency-key", format!("root-import-{app}-0001"))
                    .body(Body::from(json!({"app_id":app}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM environment_imports WHERE environment_id=?")
            .bind(id)
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(count, 40);
}

#[derive(Default)]
struct SecretManager {
    results: std::sync::Mutex<BTreeMap<String, Value>>,
    lose_response: AtomicBool,
}
#[async_trait]
impl Management for SecretManager {
    async fn configure(&self, o: &Value, t: &str, e: Option<&str>) -> Result<Value> {
        let mut result = Manager { accept: true }.configure(o, t, e).await?;
        result["operation_id"] = o["operation_id"].clone();
        result["app_id"] = o["app_id"].clone();
        result["app_secret"] = json!("creation-secret-never-persist");
        self.results
            .lock()
            .unwrap()
            .insert(o["operation_id"].as_str().unwrap().into(), result.clone());
        Ok(result)
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::unavailable("pending"))
    }
    async fn rotate_secret(
        &self,
        o: &Value,
        _: &str,
        step_up: Option<&str>,
        _: Option<&str>,
    ) -> Result<Value> {
        if step_up != Some("fresh-step-up-never-persist") {
            return Err(Error::new(
                StatusCode::FORBIDDEN,
                "step_up_required",
                "Fresh IAM verification is required",
            ));
        }
        let mut results = self.results.lock().unwrap();
        let id = o["operation_id"].as_str().unwrap();
        let response=results.entry(id.into()).or_insert_with(||json!({"operation_id":id,"app_id":o["app_id"],"configuration_revision":o["configuration_revision"],"state":"accepted","iam_revision":o["expected_iam_revision"].as_i64().unwrap()+1,"credential_version":2,"app_secret":"rotation-secret-never-persist"})).clone();
        if self.lose_response.swap(false, Ordering::SeqCst) {
            return Err(Error::unavailable("Lost response"));
        }
        Ok(response)
    }
    async fn operation_result(&self, id: &str, _: &str, _: Option<&str>) -> Result<Value> {
        self.results
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(Error::missing)
    }
}
#[tokio::test]
async fn secret_rotation_requires_current_admin_step_up_and_replays_without_storing_secrets() {
    let (mut s, _) = setup(true).await;
    let manager = Arc::new(SecretManager::default());
    s.management = manager.clone();
    let (_, created) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("secrets"),
        "secrets-create-0001",
        None,
    )
    .await;
    let (_, again) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("secrets"),
        "secrets-create-0001",
        None,
    )
    .await;
    assert_eq!(again["app_secret"], created["app_secret"]);
    let path = "/api/v1/apps/tos%3Esecrets/secret-rotations";
    assert_eq!(
        call(
            &s,
            "POST",
            path,
            Some("member"),
            json!({}),
            "secret-rotate-member-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &s,
            "POST",
            path,
            Some("admin"),
            json!({}),
            "secret-rotate-stale-0001",
            Some(0)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, pending) = call(
        &s,
        "POST",
        path,
        Some("admin"),
        json!({}),
        "secret-rotate-0001",
        Some(1),
    )
    .await;
    assert_eq!(pending["state"], "pending");
    assert_eq!(pending["error_code"], "step_up_required");
    assert_eq!(
        call(
            &s,
            "PUT",
            "/api/v1/apps/tos%3Esecrets",
            Some("admin"),
            input("secrets"),
            "secret-update-blocked-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, accepted) = call(
        &s,
        "POST",
        path,
        Some("admin"),
        json!({"step_up_assertion":"fresh-step-up-never-persist"}),
        "secret-rotate-0001",
        Some(1),
    )
    .await;
    assert_eq!(accepted["state"], "accepted");
    assert_eq!(accepted["app_secret"], "rotation-secret-never-persist");
    let (_, replay) = call(
        &s,
        "POST",
        path,
        Some("admin"),
        json!({}),
        "secret-rotate-0001",
        Some(1),
    )
    .await;
    assert_eq!(replay["app_secret"], accepted["app_secret"]);
    assert_eq!(manager.results.lock().unwrap().len(), 2);
    let rows: Vec<(Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT request_json,result,error FROM operations")
            .fetch_all(&s.db)
            .await
            .unwrap();
    assert!(!format!("{rows:?}").contains("never-persist"));
    let (_, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Esecrets",
        Some("admin"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert!(!app.to_string().contains("never-persist"));
    let id = accepted["id"].as_str().unwrap();
    assert_eq!(
        call(
            &s,
            "POST",
            &format!("/api/v1/operations/{id}/result"),
            Some("outsider"),
            json!({}),
            "secret-read-denied-0001",
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // Expiry returns no old credential and never causes a fresh rotation.
    manager
        .results
        .lock()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("app_secret");
    let (_, expired) = call(
        &s,
        "POST",
        &format!("/api/v1/operations/{id}/result"),
        Some("admin"),
        json!({}),
        "secret-read-expired-0001",
        None,
    )
    .await;
    assert!(expired.get("app_secret").is_none());
    assert!(expired["message"].as_str().unwrap().contains("expired"));
}
#[tokio::test]
async fn lost_rotation_response_recovers_original_result_and_unblocks_configuration() {
    let (mut s, _) = setup(true).await;
    let manager = Arc::new(SecretManager::default());
    s.management = manager.clone();
    call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("lost-secret"),
        "secret-lost-create-0001",
        None,
    )
    .await;
    manager.lose_response.store(true, Ordering::SeqCst);
    let (_, pending) = call(
        &s,
        "POST",
        "/api/v1/apps/tos%3Elost-secret/secret-rotations",
        Some("admin"),
        json!({"step_up_assertion":"fresh-step-up-never-persist"}),
        "secret-lost-rotate-0001",
        Some(1),
    )
    .await;
    assert_eq!(pending["state"], "pending");
    let id = pending["id"].as_str().unwrap();
    let (_, result) = call(
        &s,
        "POST",
        &format!("/api/v1/operations/{id}/result"),
        Some("admin"),
        json!({}),
        "secret-lost-recover-0001",
        None,
    )
    .await;
    assert_eq!(result["state"], "accepted");
    assert_eq!(result["app_secret"], "rotation-secret-never-persist");
    let (_, op) = call(
        &s,
        "GET",
        &format!("/api/v1/operations/{id}"),
        Some("admin"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(op["state"], "accepted");
    assert_eq!(
        call(
            &s,
            "PUT",
            "/api/v1/apps/tos%3Elost-secret",
            Some("admin"),
            input("lost-secret"),
            "secret-lost-update-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
}

struct ReviewIdentity;
#[async_trait]
impl IdentityProvider for ReviewIdentity {
    async fn login(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unauthorized())
    }
    async fn refresh(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unauthorized())
    }
    async fn revoke(&self, _: &str, _: &str, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    async fn authenticate(&self, token: &str, env: Option<&str>) -> Result<Identity> {
        if ["validator", "iam-reviewer"].contains(&token) {
            return Ok(Identity {
                principal_id: token.into(),
                actor_type: Some("silicon".into()),
                organizations: BTreeMap::new(),
                testing_environment_id: None,
                validator: token == "validator",
            });
        }
        Idp {
            revoked: Arc::new(AtomicBool::new(false)),
        }
        .authenticate(token, env)
        .await
    }
}
#[derive(Default)]
struct ReviewManager {
    accept: AtomicBool,
}
#[async_trait]
impl Management for ReviewManager {
    async fn configure(&self, o: &Value, t: &str, e: Option<&str>) -> Result<Value> {
        Manager { accept: true }.configure(o, t, e).await
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::unavailable("pending"))
    }
    async fn publication_plan(&self, r: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        Ok(
            json!({"state":"accepted","request_id":r["request_id"],"app_id":r["app_id"],"configuration_revision":r["configuration_revision"],"plan_id":format!("plan-{}",r["request_id"].as_str().unwrap()),"gates":[{"provider":"iam","scopes":["directory.carbons.read"]},{"provider":"other>provider","scopes":["files.read"]}]}),
        )
    }
    async fn iam_scope_reviewer(&self, token: &str, _: Option<&str>) -> Result<bool> {
        Ok(token == "iam-reviewer")
    }
    async fn review_decision(&self, o: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        if !self.accept.load(Ordering::SeqCst) {
            return Err(Error::unavailable("Decision pending"));
        }
        let mut response = o.clone();
        response["state"] = json!("accepted");
        Ok(response)
    }
}
async fn review_application(s: &State) -> String {
    import_source(s, "other>provider", &[], true).await;
    let mut config = input("review-app");
    config["app_scope"] = json!({"iam":["self.identity.read","directory.carbons.read"],"external":[{"app_id":"other>provider","endpoint_id":"files.read"}]});
    call(
        s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        config,
        "review-app-create-0001",
        None,
    )
    .await;
    sqlx::query("INSERT INTO releases(plane,app_id,version,sha256,size,storage_ref,created_at) VALUES('production','tos>review-app','1.0.0','fixture-only',1,'fixture-release',1)").execute(&s.db).await.unwrap();
    let (status, result) = call(
        s,
        "POST",
        "/api/v1/apps/tos%3Ereview-app/publication",
        Some("admin"),
        json!({"message":"Please review these requested public scopes."}),
        "review-request-0001",
        Some(1),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{result}");
    result["id"].as_str().unwrap().into()
}
#[tokio::test]
async fn review_authority_discussions_and_provider_before_validator_order_are_enforced() {
    let (mut s, _) = setup(true).await;
    let manager = Arc::new(ReviewManager::default());
    s.management = manager.clone();
    s.identity = Arc::new(ReviewIdentity);
    let id = review_application(&s).await;
    let provider_path = format!("/api/v1/review-requests/{id}/other%3Eprovider/decisions");
    let validator_path = format!("/api/v1/review-requests/{id}/honeycomb/decisions");
    let (_, inbox) = call(
        &s,
        "GET",
        "/api/v1/review-requests",
        Some("outsider"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(inbox["items"][0]["app_id"], "tos>review-app");
    let (_, inbox) = call(
        &s,
        "GET",
        "/api/v1/review-requests",
        Some("validator"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(inbox["items"].as_array().unwrap().len(), 0);
    assert_eq!(
        call(
            &s,
            "POST",
            &provider_path,
            Some("admin"),
            json!({"decision":"approve"}),
            "review-owner-cannot-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &s,
            "POST",
            &validator_path,
            Some("validator"),
            json!({"decision":"approve"}),
            "review-validator-early-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let message_path = format!("/api/v1/review-requests/{id}/other%3Eprovider/messages");
    let (_, reply) = call(
        &s,
        "POST",
        &message_path,
        Some("outsider"),
        json!({"message":"Explain how files are selected."}),
        "review-provider-reply-0001",
        None,
    )
    .await;
    let (_, again) = call(
        &s,
        "POST",
        &message_path,
        Some("outsider"),
        json!({"message":"Explain how files are selected."}),
        "review-provider-reply-0001",
        None,
    )
    .await;
    assert_eq!(reply["id"], again["id"]);
    let (_, detail) = call(
        &s,
        "GET",
        &format!("/api/v1/review-requests/{id}/other%3Eprovider"),
        Some("admin"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert!(detail.to_string().contains("Explain how files"));
    assert_eq!(
        call(
            &s,
            "POST",
            &provider_path,
            Some("outsider"),
            json!({"decision":"deny"}),
            "review-denial-reason-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (_, pending) = call(
        &s,
        "POST",
        &provider_path,
        Some("outsider"),
        json!({"decision":"approve"}),
        "review-provider-approve-0001",
        Some(1),
    )
    .await;
    assert_eq!(pending["state"], "pending");
    manager.accept.store(true, Ordering::SeqCst);
    let (_, accepted) = call(
        &s,
        "POST",
        &provider_path,
        Some("outsider"),
        json!({"decision":"approve"}),
        "review-provider-approve-0001",
        Some(1),
    )
    .await;
    assert_eq!(accepted["publication_state"], "awaiting_scope_review");
    let (_, accepted) = call(
        &s,
        "POST",
        &format!("/api/v1/review-requests/{id}/iam/decisions"),
        Some("iam-reviewer"),
        json!({"decision":"approve"}),
        "review-iam-approve-0001",
        Some(1),
    )
    .await;
    assert_eq!(accepted["publication_state"], "awaiting_validator");
    let (_, inbox) = call(
        &s,
        "GET",
        "/api/v1/review-requests",
        Some("validator"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(inbox["items"][0]["provider"], "honeycomb");
    let (_, accepted) = call(
        &s,
        "POST",
        &validator_path,
        Some("validator"),
        json!({"decision":"approve"}),
        "review-validator-approve-0001",
        Some(1),
    )
    .await;
    assert_eq!(accepted["publication_state"], "awaiting_activation");
    let (_, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Ereview-app",
        Some("admin"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(app["visibility"], "private");
    let (_, replay) = call(
        &s,
        "POST",
        "/api/v1/apps/tos%3Ereview-app/publication",
        Some("admin"),
        json!({"message":"Please review these requested public scopes."}),
        "review-request-0001",
        Some(1),
    )
    .await;
    assert_eq!(replay["id"], id);
    assert_eq!(replay["state"], "awaiting_activation");
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/apps/tos%3Ereview-app/publication",
            Some("admin"),
            json!({"message":"Different request"}),
            "review-request-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}
#[tokio::test]
async fn review_denials_preserve_private_visibility_and_stale_decisions_cannot_apply() {
    let (mut s, _) = setup(true).await;
    s.management = Arc::new(ReviewManager {
        accept: AtomicBool::new(true),
    });
    s.identity = Arc::new(ReviewIdentity);
    let id = review_application(&s).await;
    let (_, denied) = call(
        &s,
        "POST",
        &format!("/api/v1/review-requests/{id}/other%3Eprovider/decisions"),
        Some("outsider"),
        json!({"decision":"deny","reason":"Narrow the requested file access."}),
        "review-provider-deny-0001",
        Some(1),
    )
    .await;
    assert_eq!(denied["publication_state"], "denied");
    sqlx::query(
        "UPDATE applications SET revision=2 WHERE plane='production' AND app_id='tos>review-app'",
    )
    .execute(&s.db)
    .await
    .unwrap();
    assert_eq!(
        call(
            &s,
            "POST",
            &format!("/api/v1/review-requests/{id}/iam/decisions"),
            Some("iam-reviewer"),
            json!({"decision":"approve"}),
            "review-stale-approve-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Ereview-app",
        Some("admin"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(app["visibility"], "private");
}

#[tokio::test]
async fn public_apps_can_review_requested_scopes_while_older_scopes_remain_effective() {
    let (mut s, _) = setup(true).await;
    s.management = Arc::new(PublicationManager::default());
    s.storage = Arc::new(PublicationStorage::default());
    s.identity = Arc::new(ReviewIdentity);
    review_application(&s).await;
    sqlx::query("UPDATE applications SET visibility='public',revision=2 WHERE plane='production' AND app_id='tos>review-app'").execute(&s.db).await.unwrap();
    let (status, result) = call(
        &s,
        "POST",
        "/api/v1/apps/tos%3Ereview-app/publication",
        Some("admin"),
        json!({"message":"Review this public app's additional requested scopes."}),
        "review-public-addition-0001",
        Some(2),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{result}");
    assert_eq!(result["state"], "awaiting_scope_review");
    assert_eq!(result["visibility"], "public");
    let (_, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Ereview-app",
        Some("admin"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(app["effective_revision"], 1);
    assert_eq!(app["revision"], 2);
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES('pending-public-config','production','admin','pending-public-config-key','configure','tos>review-app','test-hash',2,1)").execute(&s.db).await.unwrap();
    let id = result["id"].as_str().unwrap();
    for (provider, actor) in [
        ("other%3Eprovider", "outsider"),
        ("iam", "iam-reviewer"),
        ("honeycomb", "validator"),
    ] {
        let (status, body) = call(
            &s,
            "POST",
            &format!("/api/v1/review-requests/{id}/{provider}/decisions"),
            Some(actor),
            json!({"decision":"approve"}),
            &format!("public-revision-two-{provider}"),
            Some(2),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (_, activated) = call(
        &s,
        "POST",
        &format!("/api/v1/review-requests/{id}/activate"),
        Some("admin"),
        Value::Null,
        "public-revision-two-activate",
        Some(2),
    )
    .await;
    assert_eq!(activated["state"], "accepted", "{activated}");
    let state: String =
        sqlx::query_scalar("SELECT state FROM operations WHERE id='pending-public-config'")
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(state, "accepted");
    let (_, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Ereview-app",
        None,
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(app["effective_revision"], 2);
}

#[derive(Default)]
struct PublicationManager {
    calls: std::sync::atomic::AtomicUsize,
    state: std::sync::Mutex<Option<Value>>,
    revoked: AtomicBool,
    wrong_receipt: AtomicBool,
}
#[async_trait]
impl Management for PublicationManager {
    async fn configure(&self, o: &Value, t: &str, e: Option<&str>) -> Result<Value> {
        Manager { accept: true }.configure(o, t, e).await
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::unavailable("pending"))
    }
    async fn publication_plan(&self, r: &Value, t: &str, e: Option<&str>) -> Result<Value> {
        ReviewManager::default().publication_plan(r, t, e).await
    }
    async fn iam_scope_reviewer(&self, t: &str, _: Option<&str>) -> Result<bool> {
        Ok(t == "iam-reviewer")
    }
    async fn review_decision(&self, o: &Value, t: &str, e: Option<&str>) -> Result<Value> {
        ReviewManager {
            accept: AtomicBool::new(true),
        }
        .review_decision(o, t, e)
        .await
    }
    async fn activate_publication(&self, o: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let response = json!({"state":"accepted","operation_id":if self.wrong_receipt.load(Ordering::SeqCst){json!("wrong-operation")}else{o["operation_id"].clone()},"request_id":o["request_id"],"app_id":o["app_id"],"configuration_revision":o["configuration_revision"],"iam_revision":o["expected_iam_revision"].as_i64().unwrap()+1,"visibility":"public","effective_configuration":o["configuration"]});
        *self.state.lock().unwrap() = Some(response.clone());
        Ok(response)
    }
    async fn application_state(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        let mut state = self.state.lock().unwrap().clone().unwrap();
        state["publication_request_id"] = state["request_id"].clone();
        if self.revoked.load(Ordering::SeqCst) {
            state["visibility"] = json!("private");
        }
        Ok(state)
    }
}
#[derive(Default)]
struct PublicationStorage {
    fail_second: AtomicBool,
    calls: std::sync::Mutex<BTreeMap<String, usize>>,
}
#[async_trait]
impl ArchiveStorage for PublicationStorage {
    async fn put(
        &self,
        _: &str,
        _: &str,
        _: &std::path::Path,
        _: &str,
        _: Option<&str>,
        _: &str,
    ) -> Result<String> {
        unreachable!()
    }
    async fn read(&self, _: &str, _: Option<&str>, _: Option<&str>) -> Result<Vec<u8>> {
        Ok(vec![])
    }
    async fn publish(&self, reference: &str, _: &str, _: Option<&str>, _: &str) -> Result<String> {
        *self
            .calls
            .lock()
            .unwrap()
            .entry(reference.into())
            .or_default() += 1;
        if reference == "release-two" && self.fail_second.load(Ordering::SeqCst) {
            return Err(Error::unavailable(
                "Archive sharing temporarily unavailable",
            ));
        }
        Ok(format!("public:{reference}"))
    }
}
async fn approve_publication(s: &State, id: &str) {
    for (provider, actor) in [
        ("other%3Eprovider", "outsider"),
        ("iam", "iam-reviewer"),
        ("honeycomb", "validator"),
    ] {
        let (status, response) = call(
            s,
            "POST",
            &format!("/api/v1/review-requests/{id}/{provider}/decisions"),
            Some(actor),
            json!({"decision":"approve"}),
            &format!("activate-approval-{provider}-0001"),
            Some(1),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{response}");
    }
}
#[tokio::test]
async fn publication_waits_for_iam_and_all_archives_then_retries_only_incomplete_sharing() {
    let (mut s, _) = setup(true).await;
    let manager = Arc::new(PublicationManager::default());
    let storage = Arc::new(PublicationStorage::default());
    storage.fail_second.store(true, Ordering::SeqCst);
    s.management = manager.clone();
    s.storage = storage.clone();
    s.identity = Arc::new(ReviewIdentity);
    let id = review_application(&s).await;
    let path = format!("/api/v1/review-requests/{id}/activate");
    assert_eq!(
        call(
            &s,
            "POST",
            &path,
            Some("admin"),
            json!({}),
            "activate-too-early-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    approve_publication(&s, &id).await;
    sqlx::query("INSERT INTO releases(plane,app_id,version,sha256,size,storage_ref,created_at) VALUES('production','tos>review-app','2.0.0','fixture-only',1,'release-two',1)").execute(&s.db).await.unwrap();
    assert_eq!(
        call(
            &s,
            "POST",
            &path,
            Some("outsider"),
            json!({}),
            "activate-outsider-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, pending) = call(
        &s,
        "POST",
        &path,
        Some("admin"),
        json!({}),
        "activation-operation-0001",
        Some(1),
    )
    .await;
    assert_eq!(pending["state"], "pending");
    assert_eq!(pending["archives"][0]["state"], "ready");
    assert_eq!(
        call(
            &s,
            "GET",
            "/api/v1/apps/tos%3Ereview-app",
            None,
            Value::Null,
            "",
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &s,
            "PUT",
            "/api/v1/apps/tos%3Ereview-app",
            Some("admin"),
            input("review-app"),
            "activation-edit-block-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/apps/tos%3Ereview-app/secret-rotations",
            Some("admin"),
            json!({}),
            "activation-rotate-block-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    storage.fail_second.store(false, Ordering::SeqCst);
    let (_, done) = call(
        &s,
        "POST",
        &path,
        Some("admin"),
        json!({}),
        "activation-operation-0001",
        Some(1),
    )
    .await;
    assert_eq!(done["state"], "accepted", "{done}");
    assert_eq!(manager.calls.load(Ordering::SeqCst), 1);
    assert_eq!(storage.calls.lock().unwrap()["fixture-release"], 1);
    let (status, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Ereview-app",
        None,
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app["visibility"], "public");
    let (_, again) = call(
        &s,
        "POST",
        &path,
        Some("admin"),
        json!({}),
        "activation-operation-0001",
        Some(1),
    )
    .await;
    assert_eq!(again["id"], done["id"]);
    assert_eq!(manager.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn publication_rejects_wrong_receipts_and_rechecks_current_iam_visibility() {
    let (mut s, _) = setup(true).await;
    let manager = Arc::new(PublicationManager::default());
    let storage = Arc::new(PublicationStorage::default());
    s.management = manager.clone();
    s.storage = storage.clone();
    s.identity = Arc::new(ReviewIdentity);
    let id = review_application(&s).await;
    approve_publication(&s, &id).await;
    let path = format!("/api/v1/review-requests/{id}/activate");
    manager.wrong_receipt.store(true, Ordering::SeqCst);
    let (_, pending) = call(
        &s,
        "POST",
        &path,
        Some("admin"),
        json!({}),
        "activation-receipt-0001",
        Some(1),
    )
    .await;
    assert_eq!(pending["state"], "pending");
    assert!(storage.calls.lock().unwrap().is_empty());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM operations WHERE kind='logo.upload'")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        0
    );
    manager.wrong_receipt.store(false, Ordering::SeqCst);
    manager.revoked.store(true, Ordering::SeqCst);
    let (_, pending) = call(
        &s,
        "POST",
        &path,
        Some("admin"),
        json!({}),
        "activation-receipt-0001",
        Some(1),
    )
    .await;
    assert_eq!(pending["state"], "pending");
    assert_eq!(
        call(
            &s,
            "GET",
            "/api/v1/apps/tos%3Ereview-app",
            None,
            Value::Null,
            "",
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    manager.revoked.store(false, Ordering::SeqCst);
    let (_, done) = call(
        &s,
        "POST",
        &path,
        Some("admin"),
        json!({}),
        "activation-receipt-0001",
        Some(1),
    )
    .await;
    assert_eq!(done["state"], "accepted");
    assert_eq!(storage.calls.lock().unwrap()["fixture-release"], 1);
}

struct SnapshotManager(std::sync::Mutex<Value>);
#[async_trait]
impl Management for SnapshotManager {
    async fn configure(&self, o: &Value, t: &str, e: Option<&str>) -> Result<Value> {
        Manager { accept: true }.configure(o, t, e).await
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        unreachable!()
    }
    async fn application_snapshot(&self, _: &str, _: Option<&str>) -> Result<Value> {
        Ok(self.0.lock().unwrap().clone())
    }
}
async fn signed_event(s: &State, event: &Value, valid: bool) -> (StatusCode, Value) {
    use hmac::{Hmac, Mac};
    let body = event.to_string();
    let timestamp = silicon_honeycomb_server::now().to_string();
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(s.webhook_secret.as_bytes()).unwrap();
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body.as_bytes());
    let signature = if valid {
        hex::encode(mac.finalize().into_bytes())
    } else {
        "00".repeat(32)
    };
    let request = Request::builder()
        .method("POST")
        .uri("/webhook/")
        .header("content-type", "application/json")
        .header(
            "x-silicon-iam-event-id",
            event["event_id"]
                .as_str()
                .or_else(|| event["test"]["metadata"]["event_id"].as_str())
                .unwrap(),
        )
        .header("x-silicon-iam-timestamp", timestamp)
        .header("x-silicon-iam-key-version", "1")
        .header("x-silicon-iam-signature", format!("v1={signature}"))
        .body(Body::from(body))
        .unwrap();
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap(),
    )
}
fn management_event(revision: i64) -> Value {
    json!({"spec_version":"1.0","event_id":uuid::Uuid::new_v4(),"event_type":"honeycomb.application.changed.v1","occurred_at":"2026-09-16T00:00:00Z","organization_id":null,"aggregate":{"type":"application","id":"00000000-0000-0000-0000-000000000001","version":revision},"data":{"app_id":"tos>reconcile"}})
}
#[tokio::test]
async fn periodic_reconciliation_recovers_missed_notifications_and_preserves_newer_targets() {
    use silicon_honeycomb_server::reconciliation;
    let (mut s, _) = setup(true).await;
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/apps",
            Some("admin"),
            input("reconcile"),
            "periodic-create-0001",
            None
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    let config: Value = serde_json::from_str(
        &sqlx::query_scalar::<_, String>(
            "SELECT effective_config FROM applications WHERE app_id='tos>reconcile'",
        )
        .fetch_one(&s.db)
        .await
        .unwrap(),
    )
    .unwrap();
    let manager = Arc::new(SnapshotManager(std::sync::Mutex::new(
        json!({"app_id":"tos>reconcile","iam_revision":3,"configuration_revision":1,"visibility":"private","availability":"disabled","effective_configuration":config}),
    )));
    s.management = manager.clone();
    sqlx::query("UPDATE applications SET visibility='public' WHERE app_id='tos>reconcile'")
        .execute(&s.db)
        .await
        .unwrap();
    reconciliation::tick(&s).await.unwrap();
    let state: (i64, String, String) = sqlx::query_as(
        "SELECT iam_revision,visibility,state FROM applications WHERE app_id='tos>reconcile'",
    )
    .fetch_one(&s.db)
    .await
    .unwrap();
    assert_eq!(state, (3, "private".into(), "disabled".into()));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM webhook_events")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        0
    );

    // Another tick does not poll again until due, including after worker restart.
    manager.0.lock().unwrap()["iam_revision"] = json!(4);
    reconciliation::tick(&s).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT iam_revision FROM applications WHERE app_id='tos>reconcile'"
        )
        .fetch_one(&s.db)
        .await
        .unwrap(),
        3
    );

    // A newer signed notification keeps its target and backoff during the scan.
    reconciliation::receive(&s, "production", &management_event(8))
        .await
        .unwrap();
    let retry = silicon_honeycomb_server::now() + 600;
    sqlx::query("UPDATE application_reconciliation SET retry_at=?")
        .bind(retry)
        .execute(&s.db)
        .await
        .unwrap();
    sqlx::query("UPDATE applications SET iam_check_after=0")
        .execute(&s.db)
        .await
        .unwrap();
    reconciliation::tick(&s).await.unwrap();
    let pending: (i64, i64, String) =
        sqlx::query_as("SELECT target_revision,retry_at,state FROM application_reconciliation")
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(pending, (8, retry, "pending".into()));
}
#[tokio::test]
async fn signed_management_events_reconcile_revocation_without_stale_or_unpublished_grants() {
    use silicon_honeycomb_server::reconciliation;
    let (mut s, _) = setup(true).await;
    s.webhook_secret = "test-webhook-signing-secret-000000000000000".into();
    call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("reconcile"),
        "reconcile-create-0001",
        None,
    )
    .await;
    sqlx::query("UPDATE applications SET visibility='public' WHERE app_id='tos>reconcile'")
        .execute(&s.db)
        .await
        .unwrap();
    let config: Value = serde_json::from_str(
        &sqlx::query_scalar::<_, String>(
            "SELECT effective_config FROM applications WHERE app_id='tos>reconcile'",
        )
        .fetch_one(&s.db)
        .await
        .unwrap(),
    )
    .unwrap();
    let snapshot = json!({"app_id":"tos>reconcile","iam_revision":3,"configuration_revision":1,"visibility":"private","availability":"disabled","effective_configuration":config});
    let manager = Arc::new(SnapshotManager(std::sync::Mutex::new(snapshot)));
    s.management = manager.clone();
    let event = management_event(3);
    assert_eq!(
        signed_event(&s, &event, false).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &s,
            "GET",
            "/api/v1/apps/tos%3Ereconcile",
            None,
            Value::Null,
            "",
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, receipt) = signed_event(&s, &event, true).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(signed_event(&s, &event, true).await.1["duplicate"], true);
    assert_eq!(
        call(
            &s,
            "GET",
            "/api/v1/apps/tos%3Ereconcile",
            None,
            Value::Null,
            "",
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    reconciliation::reconcile(&s, "production", "tos>reconcile")
        .await
        .unwrap();
    assert_eq!(
        call(
            &s,
            "GET",
            "/api/v1/apps/tos%3Ereconcile",
            Some("member"),
            Value::Null,
            "",
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Ereconcile",
        Some("admin"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(app["state"], "disabled");
    assert_eq!(app["iam_revision"], 3);
    signed_event(&s, &management_event(2), true).await;
    let queued: String = sqlx::query_scalar("SELECT state FROM application_reconciliation")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(queued, "accepted");
    signed_event(&s, &management_event(5), true).await;
    assert!(
        reconciliation::reconcile(&s, "production", "tos>reconcile")
            .await
            .is_err()
    );
    {
        let mut snapshot = manager.0.lock().unwrap();
        snapshot["iam_revision"] = json!(5);
        snapshot["visibility"] = json!("public");
        snapshot["availability"] = json!("active");
        snapshot["effective_configuration"]["app_secret"] = json!("never-save-this");
    }
    reconciliation::reconcile(&s, "production", "tos>reconcile")
        .await
        .unwrap();
    let (_, app) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Ereconcile",
        Some("admin"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(
        app["visibility"], "private",
        "IAM cannot bypass Honeycomb's archive/publication gate"
    );
    assert_eq!(app["iam_revision"], 5);
    assert!(!app.to_string().contains("never-save-this"));
    // A genuinely completed local publication may be restored by a newer accepted state.
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,state,created_at) VALUES('published-operation','production','admin','published-operation-key','publication.activate','tos>reconcile','test',1,'accepted',1)").execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO publication_requests(id,plane,app_id,revision,state,requested_by,created_at,activation_operation) VALUES('published-request','production','tos>reconcile',1,'published','admin',1,'published-operation')").execute(&s.db).await.unwrap();
    {
        let mut snapshot = manager.0.lock().unwrap();
        snapshot["iam_revision"] = json!(6);
        snapshot["publication_request_id"] = json!("published-request");
    }
    signed_event(&s, &management_event(6), true).await;
    reconciliation::reconcile(&s, "production", "tos>reconcile")
        .await
        .unwrap();
    assert_eq!(
        call(
            &s,
            "GET",
            "/api/v1/apps/tos%3Ereconcile",
            None,
            Value::Null,
            "",
            None
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn management_notifications_remain_in_their_signed_testing_generation() {
    let (mut s, _) = setup(true).await;
    s.webhook_secret = "test-webhook-signing-secret-000000000000000".into();
    call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("reconcile"),
        "test-event-create-0001",
        None,
    )
    .await;
    let root = "Abcdefgh123456789012345678901234";
    sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,state,generation,created_at,last_activity) VALUES('test-events','tos','admin','Events','',?,'unused-test-event-hash','ready',2,1,1)")
        .bind(s.encrypt(root).unwrap()).execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,config,effective_config,webhook_secret,revision,iam_revision,effective_revision,visibility,state,created_at,updated_at) SELECT 'test-events',app_id,org_id,name,description,config,effective_config,webhook_secret,revision,iam_revision,effective_revision,'public',state,created_at,updated_at FROM applications WHERE plane='production' AND app_id='tos>reconcile'").execute(&s.db).await.unwrap();
    let mut metadata = management_event(2);
    metadata.as_object_mut().unwrap().remove("data");
    metadata["aggregate"]["environment_id"] = json!("test-events");
    metadata["aggregate"]["generation"] = json!(1);
    let mut event =
        json!({"test":{"testing_key":root,"metadata":metadata,"data":{"app_id":"tos>reconcile"}}});
    assert_eq!(signed_event(&s, &event, true).await.0, StatusCode::CONFLICT);
    event["test"]["metadata"]["aggregate"]["generation"] = json!(2);
    let (status, body) = signed_event(&s, &event, true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let plane: String = sqlx::query_scalar("SELECT plane FROM application_reconciliation")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(plane, "test-events");
    let production: i64 = sqlx::query_scalar(
        "SELECT iam_revision FROM applications WHERE plane='production' AND app_id='tos>reconcile'",
    )
    .fetch_one(&s.db)
    .await
    .unwrap();
    assert_eq!(production, 1);
    let resource: String = sqlx::query_scalar("SELECT resource FROM webhook_events")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(resource, "tos>reconcile");
    assert!(!resource.contains(root));
    event["test"]["testing_key"] = json!("Bbcdefgh123456789012345678901234");
    assert_eq!(
        signed_event(&s, &event, true).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn iam_management_notifications_use_independent_signatures_and_durable_deduplication() {
    use hmac::{Hmac, Mac};
    let (s, _) = setup(true).await;
    call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("reconcile"),
        "management-signature-create",
        None,
    )
    .await;
    let body=json!({"event_id":uuid::Uuid::new_v4(),"operation_id":uuid::Uuid::new_v4(),"resource_id":"tos>reconcile","environment_id":null,"revision":2,"event_type":"application.scope.decided","data":{"app_secret":"must-never-persist"}}).to_string();
    let timestamp = silicon_honeycomb_server::now();
    let key = "independent-management-signing-key";
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes()).unwrap();
    mac.update(format!("{timestamp}.").as_bytes());
    mac.update(body.as_bytes());
    let signature = format!(
        "t={timestamp},v1={}",
        hex::encode(mac.finalize().into_bytes())
    );
    let router =
        silicon_honeycomb_server::iam_management::notification_router(s.clone(), key.into());
    for duplicate in [false, true] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/management/webhook/")
                    .header("x-iam-management-signature", &signature)
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let receipt: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(receipt["duplicate"], duplicate);
    }
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/management/webhook/")
                .header("x-iam-management-signature", signature)
                .body(Body::from(body + " "))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let resource: String = sqlx::query_scalar("SELECT resource FROM webhook_events")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(resource, "tos>reconcile");
    let queued: i64 = sqlx::query_scalar("SELECT target_revision FROM application_reconciliation")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(queued, 2);
}

struct CatalogManager;
#[async_trait]
impl Management for CatalogManager {
    async fn configure(&self, _: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        unreachable!()
    }
    async fn scope_catalog(&self, org: &str, provider: Option<&str>) -> Result<Value> {
        assert_eq!(org, "tos");
        Ok(
            json!({"items":[{"scope":provider.map(|p|format!("obo:{p}:files.read")).unwrap_or_else(||"self.profile.read".into()),"description":"Read permitted data","critical":provider.is_some(),"eligible":true},{"scope":"obo:hidden>provider:secret","description":"Must be filtered","critical":true,"eligible":true}]}),
        )
    }
}
#[tokio::test]
async fn permission_discovery_requires_current_admin_and_does_not_reveal_private_providers() {
    let (mut s, _) = setup(true).await;
    import_source(&s, "other>private-provider", &[], false).await;
    sqlx::query("UPDATE applications SET effective_config=json_set(effective_config,'$.obo_endpoints',json('[{\"endpoint_id\":\"files.read\",\"path\":\"/files\",\"critical\":true}]')) WHERE app_id='other>private-provider'").execute(&s.db).await.unwrap();
    s.management = Arc::new(CatalogManager);
    let path = "/api/v1/organizations/tos/scope-catalog";
    for actor in [Some("member"), Some("outsider"), None] {
        assert!(
            call(&s, "GET", path, actor, Value::Null, "", None)
                .await
                .0
                .is_client_error()
        );
    }
    let (status, catalog) = call(&s, "GET", path, Some("admin"), Value::Null, "", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(catalog["items"].as_array().unwrap().len(), 1);
    assert_eq!(catalog["providers"], json!([]));
    let provider = format!("{path}?provider=other%3Eprivate-provider");
    assert_eq!(
        call(&s, "GET", &provider, Some("admin"), Value::Null, "", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    sqlx::query(
        "UPDATE applications SET visibility='public' WHERE app_id='other>private-provider'",
    )
    .execute(&s.db)
    .await
    .unwrap();
    let (status, catalog) = call(&s, "GET", &provider, Some("admin"), Value::Null, "", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(catalog["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        catalog["items"][0]["scope"],
        "obo:other>private-provider:files.read"
    );
    assert_eq!(catalog["providers"][0]["app_id"], "other>private-provider");
}

struct WebhookManager {
    records: std::sync::Mutex<BTreeMap<String, Value>>,
    lose_response: AtomicBool,
}
#[async_trait]
impl Management for WebhookManager {
    async fn configure(&self, o: &Value, t: &str, e: Option<&str>) -> Result<Value> {
        let response = Manager { accept: true }.configure(o, t, e).await?;
        self.records.lock().unwrap().insert(o["app_id"].as_str().unwrap().into(), json!({"app_id":o["app_id"],"configuration_revision":o["configuration_revision"],"iam_revision":response["iam_revision"],"pending_endpoint_id":"11111111-1111-4111-8111-111111111111"}));
        Ok(response)
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::unavailable("pending"))
    }
    async fn webhook_state(&self, app: &str, _: Option<&str>) -> Result<Value> {
        self.records
            .lock()
            .unwrap()
            .get(app)
            .cloned()
            .ok_or_else(Error::missing)
    }
    async fn webhook_mutation(
        &self,
        o: &Value,
        _: &str,
        proof: Option<&str>,
        _: Option<&str>,
    ) -> Result<Value> {
        let mut records = self.records.lock().unwrap();
        let id = o["operation_id"].as_str().unwrap();
        if let Some(receipt) = records.get(id) {
            return Ok(receipt.clone());
        }
        if proof != Some("verified-channel-proof") {
            return Err(Error::new(
                StatusCode::FORBIDDEN,
                "step_up_required",
                "Verify identity",
            ));
        }
        let mut receipt = json!({"operation_id":id,"app_id":o["app_id"],"state":"accepted","iam_revision":o["expected_iam_revision"].as_i64().unwrap()+1});
        if o["kind"] == "webhook.approve" {
            receipt["webhook_endpoint_id"] = o["pending_endpoint_id"].clone();
        } else {
            receipt["webhook_secret_version"] = json!(2);
        }
        let state = records.get_mut(o["app_id"].as_str().unwrap()).unwrap();
        state["iam_revision"] = receipt["iam_revision"].clone();
        state["pending_endpoint_id"] = Value::Null;
        records.insert(id.into(), receipt.clone());
        if self.lose_response.swap(false, Ordering::SeqCst) {
            return Err(Error::unavailable("Lost response"));
        }
        Ok(receipt)
    }
}
#[tokio::test]
async fn webhook_changes_require_current_authority_bind_endpoint_and_retry_without_plaintext() {
    let (mut s, _) = setup(true).await;
    let manager = Arc::new(WebhookManager {
        records: Default::default(),
        lose_response: AtomicBool::new(false),
    });
    s.management = manager.clone();
    call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("webhooks"),
        "webhook-create-0001",
        None,
    )
    .await;
    let path = "/api/v1/apps/tos%3Ewebhooks/webhook";
    for (token, status) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some("member"), StatusCode::FORBIDDEN),
        (Some("outsider"), StatusCode::FORBIDDEN),
    ] {
        assert_eq!(
            call(&s, "GET", path, token, json!({}), "webhook-read-0001", None)
                .await
                .0,
            status
        );
    }
    let approval =
        json!({"action":"approve","pending_endpoint_id":"11111111-1111-4111-8111-111111111111"});
    assert_eq!(
        call(
            &s,
            "POST",
            path,
            Some("member"),
            approval.clone(),
            "webhook-member-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &s,
            "POST",
            path,
            Some("admin"),
            approval.clone(),
            "webhook-stale-0001",
            Some(0)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &s,
            "POST",
            path,
            Some("admin"),
            json!({"action":"approve","pending_endpoint_id":uuid::Uuid::new_v4()}),
            "webhook-wrong-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, pending) = call(
        &s,
        "POST",
        path,
        Some("admin"),
        approval.clone(),
        "webhook-approve-0001",
        Some(1),
    )
    .await;
    assert_eq!(pending["error_code"], "step_up_required");
    assert_eq!(
        call(
            &s,
            "PUT",
            "/api/v1/apps/tos%3Ewebhooks",
            Some("admin"),
            input("webhooks"),
            "webhook-edit-blocked-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/apps/tos%3Ewebhooks/secret-rotations",
            Some("admin"),
            json!({}),
            "webhook-secret-blocked-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let retry = format!(
        "/api/v1/operations/{}/webhook-retry",
        pending["id"].as_str().unwrap()
    );
    assert_eq!(
        call(
            &s,
            "POST",
            &retry,
            Some("member"),
            json!({}),
            "webhook-retry-member-0001",
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, accepted) = call(
        &s,
        "POST",
        &retry,
        Some("admin"),
        json!({"step_up_assertion":"verified-channel-proof"}),
        "webhook-retry-0001",
        None,
    )
    .await;
    assert_eq!(accepted["state"], "accepted");
    let (_, replay) = call(
        &s,
        "POST",
        path,
        Some("admin"),
        approval,
        "webhook-approve-0001",
        Some(1),
    )
    .await;
    assert_eq!(replay, accepted);
    let secret = "new-webhook-signing-secret-never-plaintext";
    manager.lose_response.store(true, Ordering::SeqCst);
    let (_, lost) = call(&s,"POST",path,Some("admin"),json!({"action":"rotate","webhook_secret":secret,"step_up_assertion":"verified-channel-proof"}),"webhook-rotate-0001",Some(1)).await;
    assert_eq!(lost["state"], "pending");
    assert_eq!(
        call(
            &s,
            "POST",
            path,
            Some("admin"),
            json!({"action":"rotate","webhook_secret":"different-secret-that-is-long-enough"}),
            "webhook-rotate-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, result) = call(
        &s,
        "POST",
        &format!(
            "/api/v1/operations/{}/webhook-retry",
            lost["id"].as_str().unwrap()
        ),
        Some("admin"),
        json!({}),
        "webhook-recover-0001",
        None,
    )
    .await;
    assert_eq!(result["state"], "accepted");
    assert_eq!(result["webhook_secret_version"], 2);
    let encrypted: String =
        sqlx::query_scalar("SELECT webhook_secret FROM applications WHERE app_id='tos>webhooks'")
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(s.decrypt(&encrypted).unwrap(), secret);
    let requests: Vec<String> =
        sqlx::query_scalar("SELECT request_json FROM operations WHERE kind LIKE 'webhook.%'")
            .fetch_all(&s.db)
            .await
            .unwrap();
    assert!(
        requests
            .iter()
            .all(|r| !r.contains(secret) && !r.contains("verified-channel-proof"))
    );
    let (_, operations) = call(
        &s,
        "GET",
        "/api/v1/apps/tos%3Ewebhooks/operations",
        Some("admin"),
        json!({}),
        "webhook-list-0001",
        None,
    )
    .await;
    assert!(!operations.to_string().contains(secret));
    assert!(!operations.to_string().contains("encrypted_webhook_secret"));
}

async fn retention_fixture(s: &State) -> i64 {
    use sha2::{Digest, Sha256};
    let at = silicon_honeycomb_server::now();
    let root = "RetentionRootKey00000000000000000";
    sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,state,created_at,last_activity) VALUES('retention-env','tos','admin','Retention','',?,?,'ready',?,?)")
        .bind(s.encrypt(root).unwrap()).bind(hex::encode(Sha256::digest(root))).bind(at-40*86400).bind(at-40*86400).execute(&s.db).await.unwrap();
    for (app, days, dependencies) in [
        ("a", 1, vec!["tos>b"]),
        ("b", 1, vec!["tos>c"]),
        ("c", 1, vec!["tos>b"]),
        ("d", 1, vec![]),
        ("long", 1, vec![]),
    ] {
        let config=json!({"testing_idle_days":days,"app_scope":{"external":dependencies.into_iter().map(|d|json!({"app_id":d,"endpoint_id":"use"})).collect::<Vec<_>>()}}).to_string();
        let id = format!("tos>{app}");
        sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,state,iam_revision,effective_revision,config,effective_config,webhook_secret,created_at,updated_at) VALUES('retention-env',?,'tos',?,'','active',1,1,?,?,'',?,?)")
            .bind(&id).bind(app).bind(&config).bind(&config).bind(at-40*86400).bind(at).execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO environment_app_activity(environment_id,app_id,last_activity) VALUES('retention-env',?,?)").bind(&id).bind(if app=="a" {at} else {at-40*86400}).execute(&s.db).await.unwrap();
    }
    sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,state,iam_revision,effective_revision,config,effective_config,webhook_secret,created_at,updated_at) VALUES('production','tos>long','tos','Long','','active',1,1,'{}','{\"testing_idle_days\":90}','',?,?)").bind(at).bind(at).execute(&s.db).await.unwrap();
    at
}
#[tokio::test]
async fn retention_protects_transitive_dependencies_cycles_and_longer_production_policies() {
    let (s, _) = setup(true).await;
    let at = retention_fixture(&s).await;
    let mut connection = s.db.acquire().await.unwrap();
    let plan = silicon_honeycomb_server::retention::plan(&mut connection, "retention-env", at)
        .await
        .unwrap();
    let apps = plan["applications"].as_array().unwrap();
    for id in ["tos>b", "tos>c"] {
        let app = apps.iter().find(|a| a["app_id"] == id).unwrap();
        assert_eq!(app["required_by_active_app"], true);
        assert_eq!(app["eligible_for_retirement"], false);
    }
    assert_eq!(
        apps.iter().find(|a| a["app_id"] == "tos>d").unwrap()["eligible_for_retirement"],
        true
    );
    assert_eq!(
        apps.iter().find(|a| a["app_id"] == "tos>long").unwrap()["idle_days"],
        90
    );
    assert_eq!(plan["delete_after"], at + 50 * 86400);
    assert_eq!(plan["eligible_for_deletion"], false);
    let later = silicon_honeycomb_server::retention::plan(
        &mut connection,
        "retention-env",
        at + 91 * 86400,
    )
    .await
    .unwrap();
    assert_eq!(later["eligible_for_deletion"], true);
    assert!(
        later["applications"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["eligible_for_retirement"] == true)
    );
}
#[tokio::test]
async fn activity_is_generation_bound_server_timed_and_replay_does_not_keep_environment_alive() {
    let (s, _) = setup(true).await;
    let at = retention_fixture(&s).await;
    let path = "/api/v1/environments/retention-env/apps/tos%3Ed/activity";
    let payload = json!({"generation":1,"key_version":1});
    assert_eq!(
        call(
            &s,
            "POST",
            path,
            Some("outsider"),
            payload.clone(),
            "activity-outsider-0001",
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, first) = call(
        &s,
        "POST",
        path,
        Some("admin"),
        payload.clone(),
        "activity-report-0001",
        None,
    )
    .await;
    assert!(first["last_activity"].as_i64().unwrap() >= at);
    assert_eq!(first["replayed"], false);
    sqlx::query("UPDATE environments SET last_activity=1 WHERE id='retention-env'")
        .execute(&s.db)
        .await
        .unwrap();
    let (_, replayed) = call(
        &s,
        "POST",
        path,
        Some("admin"),
        payload.clone(),
        "activity-report-0001",
        None,
    )
    .await;
    assert_eq!(replayed["last_activity"], first["last_activity"]);
    assert_eq!(replayed["replayed"], true);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT last_activity FROM environments WHERE id='retention-env'"
        )
        .fetch_one(&s.db)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/environments/retention-env/apps/tos%3Eb/activity",
            Some("admin"),
            payload,
            "activity-report-0001",
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    // Test apps can report with root authority; no user session is required.
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("idempotency-key", "activity-root-report-0001")
        .header(
            "x-testing-environment-key",
            "RetentionRootKey00000000000000000",
        )
        .body(Body::from(
            json!({"generation":1,"key_version":1}).to_string(),
        ))
        .unwrap();
    assert_eq!(
        silicon_honeycomb_server::api::router(s.clone())
            .oneshot(request)
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    sqlx::query("UPDATE environments SET generation=2 WHERE id='retention-env'")
        .execute(&s.db)
        .await
        .unwrap();
    assert_eq!(
        call(
            &s,
            "POST",
            path,
            Some("admin"),
            json!({"generation":1,"key_version":1}),
            "activity-old-generation-0001",
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &s,
            "POST",
            path,
            Some("admin"),
            json!({"generation":2,"key_version":1,"timestamp":9999999999_i64}),
            "activity-future-time-0001",
            None
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
#[tokio::test]
async fn retention_changes_are_authorized_revisioned_and_idempotent() {
    let (s, _) = setup(true).await;
    retention_fixture(&s).await;
    let path = "/api/v1/environments/retention-env/retention";
    assert_eq!(
        call(
            &s,
            "PUT",
            path,
            Some("member"),
            json!({"idle_days":90}),
            "retention-member-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &s,
            "PUT",
            path,
            Some("admin"),
            json!({"idle_days":0}),
            "retention-invalid-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &s,
            "PUT",
            path,
            Some("admin"),
            json!({"idle_days":90}),
            "retention-stale-0001",
            Some(2)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (status, result) = call(
        &s,
        "PUT",
        path,
        Some("admin"),
        json!({"idle_days":90}),
        "retention-update-0001",
        Some(1),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["revision"], 2);
    let (_, replay) = call(
        &s,
        "PUT",
        path,
        Some("admin"),
        json!({"idle_days":90}),
        "retention-update-0001",
        Some(1),
    )
    .await;
    assert_eq!(replay, result);
    assert_eq!(
        call(
            &s,
            "PUT",
            path,
            Some("admin"),
            json!({"idle_days":91}),
            "retention-update-0001",
            Some(1)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[derive(Default)]
struct RetentionServices {
    calls: std::sync::Mutex<Vec<(String, String)>>,
    fail_iam: AtomicBool,
    fail_storage: AtomicBool,
    bad_targets: AtomicBool,
}
#[async_trait]
impl Management for RetentionServices {
    async fn configure(&self, o: &Value, t: &str, e: Option<&str>) -> Result<Value> {
        Manager { accept: true }.configure(o, t, e).await
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::unavailable(
            "User path must not handle automatic retention",
        ))
    }
    async fn retention_lifecycle(&self, app: &str, o: &Value) -> Result<Value> {
        self.calls
            .lock()
            .unwrap()
            .push((app.into(), o["action"].as_str().unwrap().into()));
        if (app == "tos>iam" && self.fail_iam.load(Ordering::SeqCst))
            || (app == "tos>briefcase" && self.fail_storage.load(Ordering::SeqCst))
        {
            return Err(Error::unavailable("Service temporarily unavailable"));
        }
        Ok(
            json!({"state":"completed","operation_id":o["operation_id"],"environment_id":o["environment_id"],"app_id":app,"environment_revision":o["environment_revision"],"generation":o["generation"],"key_version":o["key_version"],"retired_apps":if self.bad_targets.load(Ordering::SeqCst){json!([])}else{o["retired_apps"].clone()}}),
        )
    }
}
async fn add_storage_participant(s: &State) {
    sqlx::query("INSERT INTO environment_services(environment_id,app_id,source_revision,snapshot,state,operation_id,generation) VALUES('retention-env','tos>briefcase',1,'{}','ready','earlier',1)").execute(&s.db).await.unwrap();
}
#[tokio::test]
async fn scheduled_retirement_requires_exact_receipts_and_preserves_active_apps_and_production() {
    let (mut s, _) = setup(true).await;
    let at = retention_fixture(&s).await;
    add_storage_participant(&s).await;
    let services = Arc::new(RetentionServices::default());
    services.fail_iam.store(true, Ordering::SeqCst);
    s.management = services.clone();
    let op = silicon_honeycomb_server::retention_worker::schedule(&s, "retention-env", at)
        .await
        .unwrap()
        .unwrap();
    assert!(
        silicon_honeycomb_server::retention_worker::schedule(&s, "retention-env", at)
            .await
            .unwrap()
            .is_none()
    );
    let targets: Vec<String> =
        sqlx::query_scalar("SELECT app_id FROM retirement_targets WHERE operation_id=?")
            .bind(&op)
            .fetch_all(&s.db)
            .await
            .unwrap();
    assert_eq!(targets, vec!["tos>d"]);
    silicon_honeycomb_server::lifecycle::coordinate(&s, "retention-env", &op, "")
        .await
        .unwrap();
    assert_eq!(services.calls.lock().unwrap().len(), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM applications WHERE plane='retention-env'"
        )
        .fetch_one(&s.db)
        .await
        .unwrap(),
        5
    );
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/environments/retention-env/apps/tos%3Ed/activity",
            Some("admin"),
            json!({"generation":1,"key_version":1}),
            "retiring-activity-0001",
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/environments/retention-env/apps/tos%3Ea/activity",
            Some("admin"),
            json!({"generation":1,"key_version":1}),
            "active-survivor-0001",
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    services.fail_iam.store(false, Ordering::SeqCst);
    services.bad_targets.store(true, Ordering::SeqCst);
    let rejected = silicon_honeycomb_server::lifecycle::coordinate(&s, "retention-env", &op, "")
        .await
        .unwrap();
    assert_eq!(rejected["operation_state"], "pending");
    services.bad_targets.store(false, Ordering::SeqCst);
    services.fail_storage.store(true, Ordering::SeqCst);
    let pending = silicon_honeycomb_server::lifecycle::coordinate(&s, "retention-env", &op, "")
        .await
        .unwrap();
    assert_eq!(pending["operation_state"], "pending");
    let iam_calls = services
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(a, _)| a == "tos>iam")
        .count();
    services.fail_storage.store(false, Ordering::SeqCst);
    let completed = silicon_honeycomb_server::lifecycle::coordinate(&s, "retention-env", &op, "")
        .await
        .unwrap();
    assert_eq!(completed["operation_state"], "accepted");
    assert_eq!(
        services
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(a, _)| a == "tos>iam")
            .count(),
        iam_calls
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM applications WHERE plane='retention-env'"
        )
        .fetch_one(&s.db)
        .await
        .unwrap(),
        4
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM applications WHERE plane='production' AND app_id='tos>long'"
        )
        .fetch_one(&s.db)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM environments WHERE id='retention-env'")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        "ready"
    );
}
#[tokio::test]
async fn automatic_delete_waits_for_disable_and_purge_waits_for_recovery_deadline() {
    let (mut s, _) = setup(true).await;
    let at = retention_fixture(&s).await;
    add_storage_participant(&s).await;
    let services = Arc::new(RetentionServices::default());
    services.fail_storage.store(true, Ordering::SeqCst);
    s.management = services.clone();
    let op =
        silicon_honeycomb_server::retention_worker::schedule(&s, "retention-env", at + 100 * 86400)
            .await
            .unwrap()
            .unwrap();
    let pending = silicon_honeycomb_server::lifecycle::coordinate(&s, "retention-env", &op, "")
        .await
        .unwrap();
    assert_eq!(pending["state"], "deleting");
    assert!(
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT purge_after FROM environments WHERE id='retention-env'"
        )
        .fetch_one(&s.db)
        .await
        .unwrap()
        .is_none()
    );
    services.fail_storage.store(false, Ordering::SeqCst);
    let result = silicon_honeycomb_server::lifecycle::coordinate(&s, "retention-env", &op, "")
        .await
        .unwrap();
    assert_eq!(result["state"], "deleted");
    let deadline: i64 =
        sqlx::query_scalar("SELECT purge_after FROM environments WHERE id='retention-env'")
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert!(deadline >= at + 30 * 86400);
    assert!(
        silicon_honeycomb_server::retention_worker::schedule(&s, "retention-env", deadline - 1)
            .await
            .unwrap()
            .is_none()
    );
    let purge = silicon_honeycomb_server::retention_worker::schedule(&s, "retention-env", deadline)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        silicon_honeycomb_server::lifecycle::coordinate(&s, "retention-env", &purge, "")
            .await
            .unwrap()["state"],
        "purged"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT encrypted_key FROM environments WHERE id='retention-env'"
        )
        .fetch_one(&s.db)
        .await
        .unwrap(),
        ""
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM environment_app_activity WHERE environment_id='retention-env'"
        )
        .fetch_one(&s.db)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM applications WHERE plane='production'")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
async fn retention_worker_retries_with_backoff_without_rescheduling_or_fabricating_actor_tokens() {
    let (mut s, _) = setup(true).await;
    let at = retention_fixture(&s).await;
    let services = Arc::new(RetentionServices::default());
    services.fail_iam.store(true, Ordering::SeqCst);
    s.management = services.clone();
    let op = silicon_honeycomb_server::retention_worker::schedule(&s, "retention-env", at)
        .await
        .unwrap()
        .unwrap();
    silicon_honeycomb_server::retention_worker::tick(&s)
        .await
        .unwrap();
    assert_eq!(services.calls.lock().unwrap().len(), 1);
    silicon_honeycomb_server::retention_worker::tick(&s)
        .await
        .unwrap();
    assert_eq!(services.calls.lock().unwrap().len(), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM retention_jobs")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        1
    );
    services.fail_iam.store(false, Ordering::SeqCst);
    sqlx::query("UPDATE retention_jobs SET next_attempt_at=0")
        .execute(&s.db)
        .await
        .unwrap();
    silicon_honeycomb_server::retention_worker::tick(&s)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM operations WHERE id=?")
            .bind(op)
            .fetch_one(&s.db)
            .await
            .unwrap(),
        "accepted"
    );
}

type StoredLogo = (String, Vec<u8>, Option<String>);
struct LogoStorage {
    calls: std::sync::Mutex<Vec<StoredLogo>>,
    fail: AtomicBool,
}
#[async_trait]
impl ArchiveStorage for LogoStorage {
    async fn upload_logo(
        &self,
        org: &str,
        path: &std::path::Path,
        actor: &str,
        environment: Option<&str>,
        op: &str,
    ) -> Result<String> {
        assert_eq!(org, "tos");
        assert_eq!(actor, "admin");
        self.calls.lock().unwrap().push((
            op.into(),
            std::fs::read(path).unwrap(),
            environment.map(str::to_owned),
        ));
        if self.fail.swap(false, Ordering::SeqCst) {
            return Err(Error::unavailable("Temporary Briefcase failure"));
        }
        Ok(format!(
            "https://briefcase.example.com/public/logo-{op}.png"
        ))
    }
    async fn put(
        &self,
        _: &str,
        _: &str,
        _: &std::path::Path,
        _: &str,
        _: Option<&str>,
        _: &str,
    ) -> Result<String> {
        unreachable!()
    }
    async fn read(&self, _: &str, _: Option<&str>, _: Option<&str>) -> Result<Vec<u8>> {
        unreachable!()
    }
    async fn publish(&self, _: &str, _: &str, _: Option<&str>, _: &str) -> Result<String> {
        unreachable!()
    }
}
fn logo_bytes() -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(2, 2, image::Rgba([23, 54, 184, 255]))
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}
async fn upload_logo_call(
    s: &State,
    actor: Option<&str>,
    idem: &str,
    bytes: Vec<u8>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/organizations/tos/logos")
        .header("idempotency-key", format!("test-operation-{idem}"));
    if let Some(actor) = actor {
        request = request.header("authorization", format!("Bearer {actor}"));
    }
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(request.body(Body::from(bytes)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
#[tokio::test]
async fn logos_require_current_admin_validate_bytes_and_replay_exact_uploads() {
    let (mut s, revoked) = setup(true).await;
    let storage = Arc::new(LogoStorage {
        calls: Default::default(),
        fail: AtomicBool::new(false),
    });
    s.storage = storage.clone();
    let bytes = logo_bytes();
    for (actor, expected) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some("member"), StatusCode::FORBIDDEN),
        (Some("outsider"), StatusCode::FORBIDDEN),
    ] {
        assert_eq!(
            upload_logo_call(&s, actor, "logo-gate", bytes.clone())
                .await
                .0,
            expected
        );
    }
    assert_eq!(
        upload_logo_call(
            &s,
            Some("admin"),
            "logo-svg",
            b"<svg onload='alert(1)'/>".to_vec()
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        upload_logo_call(
            &s,
            Some("admin"),
            "logo-large",
            vec![0; 2 * 1024 * 1024 + 1]
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert!(storage.calls.lock().unwrap().is_empty());
    let mut payload = bytes.clone();
    payload.extend_from_slice(b"private image metadata");
    let (status, result) =
        upload_logo_call(&s, Some("admin"), "logo-success", payload.clone()).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["state"], "accepted");
    assert_eq!(
        upload_logo_call(&s, Some("admin"), "logo-success", payload.clone())
            .await
            .1,
        result
    );
    assert_eq!(storage.calls.lock().unwrap().len(), 1);
    assert_eq!(storage.calls.lock().unwrap()[0].1, bytes);
    assert_eq!(
        upload_logo_call(&s, Some("admin"), "logo-success", bytes)
            .await
            .0,
        StatusCode::CONFLICT
    );
    revoked.store(true, Ordering::SeqCst);
    assert_eq!(
        upload_logo_call(&s, Some("admin"), "logo-success", payload)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}
#[tokio::test]
async fn logo_retry_preserves_storage_operation_after_failure() {
    let (mut s, _) = setup(true).await;
    let storage = Arc::new(LogoStorage {
        calls: Default::default(),
        fail: AtomicBool::new(true),
    });
    s.storage = storage.clone();
    assert_eq!(
        upload_logo_call(&s, Some("admin"), "logo-retry", logo_bytes())
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let (status, result) = upload_logo_call(&s, Some("admin"), "logo-retry", logo_bytes()).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let calls = storage.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, calls[1].0);
    assert_eq!(result["id"], calls[0].0);
}

#[derive(Default)]
struct Diagnostics(std::sync::Mutex<Vec<Value>>);
impl silicon_honeycomb_server::telemetry::EventSink for Diagnostics {
    fn record(&self, value: Value) {
        self.0.lock().unwrap().push(value);
    }
}
#[tokio::test]
async fn telemetry_opt_out_and_schema_prevent_exporting_user_content() {
    let (mut s, _) = setup(true).await;
    let sink = Arc::new(Diagnostics::default());
    s.telemetry = silicon_honeycomb_server::telemetry::Recorder::with_sink(sink.clone());
    let (_, _) = call(
        &s,
        "GET",
        "/api/v1/apps?q=secret-search-text",
        Some("admin"),
        Value::Null,
        "telemetry-search-0001",
        None,
    )
    .await;
    let events = sink.0.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["context"]["route"], "/api/v1/apps");
    assert!(!events[0].to_string().contains("secret-search-text"));
    sink.0.lock().unwrap().clear();
    let body = json!({"source":"cli","event":"command_completed","action":"apps","duration_ms":12,"success":false});
    let (status, _) = call(
        &s,
        "POST",
        "/api/v1/telemetry",
        Some("admin"),
        body.clone(),
        "telemetry-command-001",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(sink.0.lock().unwrap().len(), 1);
    let mut untrusted = body.clone();
    untrusted["token"] = json!("do-not-export-this-token");
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/telemetry",
            Some("admin"),
            untrusted,
            "telemetry-unknown-001",
            None
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut untrusted = body.clone();
    untrusted["action"] = json!("secret-app-name");
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/telemetry",
            Some("admin"),
            untrusted,
            "telemetry-action-001",
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &s,
            "POST",
            "/api/v1/telemetry",
            None,
            body.clone(),
            "telemetry-anonymous-01",
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    for (path, method, payload) in [
        ("/api/v1/apps", "GET", String::new()),
        ("/api/v1/telemetry", "POST", body.to_string()),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .header("x-honeycomb-telemetry", "false")
            .body(Body::from(payload))
            .unwrap();
        let response = silicon_honeycomb_server::api::router(s.clone())
            .oneshot(request)
            .await
            .unwrap();
        assert!(response.status().is_success());
    }
    assert_eq!(sink.0.lock().unwrap().len(), 1);
    assert!(!sink.0.lock().unwrap()[0].to_string().contains("token"));
}
#[tokio::test]
async fn telemetry_test_events_are_local_bounded_and_generation_fenced() {
    let (mut s, _) = setup(true).await;
    let sink = Arc::new(Diagnostics::default());
    s.telemetry = silicon_honeycomb_server::telemetry::Recorder::with_sink(sink.clone());
    sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,state,created_at,last_activity) VALUES('telemetry-env','tos','admin','Test','','unused','unused','ready',1,1)").execute(&s.db).await.unwrap();
    let record = |generation| {
        silicon_honeycomb_server::telemetry::record(
            &s,
            "telemetry-env",
            Some(generation),
            "cli",
            "command_completed",
            json!({"action":"apps"}),
        )
    };
    record(1).await;
    assert!(sink.0.lock().unwrap().is_empty());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM telemetry_events")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        1
    );
    sqlx::query("UPDATE environments SET generation=2 WHERE id='telemetry-env'")
        .execute(&s.db)
        .await
        .unwrap();
    record(1).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM telemetry_events")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        1
    );
    // Fill without exercising the external SDK; the next event enforces the per-plane cap.
    sqlx::query("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<1002) INSERT INTO telemetry_events(plane,generation,event,created_at) SELECT 'telemetry-env',2,'{}',1 FROM n").execute(&s.db).await.unwrap();
    record(2).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM telemetry_events")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        1000
    );
    sqlx::query("UPDATE environments SET state='cleaning' WHERE id='telemetry-env'")
        .execute(&s.db)
        .await
        .unwrap();
    record(2).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM telemetry_events")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        1000
    );
}
