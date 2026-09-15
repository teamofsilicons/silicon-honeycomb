//! Wire checks against the exact locally pinned official IAM 1.10 SDK.
use serde_json::{Value, json};
use silicon_honeycomb_server::{iam_management::IamManagement, integration::Management};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

async fn setup(server: &MockServer) -> (IamManagement, sqlx::SqlitePool, Value, String) {
    let db = silicon_honeycomb_server::database("sqlite::memory:")
        .await
        .unwrap();
    let config = json!({"org_id":"tos","local_app_id":"sdk-test","name":"SDK test","description":"Catalog-owned description","webhook_url":"https://example.com/webhook/","webhook_scope":["membership"],"app_scope":{"iam":["self.identity.read","directory.carbons.read"],"external":[]},"obo_endpoints":[],"testing_idle_days":30});
    sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,config,webhook_secret,created_at,updated_at) VALUES('production','tos>sdk-test','tos','SDK test','Catalog-owned description',?,'unused',1,1)")
        .bind(config.to_string()).execute(&db).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,'production','actor',?,'configure','tos>sdk-test','test',1,1)")
        .bind(&id).bind(&id).execute(&db).await.unwrap();
    let adapter = IamManagement::new(
        &server.uri(),
        format!("hck_{}", "a".repeat(43)),
        db.clone(),
        [7; 32],
    )
    .unwrap();
    (adapter, db, config, id)
}
fn record() -> Value {
    json!({"app_id":"tos>sdk-test","org_id":"tos","app_name":"Accepted name","app_logo":null,"base_url":null,"configuration_revision":1,"iam_revision":2,"visibility":"private","availability":"verified","app_scope":{"iam":["self.identity.read","directory.carbons.read"],"external":[]},"effective_scopes":[{"scope":"self.identity.read","basis":"policy"}],"obo_endpoints":[],"webhook_scope":["membership"],"testing_idle_days":30})
}
#[tokio::test]
async fn configuration_uses_service_and_actor_authority_and_replays_identical_encrypted_body() {
    let server = MockServer::start().await;
    let (adapter, db, config, id) = setup(&server).await;
    Mock::given(method("PUT")).and(path("/api/v1/honeycomb/applications/tos%3Esdk-test/configuration"))
        .and(header("authorization",format!("Bearer hck_{}","a".repeat(43))))
        .and(header("x-honeycomb-actor-token","oat_actor")).and(header("idempotency-key",id.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"operation_id":id,"state":"accepted","configuration_revision":1,"iam_revision":2,"effective_configuration":record(),"app_secret":"one-time-result"})))
        .expect(2).mount(&server).await;
    let mut operation = json!({"operation_id":id,"app_id":"tos>sdk-test","configuration_revision":1,"expected_iam_revision":0,"configuration":config,"webhook_secret":"webhook-secret-only-inside-encryption","visibility":"private"});
    let result = adapter
        .configure(&operation, "oat_actor", None)
        .await
        .unwrap();
    assert_eq!(result["effective_configuration"]["name"], "Accepted name");
    assert_eq!(
        result["effective_configuration"]["app_scope"]["iam"],
        json!(["self.identity.read"])
    );
    assert!(
        result["effective_configuration"]
            .get("webhook_url")
            .is_none()
    );
    assert_eq!(result["app_secret"], "one-time-result");
    operation["expected_iam_revision"] = json!(99);
    adapter
        .configure(&operation, "oat_actor", None)
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests[0].body, requests[1].body);
    let wire: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(wire["expected_iam_revision"], 0);
    assert_eq!(wire["publication_approved"], false);
    assert!(wire.get("description").is_none());
    let stored: String = sqlx::query_scalar("SELECT encrypted_body FROM management_requests")
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(!stored.contains("webhook-secret"));
    assert!(!stored.contains("one-time-result"));
    assert!(
        adapter
            .configure(&operation, "oat_actor", Some("test-key"))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn rotation_passes_transient_step_up_and_recovers_by_exact_mutation_replay() {
    let server = MockServer::start().await;
    let (adapter, db, _, _) = setup(&server).await;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,'production','actor',?,'secret.rotate','tos>sdk-test','test',1,1)").bind(&id).bind(&id).execute(&db).await.unwrap();
    Mock::given(method("POST")).and(path("/api/v1/honeycomb/applications/tos%3Esdk-test/secret-rotations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"operation_id":id,"state":"accepted","iam_revision":3,"app_id":"tos>sdk-test","credential_version":2,"app_secret":"replacement-secret"}))).expect(2).mount(&server).await;
    let operation = json!({"operation_id":id,"app_id":"tos>sdk-test","configuration_revision":1,"expected_iam_revision":2});
    let rotated = adapter
        .rotate_secret(&operation, "oat_actor", Some("step-up-proof"), None)
        .await
        .unwrap();
    assert_eq!(rotated["configuration_revision"], 1);
    assert_eq!(rotated["app_secret"], "replacement-secret");
    let recovered = adapter
        .operation_result(&id, "oat_actor", None)
        .await
        .unwrap();
    assert_eq!(recovered["app_secret"], "replacement-secret");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests[0].headers["x-step-up-token"], "step-up-proof");
    assert!(!requests[1].headers.contains_key("x-step-up-token"));
    assert_eq!(requests[0].body, requests[1].body);
    let stored: String =
        sqlx::query_scalar("SELECT encrypted_body FROM management_requests WHERE operation_id=?")
            .bind(&id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(!stored.contains("replacement-secret"));
    assert!(!stored.contains("step-up-proof"));
}
#[tokio::test]
async fn snapshot_reads_effective_scopes_and_maps_iam_availability() {
    let server = MockServer::start().await;
    let (adapter, db, config, _) = setup(&server).await;
    let mut desired = config.clone();
    desired["description"] = json!("Unaccepted new description");
    sqlx::query("UPDATE applications SET revision=2,config=?,effective_config=?")
        .bind(desired.to_string())
        .bind(config.to_string())
        .execute(&db)
        .await
        .unwrap();
    Mock::given(method("GET"))
        .and(path("/api/v1/honeycomb/applications/tos%3Esdk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(record()))
        .mount(&server)
        .await;
    let current = adapter
        .application_snapshot("tos>sdk-test", None)
        .await
        .unwrap();
    assert_eq!(current["availability"], "active");
    assert_eq!(
        current["effective_configuration"]["description"],
        "Catalog-owned description"
    );
    assert_eq!(
        current["effective_configuration"]["app_scope"]["iam"],
        json!(["self.identity.read"])
    );
}
