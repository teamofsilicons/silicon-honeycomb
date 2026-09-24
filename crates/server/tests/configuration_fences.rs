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
    auth::{Identity, IdentityProvider},
    error::{Error, Result},
    integration::{ArchiveStorage, Management},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::Notify;
use tower::ServiceExt;
const FIRST: &str = "00000000-0000-4000-8000-000000000001";
const SECOND: &str = "00000000-0000-4000-8000-000000000002";
struct Users;
#[async_trait]
impl IdentityProvider for Users {
    async fn login(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unauthorized())
    }
    async fn refresh(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unauthorized())
    }
    async fn revoke(&self, _: &str, _: &str, _: Option<&str>) -> Result<()> {
        Err(Error::unauthorized())
    }
    async fn authenticate(&self, _: &str, environment: Option<&str>) -> Result<Identity> {
        Ok(Identity {
            principal_id: "fixture-admin".into(),
            actor_type: Some("carbon".into()),
            organizations: BTreeMap::from([("alpha".into(), Some("org_admin".into()))]),
            testing_environment_id: environment.map(|key| {
                if key.starts_with('A') {
                    FIRST.into()
                } else {
                    SECOND.into()
                }
            }),
            validator: false,
        })
    }
}
struct Storage;
#[async_trait]
impl ArchiveStorage for Storage {
    async fn put(
        &self,
        _org: &str,
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
struct BlockingManager {
    calls: AtomicUsize,
    entered: Notify,
    release: Notify,
}
#[async_trait]
impl Management for BlockingManager {
    async fn configure(&self, o: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(
            json!({"state":"accepted","configuration_revision":o["configuration_revision"],"iam_revision":1,"effective_configuration":o["configuration"]}),
        )
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::unavailable("unused"))
    }
}
fn input(app: &str) -> Value {
    json!({"org_id":"alpha","app_id":app,"name":"Testing Application","description":"A useful application for the Silicon ecosystem. ".repeat(8),"webhook_url":"https://example.com/webhook","webhook_secret":"fixture-webhook-secret-long-enough","webhook_scope":["full"],"app_scope":{"iam":["self.identity.read"],"external":[]}})
}
async fn request(
    s: &State,
    key: Option<&str>,
    app: &str,
    idempotency: &str,
    update: bool,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(if update { "PUT" } else { "POST" })
        .uri(if update {
            format!("/api/v1/apps/{app}")
        } else {
            "/api/v1/apps".into()
        })
        .header("authorization", "Bearer fixture-admin")
        .header("content-type", "application/json")
        .header("idempotency-key", idempotency)
        .header("if-match", "1");
    if let Some(key) = key {
        request = request.header("x-testing-environment-key", key);
    }
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(request.body(Body::from(input(app).to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (status, body)
}
#[tokio::test]
async fn pending_configuration_serializes_the_entire_test_environment_without_blocking_other_planes()
 {
    let manager = Arc::new(BlockingManager {
        calls: AtomicUsize::new(0),
        entered: Notify::new(),
        release: Notify::new(),
    });
    let s = State {
        telemetry: Default::default(),
        db: silicon_honeycomb_server::database("sqlite::memory:")
            .await
            .unwrap(),
        identity: Arc::new(Users),
        management: manager.clone(),
        storage: Arc::new(Storage),
        app_id: "catalog".into(),
        iam_app_id: "identity".into(),
        iam_login_url: "https://iam.invalid".into(),
        encryption_key: [7; 32],
        webhook_secret: "fixture".into(),
    };
    for (id, key) in [(FIRST, "A".repeat(32)), (SECOND, "B".repeat(32))] {
        sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,state,created_at,last_activity) VALUES(?,'alpha','fixture-admin','Testing','',?,?,'ready',1,1)").bind(id).bind(s.encrypt(&key).unwrap()).bind(hex::encode(Sha256::digest(&key))).execute(&s.db).await.unwrap();
    }
    let first_state = s.clone();
    let first = tokio::spawn(async move {
        request(
            &first_state,
            Some(&"A".repeat(32)),
            "first",
            "first-config-operation",
            false,
        )
        .await
    });
    manager.entered.notified().await;
    // First configuration is already durable and waiting for external acceptance.
    let (status, _) = request(
        &s,
        Some(&"A".repeat(32)),
        "second",
        "second-config-operation",
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = request(
        &s,
        Some(&"A".repeat(32)),
        "first",
        "first-config-update-01",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(manager.calls.load(Ordering::SeqCst), 1);
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM applications WHERE plane=? AND app_id='first'")
            .bind(FIRST)
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(revision, 1);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM applications WHERE plane=? AND app_id='second'")
            .bind(FIRST)
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(count, 0);
    // The guard is per environment, not a global production lock.
    assert_eq!(
        request(
            &s,
            Some(&"B".repeat(32)),
            "independent",
            "second-env-operation",
            false
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        request(&s, None, "production", "production-operation", false)
            .await
            .0,
        StatusCode::ACCEPTED
    );
    manager.release.notify_one();
    assert_eq!(first.await.unwrap().0, StatusCode::ACCEPTED);
    assert_eq!(
        request(
            &s,
            Some(&"A".repeat(32)),
            "second",
            "second-config-operation",
            false
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
}
