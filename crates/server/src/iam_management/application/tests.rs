use super::*;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};
const APP: &str = "alpha>app";
const SOURCE: &str = APP;
const ENV: &str = "00000000-0000-4000-8000-000000000002";
fn authorization() -> String {
    format!("Basic {}", STANDARD.encode("alpha>app:ask_fixture_secret"))
}
fn identity() -> Value {
    json!({"application_id":SOURCE,"app_id":APP,"organization_id":"00000000-0000-4000-8000-000000000003","org_id":"alpha","iam_revision":7})
}
async fn setup(server: &MockServer) -> IamManagement {
    IamManagement::new(
        &server.uri(),
        "hck_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        crate::database("sqlite::memory:").await.unwrap(),
        [7; 32],
    )
    .unwrap()
}
async fn seed(adapter: &IamManagement) {
    let key = "K".repeat(32);
    sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,state,generation,key_version,created_at,last_activity) VALUES(?,'alpha','fixture','Testing','',?,?,'ready',2,3,1,1)").bind(ENV).bind(crate::encrypt(&[7;32],&key).unwrap()).bind(hex::encode(Sha256::digest(&key))).execute(&adapter.db).await.unwrap();
}
#[test]
fn basic_application_parser_rejects_test_or_malformed_credentials_without_echoing_them() {
    for auth in [
        "Bearer secret".to_owned(),
        "Basic !!!!!secret".into(),
        format!("Basic {}", STANDARD.encode("alpha>app:test_secret")),
        format!("Basic {}", STANDARD.encode("alpha>app:ask_\nsecret")),
    ] {
        let error = credentials(&auth).err().unwrap();
        assert!(!error.1.message.contains("secret"));
    }
    assert_eq!(credentials(&authorization()).unwrap().0, APP);
}
#[tokio::test]
async fn production_identity_uses_official_protected_credentials_and_rejects_wrong_app() {
    let server = MockServer::start().await;
    let adapter = setup(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/honeycomb/application-identity"))
        .and(header(
            "authorization",
            "Bearer hck_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ))
        .and(header(
            "x-honeycomb-application-authorization",
            authorization(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity()))
        .expect(1)
        .mount(&server)
        .await;
    let found = adapter
        .testing_verify_application(&authorization())
        .await
        .unwrap();
    assert_eq!(found.application_id, SOURCE);
    server.reset().await;
    let mut wrong = identity();
    wrong["app_id"] = json!("other>app");
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(wrong))
        .mount(&server)
        .await;
    assert!(
        adapter
            .testing_verify_application(&authorization())
            .await
            .is_err()
    );
    server.reset().await;
    let mut legacy = identity();
    legacy["application_id"] = json!("00000000-0000-4000-8000-000000000001");
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(legacy))
        .mount(&server)
        .await;
    assert!(
        adapter
            .testing_verify_application(&authorization())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn linked_environment_pagination_rejects_duplicates_and_nonadvancing_cursors() {
    let server = MockServer::start().await;
    let adapter = setup(&server).await;
    let second = "00000000-0000-4000-8000-000000000004";
    Mock::given(method("GET"))
        .and(path("/api/v1/honeycomb/testing-environments"))
        .and(query_param("limit", "100"))
        .and(|r: &wiremock::Request| !r.url.query_pairs().any(|(k, _)| k == "cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"items":[{"environment_id":ENV}],"page":{"has_more":true,"next_cursor":ENV}}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(query_param("cursor",ENV)).respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"environment_id":second}],"page":{"has_more":false,"next_cursor":null}}))).expect(1).mount(&server).await;
    assert_eq!(
        adapter
            .testing_environment_ids(&authorization())
            .await
            .unwrap(),
        vec![ENV, second]
    );
    server.reset().await;
    for bad in [
        json!({"items":[{"environment_id":ENV},{"environment_id":ENV}],"page":{"has_more":false,"next_cursor":null}}),
        json!({"items":[{"environment_id":ENV}],"page":{"has_more":true,"next_cursor":second}}),
    ] {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(bad))
            .mount(&server)
            .await;
        assert!(
            adapter
                .testing_environment_ids(&authorization())
                .await
                .is_err()
        );
        server.reset().await;
    }
}
#[tokio::test]
async fn recovery_binds_current_identity_environment_and_versions_without_persisting_secret() {
    let server = MockServer::start().await;
    let adapter = setup(&server).await;
    seed(&adapter).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/honeycomb/application-identity"))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity()))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path(format!("/api/v1/honeycomb/testing-environments/{ENV}"))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"environment_id":ENV,"state":"active","generation":2,"key_version":3,"iam_revision":8}))).expect(1).mount(&server).await;
    Mock::given(method("POST")).and(path(format!("/api/v1/honeycomb/testing-environments/{ENV}/applications/alpha%3Eapp/credential-recovery"))).and(header("x-honeycomb-application-authorization",authorization())).and(header("x-honeycomb-testing-key","K".repeat(32))).respond_with(|r:&wiremock::Request| {
        let body:Value=serde_json::from_slice(&r.body).unwrap();
        assert_eq!(body["generation"],2);assert_eq!(body["key_version"],3);assert_eq!(body["expected_environment_revision"],8);
        ResponseTemplate::new(200).set_body_json(json!({"operation_id":body["operation_id"],"state":"accepted","environment_id":ENV,"app_id":APP,"application_id":"00000000-0000-4000-8000-000000000005","iam_revision":3,"configuration_revision":1,"app_secret":"test_secret_fixture","credential_version":2}))
    }).expect(1).mount(&server).await;
    let response = adapter
        .testing_recover_credential(ENV, &authorization())
        .await;
    let response = response.unwrap();
    assert_eq!(response["app_secret"], "test_secret_fixture");
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM management_requests")
        .fetch_one(&adapter.db)
        .await
        .unwrap();
    assert_eq!(stored, 0);
}
#[tokio::test]
async fn snapshot_keeps_test_scope_and_public_visibility_without_production_fallback() {
    let server = MockServer::start().await;
    let adapter = setup(&server).await;
    seed(&adapter).await;
    let config=json!({"name":"Test","description":"test","org_id":"alpha","local_app_id":"app","app_scope":{"iam":["self.identity.read","self.email.read"],"external":[]}}).to_string();
    sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,config,effective_config,webhook_secret,created_at,updated_at) VALUES(? ,?,'alpha','Test','test',?,?,'',1,1)").bind(ENV).bind(APP).bind(&config).bind(&config).execute(&adapter.db).await.unwrap();
    Mock::given(method("GET")).and(path(format!("/api/v1/honeycomb/testing-environments/{ENV}"))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"environment_id":ENV,"state":"active","generation":2,"key_version":3,"iam_revision":8}))).expect(1).mount(&server).await;
    Mock::given(method("GET")).and(path(format!("/api/v1/honeycomb/testing-environments/{ENV}/applications/alpha%3Eapp"))).and(query_param("generation","2")).and(query_param("key_version","3")).and(query_param("expected_environment_revision","8")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"app_id":APP,"org_id":"alpha","configuration_revision":1,"iam_revision":3,"app_scope":{"iam":["self.identity.read","self.email.read"],"external":[]},"effective_scopes":[{"scope":"self.identity.read"}],"visibility":"public","availability":"verified","ready":true,"credential_version":1}))).expect(1).mount(&server).await;
    let snapshot = adapter.testing_snapshot(APP, &"K".repeat(32)).await;
    let snapshot = snapshot.unwrap();
    assert_eq!(snapshot["visibility"], "public");
    assert_eq!(
        snapshot["effective_configuration"]["app_scope"]["iam"],
        json!(["self.identity.read"])
    );
    assert!(
        adapter
            .testing_snapshot(APP, &"Z".repeat(32))
            .await
            .is_err()
    );
}

async fn operation(adapter: &IamManagement, id: Uuid, kind: &str) {
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,?,'fixture',?,? ,?,'fixture',1,1)").bind(id.to_string()).bind(ENV).bind(id.to_string()).bind(kind).bind(APP).execute(&adapter.db).await.unwrap();
}
async fn environment_mock(server: &MockServer) {
    Mock::given(method("GET")).and(path(format!("/api/v1/honeycomb/testing-environments/{ENV}"))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"environment_id":ENV,"state":"active","generation":2,"key_version":3,"iam_revision":8}))).mount(server).await;
}
#[tokio::test]
async fn isolated_rotation_replays_same_request_and_stops_after_generation_change() {
    let server = MockServer::start().await;
    let adapter = setup(&server).await;
    seed(&adapter).await;
    environment_mock(&server).await;
    let id = Uuid::new_v4();
    operation(&adapter, id, "secret.rotate").await;
    Mock::given(method("POST")).and(path(format!("/api/v1/honeycomb/testing-environments/{ENV}/applications/alpha%3Eapp/secret-rotations"))).and(header("x-honeycomb-testing-key","K".repeat(32))).respond_with(move |r:&wiremock::Request|{
        assert!(!r.headers.contains_key("x-honeycomb-actor-token"));
        assert!(!r.headers.contains_key("x-honeycomb-application-authorization"));
        assert_eq!(r.headers.get("x-honeycomb-testing-key").unwrap(), "K".repeat(32).as_str());
        let body:Value=serde_json::from_slice(&r.body).unwrap();assert_eq!(body["operation_id"],id.to_string());assert_eq!(body["generation"],2);
        ResponseTemplate::new(200).set_body_json(json!({"operation_id":id,"state":"accepted","environment_id":ENV,"app_id":APP,"configuration_revision":1,"iam_revision":4,"credential_version":2,"app_secret":"fixture-rotated-secret"}))
    }).expect(2).mount(&server).await;
    let input = json!({"operation_id":id,"app_id":APP,"configuration_revision":1,"expected_iam_revision":3});
    assert!(
        adapter
            .testing_rotate_secret(&input, "oat_fixture_actor", None, &"Z".repeat(32))
            .await
            .is_err()
    );
    let first = adapter
        .testing_rotate_secret(&input, "oat_fixture_actor", None, &"K".repeat(32))
        .await
        .unwrap();
    assert_eq!(first["state"], "accepted");
    let replay = adapter
        .testing_operation_result(&id.to_string(), "oat_fixture_actor", &"K".repeat(32))
        .await
        .unwrap();
    assert_eq!(first, replay);
    let receipts: Vec<String> =
        sqlx::query_scalar("SELECT receipt FROM iam_testing_phases WHERE receipt IS NOT NULL")
            .fetch_all(&adapter.db)
            .await
            .unwrap();
    assert!(
        receipts
            .iter()
            .all(|r| !r.contains("fixture-rotated-secret"))
    );
    sqlx::query("UPDATE environments SET generation=3 WHERE id=?")
        .bind(ENV)
        .execute(&adapter.db)
        .await
        .unwrap();
    assert!(
        adapter
            .testing_operation_result(&id.to_string(), "oat_fixture_actor", &"K".repeat(32))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn configured_app_waits_for_participant_and_reserves_revision_only_once() {
    let server = MockServer::start().await;
    let adapter = setup(&server).await;
    seed(&adapter).await;
    environment_mock(&server).await;
    let id = Uuid::new_v4();
    operation(&adapter, id, "configure").await;
    let config = json!({"org_id":"alpha","local_app_id":"app","name":"Application","description":"Test application","base_url":"https://app.invalid","webhook_url":"https://app.invalid/webhook","webhook_scope":["full"],"app_scope":{"iam":["self.identity.read"],"external":[]},"obo_endpoints":[],"obo_review_message":"","testing_idle_days":30});
    sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,config,webhook_secret,created_at,updated_at) VALUES(?,?,'alpha','Application','Test',?,'',1,1)").bind(ENV).bind(APP).bind(config.to_string()).execute(&adapter.db).await.unwrap();
    Mock::given(method("PUT")).and(path(format!("/api/v1/honeycomb/testing-environments/{ENV}/applications/alpha%3Eapp/configuration"))).respond_with(move |r:&wiremock::Request| {
        assert!(!r.headers.contains_key("x-honeycomb-actor-token"));
        assert!(!r.headers.contains_key("x-honeycomb-application-authorization"));
        assert_eq!(r.headers.get("x-honeycomb-testing-key").unwrap(), "K".repeat(32).as_str());
        let body:Value=serde_json::from_slice(&r.body).unwrap();assert_eq!(body["operation_id"],id.to_string());
        ResponseTemplate::new(200).set_body_json(json!({"operation_id":id,"state":"accepted","environment_id":ENV,"app_id":APP,"configuration_revision":1,"iam_revision":4,"ready":false,"app_secret":"fixture-config-secret","effective_configuration":{"app_id":APP,"org_id":"alpha","configuration_revision":1,"app_scope":{"iam":["self.identity.read"],"external":[]},"effective_scopes":[{"scope":"self.identity.read"}],"visibility":"private"}}))
    }).expect(2).mount(&server).await;
    let input = json!({"operation_id":id,"app_id":APP,"configuration_revision":1,"expected_iam_revision":0,"visibility":"private","configuration":config,"webhook_secret":"fixture-webhook-secret-long-enough"});
    assert!(
        adapter
            .testing_configure(&input, "oat_fixture_actor", &"K".repeat(32))
            .await
            .is_err()
    );
    assert!(
        adapter
            .testing_operation_result(&id.to_string(), "oat_fixture_actor", &"K".repeat(32))
            .await
            .is_err()
    );
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM environments WHERE id=?")
        .bind(ENV)
        .fetch_one(&adapter.db)
        .await
        .unwrap();
    assert_eq!(revision, 2);
    let phases: Vec<String> =
        sqlx::query_scalar("SELECT phase FROM iam_testing_phases ORDER BY phase")
            .fetch_all(&adapter.db)
            .await
            .unwrap();
    assert_eq!(
        phases,
        vec!["application-configuration", "application-participant"]
    );
    let receipts: Vec<String> =
        sqlx::query_scalar("SELECT receipt FROM iam_testing_phases WHERE receipt IS NOT NULL")
            .fetch_all(&adapter.db)
            .await
            .unwrap();
    assert!(
        receipts
            .iter()
            .all(|r| !r.contains("fixture-config-secret"))
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| !r.url.path().ends_with("/operations"))
    );
}

#[tokio::test]
async fn participant_receipt_allows_only_exact_application_activation() {
    let server = MockServer::start().await;
    let adapter = setup(&server).await;
    seed(&adapter).await;
    environment_mock(&server).await;
    let id = Uuid::new_v4();
    operation(&adapter, id, "configure").await;
    let op = json!({"operation_id":id,"environment_id":ENV,"app_id":APP,"org_id":"alpha","action":"import","environment_revision":2,"generation":2,"key_version":3,"snapshot":{"imports":[{"app_id":APP}]}});
    sqlx::query("UPDATE environments SET revision=2 WHERE id=?")
        .bind(ENV)
        .execute(&adapter.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO iam_testing_phases(parent_operation,phase,operation_id,environment_id,encrypted_body,receipt,created_at) VALUES(?,'application-participant',?,?,?,'{\"state\":\"completed\"}',1)")
        .bind(id.to_string()).bind(Uuid::new_v4().to_string()).bind(ENV).bind(crate::encrypt(&[7;32],&op.to_string()).unwrap()).execute(&adapter.db).await.unwrap();
    Mock::given(method("POST")).and(path(format!("/api/v1/honeycomb/testing-environments/{ENV}/operations"))).respond_with(|r:&wiremock::Request|{
        let body:Value=serde_json::from_slice(&r.body).unwrap();assert_eq!(body["operation"],"activate-apps");assert_eq!(body["app_ids"],json!([APP]));
        assert!(!r.headers.contains_key("x-honeycomb-actor-token"));assert!(!r.headers.contains_key("x-honeycomb-application-authorization"));
        ResponseTemplate::new(200).set_body_json(json!({"operation_id":body["operation_id"],"environment_id":ENV,"state":"accepted","iam_revision":9,"iam_completion":true,"environment":{"environment_id":ENV,"state":"active","generation":2,"key_version":3,"iam_revision":9}}))
    }).expect(1).mount(&server).await;
    let input = models::HoneycombTestingAppMutation {
        operation_id: id,
        environment_id: Uuid::parse_str(ENV).unwrap(),
        generation: 2,
        key_version: 3,
        expected_environment_revision: 8,
        expected_iam_revision: 0,
        configuration_revision: 1,
        configuration: None,
    };
    adapter
        .testing_configure_participant(APP, &input, &json!({}))
        .await
        .unwrap();
    adapter
        .testing_configure_participant(APP, &input, &json!({}))
        .await
        .unwrap();
}
