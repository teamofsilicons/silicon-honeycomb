//! Isolated OAuth always uses the selected environment's recovered app credential.
use async_trait::async_trait;
use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use silicon_honeycomb_server::{
    auth::{Iam, IdentityProvider},
    error::{Error, Result},
    integration::Management,
};
use sqlx::SqlitePool;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};
const APP: &str = "custom>catalog";
const ENV: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const ROOT: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const PROD: &str = "ask_ppppppppppppppppppppppppppppppppppppppppppp";
fn secret(n: usize) -> String {
    format!("ask_{n:043}")
}
struct Recovery {
    db: SqlitePool,
    mode: &'static str,
    calls: AtomicUsize,
}
#[async_trait]
impl Management for Recovery {
    async fn configure(&self, _: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        unreachable!()
    }
    async fn recover_testing_application_credential(
        &self,
        env: &str,
        authorization: &str,
    ) -> Result<Value> {
        assert_eq!(env, ENV);
        assert_eq!(
            authorization,
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("{APP}:{PROD}"))
            )
        );
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.mode == "failure" {
            return Err(Error::unavailable(format!("sensitive {PROD}")));
        }
        if self.mode == "rotate" {
            sqlx::query("UPDATE environments SET generation=generation+1 WHERE id=?")
                .bind(env)
                .execute(&self.db)
                .await?;
        }
        Ok(
            json!({"environment_id":if self.mode == "foreign" {"other"} else {env},"app_id":APP,"app_secret":if self.mode == "production" {PROD.to_owned()} else {secret(n)}}),
        )
    }
}
async fn fixture(server: &MockServer, mode: &'static str) -> (Iam, Arc<Recovery>) {
    let db = silicon_honeycomb_server::database(":memory:")
        .await
        .unwrap();
    sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,state,created_at,last_activity) VALUES(?,'custom','user','Test','','encrypted',?,'ready',1,1)")
        .bind(ENV).bind(hex::encode(Sha256::digest(ROOT))).execute(&db).await.unwrap();
    let recovery = Arc::new(Recovery {
        db: db.clone(),
        mode,
        calls: AtomicUsize::new(0),
    });
    (
        Iam::new(&server.uri(), APP, PROD)
            .unwrap()
            .with_testing_context(db, recovery.clone()),
        recovery,
    )
}
async fn mocks(server: &MockServer) {
    Mock::given(method("POST")).and(path("/api/v1/app-auth/tokens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token":"oat_test","refresh_token":"ort_test","token_type":"Bearer","expires_in":3600,"scope":"self.identity.read"})))
        .mount(server).await;
    Mock::given(method("POST")).and(path("/api/v1/oauth/introspect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"active":true,"public_id":"test-carbon","client_id":APP,"authorizations":[{"public_id":"test-carbon","organization_id":"22222222-2222-4222-8222-222222222222","org_id":"custom","membership_id":"33333333-3333-4333-8333-333333333333","membership_version":1,"authorization_epoch":1,"audience":APP,"testing_environment_id":ENV,"scopes":["self.identity.read"],"org_role":"owner"}]})))
        .mount(server).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/oauth/revoke"))
        .respond_with(ResponseTemplate::new(204))
        .mount(server)
        .await;
}
#[tokio::test]
async fn all_oauth_paths_recover_current_test_credentials_without_caching_or_production_fallback() {
    let server = MockServer::start().await;
    mocks(&server).await;
    let (iam, recovery) = fixture(&server, "normal").await;
    iam.login("slt_test", "login-testing-auth", Some(ROOT))
        .await
        .unwrap();
    iam.refresh("ort_test", "refresh-testing-auth", Some(ROOT))
        .await
        .unwrap();
    let identity = iam.authenticate("oat_test", Some(ROOT)).await.unwrap();
    assert_eq!(identity.testing_environment_id.as_deref(), Some(ENV));
    assert!(identity.admin("custom"));
    iam.revoke("ort_test", "revoke-testing-auth", Some(ROOT))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 4);
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(request.headers["x-testing-environment-key"], ROOT);
        assert_eq!(
            request.headers["authorization"],
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD
                    .encode(format!("{APP}:{}", secret(index + 1)))
            )
        );
    }
    iam.login("slt_prod", "login-production-auth", None)
        .await
        .unwrap();
    assert_eq!(recovery.calls.load(Ordering::SeqCst), 4);
    let requests = server.received_requests().await.unwrap();
    let last = requests.last().unwrap();
    assert!(!last.headers.contains_key("x-testing-environment-key"));
    assert_eq!(
        last.headers["authorization"],
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{APP}:{PROD}"))
        )
    );
}
#[tokio::test]
async fn unavailable_foreign_production_or_stale_credentials_never_reach_oauth_or_expose_secrets() {
    let server = MockServer::start().await;
    for mode in ["failure", "foreign", "production", "rotate"] {
        let (iam, _) = fixture(&server, mode).await;
        let error = iam
            .login("slt_test", "login-testing-reject", Some(ROOT))
            .await
            .unwrap_err();
        assert!(!format!("{error:?}").contains(PROD));
        assert!(!format!("{error:?}").contains(ROOT));
    }
    let (iam, recovery) = fixture(&server, "normal").await;
    for key in ["invalid", "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"] {
        assert!(
            iam.login("slt_test", "login-testing-invalid", Some(key))
                .await
                .is_err()
        );
    }
    sqlx::query("UPDATE environments SET state='deleted'")
        .execute(&recovery.db)
        .await
        .unwrap();
    assert!(
        iam.login("slt_test", "login-testing-deleted", Some(ROOT))
            .await
            .is_err()
    );
    assert_eq!(recovery.calls.load(Ordering::SeqCst), 0);
    let unconfigured = Iam::new(&server.uri(), APP, PROD).unwrap();
    assert!(
        unconfigured
            .login("slt_test", "login-testing-unconfigured", Some(ROOT))
            .await
            .is_err()
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}
