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
    async fn publish(&self, _: &str, _: &str, _: Option<&str>) -> Result<()> {
        Ok(())
    }
}
async fn setup(accept: bool) -> (State, Arc<AtomicBool>) {
    let revoked = Arc::new(AtomicBool::new(false));
    let s = State {
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
