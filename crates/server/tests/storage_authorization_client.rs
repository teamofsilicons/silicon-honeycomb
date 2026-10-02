//! Official client contracts for feature-time consent and safe upload recovery.
use honeycomb_client::{Client, Mutation};
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path},
};

#[tokio::test]
async fn storage_consent_client_binds_current_account_and_preserves_operation_keys() {
    let server = MockServer::start().await;
    let client = Client::new(&server.uri())
        .unwrap()
        .with_token("fixture-user");
    let mutation = Mutation {
        idempotency_key: "fixture-operation-key".into(),
        revision: None,
    };
    Mock::given(method("POST")).and(path("/api/v1/storage-authorizations"))
        .and(header("authorization","Bearer fixture-user")).and(header("idempotency-key","fixture-operation-key"))
        .and(body_json(json!({"org_id":"tos","return_url":null})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"authorization_id":"request-id","consent_url":"https://iam.example.test/obo/consent?request=request-id","state":"fixture-state","status":"pending"})))
        .expect(1).mount(&server).await;
    let started = client
        .storage_authorization_start("tos", None, &mutation)
        .await
        .unwrap();
    assert_eq!(started["status"], "pending");
    Mock::given(method("GET"))
        .and(path("/api/v1/storage-authorizations/request-id"))
        .and(header("authorization", "Bearer fixture-user"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"authorization_id":"request-id","status":"pending"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client
            .storage_authorization_status("request-id")
            .await
            .unwrap()["status"],
        "pending"
    );
    Mock::given(method("POST"))
        .and(path("/api/v1/storage-authorizations/request-id/complete"))
        .and(header("authorization", "Bearer fixture-user"))
        .and(header("idempotency-key", "fixture-operation-key"))
        .and(body_json(
            json!({"code":"fixture-one-time-code","state":"fixture-state"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"authorization_id":"request-id","status":"ready"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client
            .storage_authorization_complete(
                "request-id",
                "fixture-one-time-code",
                "fixture-state",
                &mutation
            )
            .await
            .unwrap()["status"],
        "ready"
    );
}
#[tokio::test]
async fn authorization_required_is_distinguishable_from_denied_access() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/api/v1/apps/fixture"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({"error":{"code":"storage_authorization_required","message":"Authorize storage","details":[]}})))
        .mount(&server).await;
    let error = Client::new(&server.uri())
        .unwrap()
        .app("fixture")
        .await
        .unwrap_err();
    let error = error.downcast_ref::<honeycomb_client::ApiError>().unwrap();
    assert_eq!(error.status, 403);
    assert_eq!(error.code, "storage_authorization_required");
}
