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
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

struct Idp;
#[async_trait]
impl IdentityProvider for Idp {
    async fn login(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unauthorized())
    }
    async fn refresh(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unauthorized())
    }
    async fn revoke(&self, _: &str, _: &str, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    async fn authenticate(&self, token: &str, _: Option<&str>) -> Result<Identity> {
        if !["carbon", "silicon", "member"].contains(&token) {
            return Err(Error::unauthorized());
        }
        Ok(Identity {
            principal_id: format!("{}:owner", if token == "silicon" { "s" } else { "c" }),
            actor_type: Some(
                if token == "silicon" {
                    "silicon"
                } else {
                    "carbon"
                }
                .into(),
            ),
            organizations: BTreeMap::from([(
                "tos".into(),
                Some(
                    if token == "member" {
                        "org_member"
                    } else {
                        "org_owner"
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
    calls: Arc<Mutex<Vec<(String, String, String)>>>,
}
#[async_trait]
impl Management for Manager {
    async fn configure(&self, _: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unavailable("unused"))
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::unavailable("unused"))
    }
    async fn ata_verification(
        &self,
        app: &str,
        action: &str,
        _: Option<uuid::Uuid>,
        _: &Value,
        actor: &str,
        _: Option<uuid::Uuid>,
    ) -> Result<Value> {
        self.calls
            .lock()
            .unwrap()
            .push((app.into(), action.into(), actor.into()));
        if app == "unavailable" {
            return Err(Error::unavailable("IAM temporarily unavailable"));
        }
        if app == "unregistered" {
            return Ok(json!({"items":[]}));
        }
        Ok(json!({"items":[
            {"id":format!("{app}-carbon"),"app_id":app,"signing_principal":{"id":"c:owner","kind":"carbon"},"created_at":"2026-10-02T00:00:00Z","refresh_token":"must-not-leak"},
            {"id":format!("{app}-silicon"),"app_id":app,"signing_principal":{"id":"s:owner","kind":"silicon"},"created_at":"2026-10-03T00:00:00Z"},
            {"id":format!("{app}-someone-else"),"app_id":app,"signing_principal":{"id":"c:other","kind":"carbon"},"created_at":"2026-10-03T00:00:00Z"}
        ]}))
    }
}
struct Store;
#[async_trait]
impl ArchiveStorage for Store {
    async fn put(
        &self,
        _: &str,
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
async fn setup() -> (State, Arc<Mutex<Vec<(String, String, String)>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let state = State {
        telemetry: Default::default(),
        db: silicon_honeycomb_server::database("sqlite::memory:")
            .await
            .unwrap(),
        identity: Arc::new(Idp),
        management: Arc::new(Manager {
            calls: calls.clone(),
        }),
        storage: Arc::new(Store),
        app_id: "honeycomb".into(),
        iam_app_id: "iam".into(),
        iam_login_url: "https://iam.example.com".into(),
        encryption_key: [7; 32],
        webhook_secret: "test-only".into(),
    };
    for (app, org, revision) in [
        ("first", "tos", 1),
        ("second", "tos", 1),
        ("unavailable", "tos", 1),
        ("unregistered", "tos", 0),
        ("private-other-org", "other", 1),
    ] {
        sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,config,webhook_secret,iam_revision,created_at,updated_at) VALUES('production',?,?,?,'test','{}','unused',?,1,1)")
            .bind(app).bind(org).bind(app).bind(revision).execute(&state.db).await.unwrap();
    }
    (state, calls)
}
async fn get(state: State, actor: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().uri("/api/v1/ata-verifications");
    if let Some(actor) = actor {
        builder = builder.header("authorization", format!("Bearer {actor}"));
    }
    let response = silicon_honeycomb_server::api::router(state)
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    if status == StatusCode::OK {
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    (
        status,
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap(),
    )
}

#[tokio::test]
async fn personal_ata_workspace_filters_signer_across_apps_and_reports_partial_failures() {
    let (state, calls) = setup().await;
    let (status, value) = get(state, Some("carbon")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["signing_principal"]["id"], "c:owner");
    assert_eq!(value["items"].as_array().unwrap().len(), 2);
    assert!(
        value["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["signing_principal"]["id"] == "c:owner"
                && item.get("refresh_token").is_none())
    );
    assert_eq!(value["applications"].as_array().unwrap().len(), 4);
    assert_eq!(
        value["applications"]
            .as_array()
            .unwrap()
            .iter()
            .find(|app| app["app_id"] == "unregistered")
            .unwrap()["can_create"],
        false
    );
    assert_eq!(
        value["failures"],
        json!([{"app_id":"unavailable","message":"IAM temporarily unavailable"}])
    );
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|(app, action, actor)| {
        ["first", "second", "unavailable", "unregistered"].contains(&app.as_str())
            && action == "list"
            && actor == "carbon"
    }));
}
#[tokio::test]
async fn personal_ata_workspace_supports_silicon_signers() {
    let (state, _) = setup().await;
    let (status, value) = get(state, Some("silicon")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["items"].as_array().unwrap().len(), 2);
    assert!(
        value["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["signing_principal"]["id"] == "s:owner")
    );
}
#[tokio::test]
async fn personal_ata_workspace_does_not_expand_member_or_anonymous_authority() {
    let (state, calls) = setup().await;
    assert_eq!(get(state.clone(), None).await.0, StatusCode::UNAUTHORIZED);
    let (status, value) = get(state, Some("member")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["items"], json!([]));
    assert_eq!(value["applications"], json!([]));
    assert!(calls.lock().unwrap().is_empty());
}
#[tokio::test]
async fn centralized_navigation_does_not_authorize_cross_org_mutations() {
    let (state, calls) = setup().await;
    let response = silicon_honeycomb_server::api::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/apps/private-other-org/ata-verifications/preview")
                .header("authorization", "Bearer carbon")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(calls.lock().unwrap().is_empty());
}
