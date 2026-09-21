//! Version negotiation and lifecycle are independent of authentication and telemetry.
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use silicon_honeycomb_server::{
    State,
    auth::Iam,
    contracts,
    error::{Error, Result},
    integration::{ArchiveStorage, AwaitingIamIntegration},
    now,
};
use std::sync::Arc;
use tower::ServiceExt;

struct UnusedStorage;
#[async_trait]
impl ArchiveStorage for UnusedStorage {
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
async fn setup() -> State {
    State {
        telemetry: Default::default(),
        db: silicon_honeycomb_server::database("sqlite::memory:")
            .await
            .unwrap(),
        identity: Arc::new(Iam::new("http://127.0.0.1:1", "tos>honeycomb", "unused").unwrap()),
        management: Arc::new(AwaitingIamIntegration),
        storage: Arc::new(UnusedStorage),
        app_id: "tos>honeycomb".into(),
        iam_app_id: "tos>iam".into(),
        iam_login_url: "https://iam.invalid".into(),
        encryption_key: [1; 32],
        webhook_secret: "unused".into(),
    }
}
async fn get(
    s: &State,
    path: &str,
    version: Option<&str>,
    client: Option<&str>,
) -> (StatusCode, HeaderMap, Value) {
    let mut request = Request::get(path);
    if let Some(version) = version {
        request = request.header("honeycomb-api-version", version);
    }
    if let Some(client) = client {
        request = request.header("honeycomb-client-version", client);
    }
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn last_request(s: &State, version: &str) -> Option<i64> {
    sqlx::query_scalar("SELECT last_request_at FROM api_contracts WHERE version=?")
        .bind(version)
        .fetch_one(&s.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn discovery_preserves_legacy_selection_and_advertises_scoped_release_contract() {
    let s = setup().await;
    for path in ["/api/contracts", "/api/v1/contract"] {
        let (status, _, body) = get(&s, path, None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["selected_version"], "v1");
        assert_eq!(body["minimum_client"], "0.1.0");
        assert_eq!(body["release_version"], "v2");
        assert_eq!(body["supported_versions"], json!(["v1", "v2"]));
        assert_eq!(body["versions"][0]["scope"], "general");
        assert_eq!(body["versions"][1]["scope"], "releases");
        assert_eq!(body["obo_endpoints"][0]["critical"], true);
    }
    let (status, headers, body) = get(&s, "/api/v2/contract", Some("v2"), Some("0.3.0")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["honeycomb-api-version"], "v2");
    assert_eq!(headers["honeycomb-contract-state"], "active");
    assert_eq!(body["selected_version"], "v2");
    assert_eq!(body["minimum_client"], "0.3.0");
    assert_eq!(
        get(&s, "/api/v2/iam", Some("v2"), None).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn major_selection_minimum_clients_and_traffic_accounting_are_independent() {
    let s = setup().await;
    sqlx::query("UPDATE api_contracts SET last_request_at=11")
        .execute(&s.db)
        .await
        .unwrap();
    for (path, version, client, status, code) in [
        (
            "/api/v1/iam",
            Some("v2"),
            Some("0.3.0"),
            StatusCode::NOT_ACCEPTABLE,
            "api_version_unsupported",
        ),
        (
            "/api/v2/contract",
            Some("v1"),
            Some("0.3.0"),
            StatusCode::NOT_ACCEPTABLE,
            "api_version_unsupported",
        ),
        (
            "/api/v3/contract",
            None,
            None,
            StatusCode::NOT_ACCEPTABLE,
            "api_version_unsupported",
        ),
        (
            "/api/v2/contract",
            Some("v2"),
            Some("invalid"),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
    ] {
        let response = get(&s, path, version, client).await;
        assert_eq!(response.0, status);
        assert_eq!(response.2["error"]["code"], code);
        assert_eq!(last_request(&s, "v1").await, Some(11));
        assert_eq!(last_request(&s, "v2").await, Some(11));
    }
    assert_eq!(
        get(&s, "/api/v1/iam", Some("v1"), Some("0.2.2")).await.0,
        StatusCode::OK
    );
    assert!(last_request(&s, "v1").await.unwrap() > 11);
    assert_eq!(last_request(&s, "v2").await, Some(11));
    let response = get(&s, "/api/v2/contract", Some("v2"), Some("0.2.2")).await;
    assert_eq!(response.0, StatusCode::BAD_REQUEST);
    assert_eq!(response.2["error"]["code"], "client_upgrade_required");
    assert!(last_request(&s, "v2").await.unwrap() > 11);
    assert_eq!(
        get(&s, "/api/v2/contract", None, None).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn one_majors_requests_do_not_prevent_the_others_retirement_or_reactivate_it() {
    for retired in ["v1", "v2"] {
        let s = setup().await;
        let active = if retired == "v1" { "v2" } else { "v1" };
        let old = now() - contracts::QUIET_PERIOD_SECONDS - 1;
        sqlx::query("UPDATE api_contracts SET state='deprecated',deprecated_at=?,last_request_at=? WHERE version=?")
            .bind(old).bind(old).bind(retired).execute(&s.db).await.unwrap();
        assert_eq!(
            get(
                &s,
                &format!("/api/{active}/contract"),
                Some(active),
                Some("0.3.0")
            )
            .await
            .0,
            StatusCode::OK
        );
        let response = get(
            &s,
            &format!("/api/{retired}/contract"),
            Some(retired),
            Some("99.0.0"),
        )
        .await;
        assert_eq!(response.0, StatusCode::GONE);
        assert_eq!(response.1["honeycomb-api-version"], retired);
        assert_eq!(response.1["honeycomb-contract-state"], "retired");
        assert_eq!(last_request(&s, retired).await, Some(old));
        let discovery = get(&s, "/api/contracts", None, None).await;
        assert_eq!(discovery.0, StatusCode::OK);
        assert_eq!(discovery.2["selected_version"], "v1");
        assert_eq!(discovery.2["supported_versions"], json!([active]));
    }
}

#[tokio::test]
async fn migration_preserves_existing_v1_policy_and_lifecycle() {
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0015_contracts.sql"))
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE api_contracts SET state='deprecated',minimum_client='0.2.0',deprecated_at=10,last_request_at=20").execute(&db).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0025_release_contract.sql"))
        .execute(&db)
        .await
        .unwrap();
    let v1:(String,String,i64,i64)=sqlx::query_as("SELECT state,minimum_client,deprecated_at,last_request_at FROM api_contracts WHERE version='v1'").fetch_one(&db).await.unwrap();
    assert_eq!(v1, ("deprecated".into(), "0.2.0".into(), 10, 20));
    let v2: (String, String, Option<i64>) = sqlx::query_as(
        "SELECT state,minimum_client,last_request_at FROM api_contracts WHERE version='v2'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(v2, ("active".into(), "0.3.0".into(), None));
}
