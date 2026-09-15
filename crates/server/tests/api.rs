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
    s.management = Arc::new(ReviewManager::default());
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
}
