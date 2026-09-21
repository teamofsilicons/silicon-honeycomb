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
    integration::{ArchiveStorage, Management, TestingApplicationIdentity},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tower::ServiceExt;

struct Users;
#[async_trait]
impl IdentityProvider for Users {
    async fn login(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unauthorized())
    }
    async fn refresh(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unauthorized())
    }
    async fn authenticate(&self, _: &str, _: Option<&str>) -> Result<Identity> {
        Err(Error::unauthorized())
    }
    async fn revoke(&self, _: &str, _: &str, _: Option<&str>) -> Result<()> {
        Err(Error::unauthorized())
    }
}
struct Storage;
#[async_trait]
impl ArchiveStorage for Storage {
    async fn put(
        &self,
        _: &str,
        _: &str,
        _: &std::path::Path,
        _: &str,
        _: Option<&str>,
        _: &str,
    ) -> Result<String> {
        Err(Error::unavailable("unused"))
    }
    async fn read(&self, _: &str, _: &str, _: Option<&str>, _: Option<&str>) -> Result<Vec<u8>> {
        Err(Error::unavailable("unused"))
    }
    async fn publish(&self, _: &str, _: &str, _: &str, _: Option<&str>, _: &str) -> Result<String> {
        Err(Error::unavailable("unused"))
    }
}
struct Apps {
    revoked: AtomicBool,
    recreated: AtomicBool,
    fail_attached: AtomicBool,
    recovery_ready: AtomicBool,
    linked_ids: std::sync::Mutex<Option<Vec<String>>>,
}
#[async_trait]
impl Management for Apps {
    async fn configure(&self, _: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unavailable("unused"))
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        panic!("App authority must never become a human token")
    }
    async fn verify_testing_application(
        &self,
        authorization: &str,
    ) -> Result<TestingApplicationIdentity> {
        if self.revoked.load(Ordering::SeqCst) {
            return Err(Error::unauthorized());
        }
        let (app_id, id) = match authorization {
            "Basic owner-credential" => (
                "alpha>app",
                if self.recreated.load(Ordering::SeqCst) {
                    "alpha>other"
                } else {
                    "alpha>app"
                },
            ),
            "Basic attached-credential" => ("beta>app", "beta>app"),
            _ => return Err(Error::unauthorized()),
        };
        Ok(TestingApplicationIdentity {
            application_id: id.into(),
            app_id: app_id.into(),
            org_id: app_id.split_once('>').unwrap().0.into(),
            organization_id: "00000000-0000-4000-8000-000000000010".into(),
            iam_revision: 1,
        })
    }
    async fn testing_application_environment_ids(
        &self,
        authorization: &str,
    ) -> Result<Vec<String>> {
        self.verify_testing_application(authorization).await?;
        self.linked_ids
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::unavailable("fixture sensitive upstream error"))
    }
    async fn recover_testing_application_credential(
        &self,
        environment_id: &str,
        authorization: &str,
    ) -> Result<Value> {
        if !self.recovery_ready.load(Ordering::SeqCst) {
            return Err(Error::unavailable("recovery pending"));
        }
        let identity = self.verify_testing_application(authorization).await?;
        Ok(
            json!({"environment_id":environment_id,"app_id":identity.app_id,"app_secret":"fixture-recovered-secret"}),
        )
    }
    async fn application_lifecycle(
        &self,
        o: &Value,
        authorization: &str,
        attachment_key: Option<&str>,
    ) -> Result<Value> {
        let identity = self.verify_testing_application(authorization).await?;
        if identity.app_id == "beta>app" {
            assert_eq!(attachment_key, o["testing_key"].as_str());
        }
        let mut receipt = receipt("platform>iam", o);
        if let Some(imports) = o["snapshot"]["imports"].as_array() {
            receipt["imports"] = json!(imports.iter().map(|i|json!({"app_id":i["app_id"],"source_revision":i["source_revision"],"configuration_revision":i["configuration_revision"],"iam_revision":2,"effective_configuration":i["configuration"],"visibility":"private"})).collect::<Vec<_>>());
            receipt["application_credential"] = json!({"app_id":identity.app_id,"environment_id":o["environment_id"],"app_secret":format!("fixture-test-secret-{}",identity.application_id)});
        }
        Ok(receipt)
    }
    async fn service_lifecycle(&self, app: &str, o: &Value, token: &str) -> Result<Value> {
        assert!(
            token.is_empty(),
            "Never forward app credentials to another participant"
        );
        if app == "beta>app" && self.fail_attached.load(Ordering::SeqCst) {
            return Err(Error::unavailable("fixture secret that must not be logged"));
        }
        Ok(receipt(app, o))
    }
}
fn receipt(app: &str, o: &Value) -> Value {
    json!({"state":"completed","operation_id":o["operation_id"],"environment_id":o["environment_id"],"app_id":app,"environment_revision":o["environment_revision"],"generation":o["generation"],"key_version":o["key_version"]})
}
async fn setup() -> (State, Arc<Apps>) {
    setup_at("sqlite::memory:").await
}
async fn setup_at(database: &str) -> (State, Arc<Apps>) {
    let apps = Arc::new(Apps {
        revoked: AtomicBool::new(false),
        recreated: AtomicBool::new(false),
        fail_attached: AtomicBool::new(false),
        recovery_ready: AtomicBool::new(false),
        linked_ids: std::sync::Mutex::new(Some(vec![])),
    });
    let s = State {
        telemetry: Default::default(),
        db: silicon_honeycomb_server::database(database).await.unwrap(),
        identity: Arc::new(Users),
        management: apps.clone(),
        storage: Arc::new(Storage),
        app_id: "platform>honeycomb".into(),
        iam_app_id: "platform>iam".into(),
        iam_login_url: "https://iam.invalid".into(),
        encryption_key: [7; 32],
        webhook_secret: "fixture".into(),
    };
    for app in ["alpha>app", "beta>app"] {
        let config =
            json!({"name":app,"description":"fixture","app_scope":{"iam":[],"external":[]}})
                .to_string();
        sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,visibility,state,revision,iam_revision,config,effective_config,effective_revision,webhook_secret,created_at,updated_at) VALUES('production',?,?,?,'fixture','private','active',1,1,?,?,1,'',1,1)")
            .bind(app).bind(app.split_once('>').unwrap().0).bind(app).bind(&config).bind(&config).execute(&s.db).await.unwrap();
    }
    (s, apps)
}
async fn call(
    s: &State,
    method: &str,
    path: &str,
    credential: &str,
    body: Value,
    key: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", credential)
        .header("idempotency-key", key)
        .header("if-match", "2")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    if method == "POST" && path == "/api/v1/environments" && status.is_success() {
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    let value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (status, value)
}
#[tokio::test]
async fn app_owned_setup_preserves_identity_permissions_and_secret_boundaries() {
    let (s, apps) = setup().await;
    let (status, created) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic owner-credential",
        json!({"name":"App testing"}),
        "owner-create-0001",
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{created}");
    assert_eq!(created["state"], "ready");
    assert_eq!(created["credential_state"], "ready");
    assert_eq!(created["can_manage"], true);
    assert!(!created.to_string().contains("testing_key"));
    let id = created["environment_id"].as_str().unwrap();
    let (_, key) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/key"),
        "Basic owner-credential",
        json!({}),
        "owner-key-read-001",
    )
    .await;
    let root = key["testing_key"].as_str().unwrap();
    // Cross-org attachment preserves each app's organization and all previously ready apps.
    let (status, attached) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic attached-credential",
        json!({"name":"Attach","iam_test_key":root}),
        "attached-create-001",
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{attached}");
    assert_eq!(attached["can_manage"], false);
    assert_eq!(attached["credential_state"], "ready");
    assert_eq!(attached["org_id"], "alpha");
    assert_ne!(attached["app_secret"], created["app_secret"]);
    let org: String =
        sqlx::query_scalar("SELECT org_id FROM applications WHERE plane=? AND app_id='beta>app'")
            .bind(id)
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(org, "beta");
    let (_, listed) = call(
        &s,
        "GET",
        "/api/v1/environments",
        "Basic attached-credential",
        json!({}),
        "attached-list-0001",
    )
    .await;
    assert_eq!(listed["items"][0]["can_manage"], false);
    assert!(!listed.to_string().contains(root));
    assert!(!listed.to_string().contains("app_secret"));
    let (status, _) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/key"),
        "Basic attached-credential",
        json!({}),
        "attached-key-read1",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/actions/clean"),
        "Basic attached-credential",
        json!({}),
        "attached-clean001",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let operations:Vec<String>=sqlx::query_scalar("SELECT coalesce(request_json,'') || coalesce(result,'') || coalesce(error,'') FROM operations").fetch_all(&s.db).await.unwrap();
    assert!(operations.iter().all(|o| !o.contains(root)
        && !o.contains("credential")
        && !o.contains("fixture-test-secret")));
    let receipts: Vec<String> =
        sqlx::query_scalar("SELECT coalesce(receipt,'') FROM environment_services")
            .fetch_all(&s.db)
            .await
            .unwrap();
    assert!(receipts.iter().all(|o| !o.contains("fixture-test-secret")));
    apps.revoked.store(true, Ordering::SeqCst);
    let (status, _) = call(
        &s,
        "GET",
        "/api/v1/environments",
        "Basic owner-credential",
        json!({}),
        "revoked-list-0001",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    apps.revoked.store(false, Ordering::SeqCst);
    apps.recreated.store(true, Ordering::SeqCst);
    let (status, _) = call(
        &s,
        "GET",
        "/api/v1/environments",
        "Basic owner-credential",
        json!({}),
        "mismatched-list001",
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let (status, _) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/key"),
        "Basic owner-credential",
        json!({}),
        "recreated-key-001",
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
#[tokio::test]
async fn invalid_or_conflicting_attachment_never_creates_an_environment() {
    let (s, _) = setup().await;
    for body in [
        json!({"name":"Bad key","testing_key":"X".repeat(32)}),
        json!({"name":"Conflict","testing_key":"X".repeat(32),"iam_test_key":"Y".repeat(32)}),
        json!({"name":"Wrong org","org_id":"other"}),
    ] {
        let (status, _) = call(
            &s,
            "POST",
            "/api/v1/environments",
            "Basic owner-credential",
            body,
            "invalid-create-001",
        )
        .await;
        assert!(status.is_client_error());
    }
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/environments")
        .header("authorization", "Basic owner-credential")
        .header("x-testing-environment-key", "X".repeat(32))
        .header("idempotency-key", "mixed-auth-create001")
        .header("content-type", "application/json")
        .body(Body::from(json!({"name":"Mixed plane"}).to_string()))
        .unwrap();
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM environments")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn partial_attach_preserves_other_apps_and_replay_exposes_recovery_pending() {
    let (s, apps) = setup().await;
    let (_, created) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic owner-credential",
        json!({"name":"Testing"}),
        "retry-create-0001",
    )
    .await;
    let id = created["environment_id"].as_str().unwrap();
    let (_, key) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/key"),
        "Basic owner-credential",
        json!({}),
        "retry-key-read001",
    )
    .await;
    apps.fail_attached.store(true, Ordering::SeqCst);
    let body = json!({"name":"Attach","testing_key":key["testing_key"]});
    let (_, pending) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic attached-credential",
        body.clone(),
        "retry-attach-0001",
    )
    .await;
    assert_eq!(pending["operation_state"], "pending");
    assert!(pending.get("app_secret").is_none());
    let state: String = sqlx::query_scalar(
        "SELECT state FROM environment_services WHERE environment_id=? AND app_id='alpha>app'",
    )
    .bind(id)
    .fetch_one(&s.db)
    .await
    .unwrap();
    assert_eq!(state, "ready");
    apps.fail_attached.store(false, Ordering::SeqCst);
    let (_, replayed) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic attached-credential",
        body,
        "retry-attach-0001",
    )
    .await;
    assert_eq!(replayed["state"], "ready");
    assert_eq!(replayed["credential_state"], "pending");
    assert!(replayed.get("app_secret").is_none());
    let errors: Vec<String> =
        sqlx::query_scalar("SELECT coalesce(error,'') FROM environment_services")
            .fetch_all(&s.db)
            .await
            .unwrap();
    assert!(errors.iter().all(|e| !e.contains("fixture secret")));
}

#[tokio::test]
async fn held_additive_import_lease_never_delivers_credentials_while_environment_is_ready() {
    let (s, apps) = setup().await;
    let (_, created) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic owner-credential",
        json!({"name":"Lease testing"}),
        "lease-create-0001",
    )
    .await;
    let id = created["environment_id"].as_str().unwrap();
    let (_, key) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/key"),
        "Basic owner-credential",
        json!({}),
        "lease-key-read001",
    )
    .await;
    let body = json!({"name":"Attach", "testing_key":key["testing_key"]});
    apps.fail_attached.store(true, Ordering::SeqCst);
    let (_, pending) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic attached-credential",
        body.clone(),
        "lease-attach-001",
    )
    .await;
    assert_eq!(pending["operation_state"], "pending");
    sqlx::query("UPDATE operations SET lease_token='held',lease_until=? WHERE id=?")
        .bind(silicon_honeycomb_server::now() + 300)
        .bind(pending["operation_id"].as_str().unwrap())
        .execute(&s.db)
        .await
        .unwrap();
    apps.recovery_ready.store(true, Ordering::SeqCst);
    let (_, replay) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic attached-credential",
        body,
        "lease-attach-001",
    )
    .await;
    assert_eq!(replay["state"], "ready");
    assert_eq!(replay["operation_state"], "pending");
    assert_ne!(replay["credential_state"], "ready");
    assert!(replay.get("app_secret").is_none());
}

#[tokio::test]
async fn iam_dependency_links_allow_listing_only_and_discovery_failures_are_explicit() {
    let (s, apps) = setup().await;
    let (_, created) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic owner-credential",
        json!({"name":"Dependency testing"}),
        "dependency-create01",
    )
    .await;
    let id = created["environment_id"].as_str().unwrap();
    // This app has no local ownership/attachment link; IAM attests dependency import.
    *apps.linked_ids.lock().unwrap() = Some(vec![id.to_owned(), uuid::Uuid::new_v4().to_string()]);
    let (_, linked) = call(
        &s,
        "GET",
        "/api/v1/environments",
        "Basic attached-credential",
        json!({}),
        "dependency-list01",
    )
    .await;
    assert_eq!(linked["linkage_discovery"], "complete");
    assert_eq!(linked["items"].as_array().unwrap().len(), 1);
    assert_eq!(linked["items"][0]["environment_id"], id);
    assert_eq!(linked["items"][0]["can_manage"], false);
    let (status, _) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/key"),
        "Basic attached-credential",
        json!({}),
        "dependency-key001",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/actions/clean"),
        "Basic attached-credential",
        json!({}),
        "dependency-clean1",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for ids in [
        None,
        Some(vec!["malformed-environment".into()]),
        Some(vec![id.to_owned(), id.to_owned()]),
    ] {
        *apps.linked_ids.lock().unwrap() = ids;
        let (_, linked) = call(
            &s,
            "GET",
            "/api/v1/environments",
            "Basic attached-credential",
            json!({}),
            "invalid-link-list01",
        )
        .await;
        assert_eq!(linked["linkage_discovery"], "pending");
        assert_eq!(linked["items"], json!([]));
        assert!(!linked.to_string().contains("sensitive"));
        let (_, owned) = call(
            &s,
            "GET",
            "/api/v1/environments",
            "Basic owner-credential",
            json!({}),
            "owner-link-list001",
        )
        .await;
        assert_eq!(owned["linkage_discovery"], "pending");
        assert_eq!(owned["items"][0]["can_manage"], true);
    }
}

#[tokio::test]
async fn migrated_application_owner_keeps_the_same_world_key_and_create_retry() {
    for edited in [false, true] {
        migrated_application_owner_case(edited).await;
    }
}
async fn migrated_application_owner_case(edited: bool) {
    use sha2::{Digest, Sha256};
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("retained.sqlite");
    let (mut s, _) = setup_at(database.to_str().unwrap()).await;
    let body = json!({"name":"Retained world"});
    let (_, created) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic owner-credential",
        body.clone(),
        "retained-create001",
    )
    .await;
    let id = created["environment_id"].as_str().unwrap().to_owned();
    let original: (String, String) =
        sqlx::query_as("SELECT encrypted_key,key_hash FROM environments WHERE id=?")
            .bind(&id)
            .fetch_one(&s.db)
            .await
            .unwrap();
    let legacy = "44444444-4444-4444-8444-444444444444";
    sqlx::query("UPDATE environments SET creator=?,creator_application_id=? WHERE id=?")
        .bind(format!("application:{legacy}"))
        .bind(legacy)
        .bind(&id)
        .execute(&s.db)
        .await
        .unwrap();
    sqlx::query("UPDATE environment_application_links SET application_id=? WHERE environment_id=?")
        .bind(legacy)
        .bind(&id)
        .execute(&s.db)
        .await
        .unwrap();
    sqlx::query("UPDATE operations SET actor=? WHERE actor='application:alpha>app'")
        .bind(format!("application:{legacy}"))
        .execute(&s.db)
        .await
        .unwrap();
    let old_digest = hex::encode(Sha256::digest(json!({"kind":"application.environment.create","application_id":legacy,"org":"alpha","name":"Retained world","description":""}).to_string()));
    sqlx::query(
        "UPDATE operations SET request_hash=? WHERE resource=? AND kind='environment.prepare'",
    )
    .bind(old_digest)
    .bind(&id)
    .execute(&s.db)
    .await
    .unwrap();
    let (status, _) = call(
        &s,
        "GET",
        &format!("/api/v1/environments/{id}"),
        "Basic owner-credential",
        json!({}),
        "before-import001",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Canonical authentication must not fall back to an old UUID owner"
    );
    if edited {
        sqlx::query("UPDATE environments SET name='Edited after creation' WHERE id=?")
            .bind(&id)
            .execute(&s.db)
            .await
            .unwrap();
    }
    // Rehearse the deployed schema22 path: the offline importer creates the
    // private context table, and normal startup later records migration23.
    sqlx::query("DROP TABLE application_create_hash_contexts")
        .execute(&s.db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version=23")
        .execute(&s.db)
        .await
        .unwrap();
    s.db.close().await;
    let mapping = temp.path().join("mapping.json");
    std::fs::write(&mapping, json!({"production":[{"legacy_id":legacy,"kind":"application","public_id":"alpha>app","testing_environment_id":null}]}).to_string()).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/import-iam-identities.py");
    for repeat in [false, true] {
        let result = std::process::Command::new("python3")
            .arg(&script)
            .arg("--database")
            .arg(&database)
            .arg("--identity-map")
            .arg(&mapping)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if repeat {
            assert!(String::from_utf8_lossy(&result.stdout).contains("Updated 0 "));
        }
    }
    s.db = silicon_honeycomb_server::database(&format!("sqlite://{}", database.display()))
        .await
        .unwrap();
    let contexts: i64 = sqlx::query_scalar("SELECT count(*) FROM application_create_hash_contexts")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(contexts, i64::from(edited));
    let migrated: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=23 AND success=1)",
    )
    .fetch_one(&s.db)
    .await
    .unwrap();
    assert!(migrated);
    let (status, found) = call(
        &s,
        "GET",
        &format!("/api/v1/environments/{id}"),
        "Basic owner-credential",
        json!({}),
        "after-import001",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["environment_id"], id);
    let (status, key) = call(
        &s,
        "POST",
        &format!("/api/v1/environments/{id}/key"),
        "Basic owner-credential",
        json!({}),
        "migrated-key-0001",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(key["testing_key"], s.decrypt(&original.0).unwrap());
    let retained: (String, String) =
        sqlx::query_as("SELECT encrypted_key,key_hash FROM environments WHERE id=?")
            .bind(&id)
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(retained, original);
    let (status, _) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic owner-credential",
        json!({"name":"Different input"}),
        "retained-create001",
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, replay) = call(
        &s,
        "POST",
        "/api/v1/environments",
        "Basic owner-credential",
        body,
        "retained-create001",
    )
    .await;
    assert!(status.is_success(), "{replay}");
    assert_eq!(replay["environment_id"], id);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM environments")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(count, 1);
    for suffix in ["", "/key"] {
        let (status, _) = call(
            &s,
            if suffix.is_empty() { "GET" } else { "POST" },
            &format!("/api/v1/environments/{id}{suffix}"),
            "Basic attached-credential",
            json!({}),
            "not-owner-key-0001",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
