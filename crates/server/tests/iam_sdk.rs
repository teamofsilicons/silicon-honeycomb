//! Contract checks through the published IAM SDK, using a local HTTP service.
use serde_json::{Value, json};
use silicon_honeycomb_server::auth::{Iam, IdentityProvider};
use silicon_iam_client::{Client, Credential};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, header_exists, method, path},
};
fn adapter(server: &MockServer) -> Iam {
    Iam {
        client: Client::builder(&server.uri())
            .unwrap()
            .telemetry(false)
            .credential(Credential::application(
                "tos>honeycomb",
                "fixture-app-secret",
            ))
            .build()
            .unwrap(),
        app_id: "tos>honeycomb".into(),
    }
}
fn snapshot(role: Option<&str>) -> Value {
    json!({"active":true,"principal_id":"11111111-1111-1111-1111-111111111111","client_id":"tos>honeycomb","actor_type":"carbon","authorizations":[{"principal_id":"11111111-1111-1111-1111-111111111111","organization_id":"22222222-2222-2222-2222-222222222222","org_id":"tos","membership_id":"33333333-3333-3333-3333-333333333333","membership_version":1,"authorization_epoch":1,"audience":"tos>honeycomb","testing_environment_id":null,"scopes":["self.identity.read","self.membership.read"],"org_role":role,"tags":null}]})
}
async fn introspection(server: &MockServer, body: Value) {
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/oauth/introspect"))
        .and(header_exists("authorization"))
        .and(body_string_contains("token=oat_fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}
#[tokio::test]
async fn sdk_live_membership_disclosure_revocation_and_plane_checks() {
    let server = MockServer::start().await;
    let iam = adapter(&server);
    introspection(&server, snapshot(Some("org_admin"))).await;
    assert!(
        iam.authenticate("oat_fixture", None)
            .await
            .unwrap()
            .admin("tos")
    );
    introspection(&server, snapshot(None)).await;
    let actor = iam.authenticate("oat_fixture", None).await.unwrap();
    assert!(actor.member("tos"));
    assert!(!actor.admin("tos"));
    assert!(!actor.validator);
    let mut wrong = snapshot(Some("org_admin"));
    wrong["client_id"] = json!("other>app");
    introspection(&server, wrong).await;
    assert!(iam.authenticate("oat_fixture", None).await.is_err());
    let mut testing = snapshot(Some("org_admin"));
    testing["authorizations"][0]["testing_environment_id"] =
        json!("44444444-4444-4444-4444-444444444444");
    introspection(&server, testing).await;
    assert!(iam.authenticate("oat_fixture", None).await.is_err());
    introspection(&server, json!({"active":false})).await;
    assert!(iam.authenticate("oat_fixture", None).await.is_err());
}
#[tokio::test]
async fn sdk_uses_slt_exchange_refresh_and_revocation_contracts() {
    let server = MockServer::start().await;
    let iam = adapter(&server);
    let response = json!({"access_token":"oat_fixture","refresh_token":"ort_fixture","token_type":"Bearer","expires_in":3600,"scope":"self.identity.read self.membership.read"});
    Mock::given(method("POST"))
        .and(path("/api/v1/app-auth/tokens"))
        .and(body_string_contains("slt=slt_fixture"))
        .and(header_exists("idempotency-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        iam.login("slt_fixture", "sdk-login-fixture-0001", None)
            .await
            .unwrap()["access_token"],
        "oat_fixture"
    );
    Mock::given(method("POST"))
        .and(path("/api/v1/app-auth/tokens"))
        .and(body_string_contains("refresh_token=ort_fixture"))
        .and(header_exists("idempotency-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        iam.refresh("ort_fixture", "sdk-refresh-fixture-0001", None)
            .await
            .unwrap()["refresh_token"],
        "ort_fixture"
    );
    Mock::given(method("POST"))
        .and(path("/api/v1/oauth/revoke"))
        .and(body_string_contains("token=ort_fixture"))
        .and(header_exists("idempotency-key"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    iam.revoke("ort_fixture", "sdk-revoke-fixture-0001", None)
        .await
        .unwrap();
}
