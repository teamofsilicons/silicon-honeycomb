//! Real Briefcase adapter and official IAM SDK against explicit HTTP contract doubles.
use serde_json::{Value, json};
use silicon_honeycomb_server::{integration::ArchiveStorage, storage::Briefcase};
use silicon_iam_client::{Client, Credential};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

async fn logo_roundtrip(testing: bool) {
    let iam = MockServer::start().await;
    let storage = MockServer::start().await;
    let environment = testing.then_some("12345678901234567890123456789012");
    let test_id = "12345678-1234-1234-1234-123456789abc";
    let op = "98765432-1234-1234-1234-123456789abc";
    let endpoints = [
        ("briefcase.uploads.reserve", "/api/v1/obo/uploads/reserve"),
        ("briefcase.uploads.commit", "/api/v1/obo/uploads/commit"),
        ("briefcase.link_access.update", "/api/v1/obo/link-access"),
    ];
    Mock::given(method("GET"))
        .and(path("/api/v1/obo-access/applications/tos%3Ebriefcase/endpoints"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"application":{"app_id":"tos>briefcase","org_id":"tos"},"endpoints":endpoints.iter().map(|(id,path)|json!({"endpoint_id":id,"path":path,"metadata":{},"critical":false})).collect::<Vec<_>>()})))
        .expect(3).mount(&iam).await;
    let mut proof = json!({"access_proof":"fixture-obo-proof","proof_id":"11111111-1111-1111-1111-111111111111","expires_in":60,"expires_at":"2030-01-01T00:00:00Z"});
    if testing {
        proof["testing_context"] = json!({"app_id":"tos>briefcase","app_secret":"fixture-test-secret","iam_test_key":environment});
    }
    Mock::given(method("POST"))
        .and(path("/api/v1/obo-access/exchanges"))
        .respond_with(ResponseTemplate::new(200).set_body_json(proof))
        .expect(3)
        .mount(&iam)
        .await;
    Mock::given(method("POST")).and(path(endpoints[0].1))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"state":"reserved","upload_id":"upload-id","capability":"private-transfer-capability"}))).expect(1).mount(&storage).await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/obo/uploads/upload-id/content"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&storage)
        .await;
    Mock::given(method("POST"))
        .and(path(endpoints[1].1))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"published_entry_id":"entry-id","state":"committed"})),
        )
        .expect(1)
        .mount(&storage)
        .await;
    let mut share =
        format!("https://briefcase.example.com/org/acme/apps/tos%3Ehoneycomb/public/logo-{op}.png");
    if testing {
        share.push_str(&format!("?test_environment={test_id}"));
    }
    Mock::given(method("POST"))
        .and(path(endpoints[2].1))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"effective":true,"url":share})),
        )
        .expect(1)
        .mount(&storage)
        .await;
    let adapter = Briefcase {
        iam: Client::builder(&iam.uri())
            .unwrap()
            .telemetry(false)
            .credential(Credential::application("tos>honeycomb", "fixture-secret"))
            .build()
            .unwrap(),
        http: reqwest::Client::new(),
        base_url: storage.uri(),
        app_id: "tos>honeycomb".into(),
        audience: "tos>briefcase".into(),
    };
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("logo.png");
    let bytes = include_bytes!("../../../web/tests/fixtures/logo.png");
    std::fs::write(&file, bytes).unwrap();
    let url = adapter
        .upload_logo("acme", &file, "oat_actor", environment, op)
        .await
        .unwrap();
    let url = url::Url::parse(&url).unwrap();
    assert!(url.as_str().starts_with(&storage.uri()));
    assert_eq!(
        url.query_pairs().find(|(k, _)| k == "view").unwrap().1,
        "inline"
    );
    assert_eq!(
        url.query_pairs()
            .find(|(k, _)| k == "test_environment")
            .map(|(_, v)| v.into_owned()),
        testing.then(|| test_id.to_string())
    );
    let requests = storage.received_requests().await.unwrap();
    for request in &requests {
        assert_eq!(
            request
                .headers
                .get("x-briefcase-app-secret")
                .map(|v| v.to_str().unwrap()),
            testing.then_some("fixture-test-secret")
        );
        if request.method == "POST" {
            assert_eq!(
                request.headers["x-iam-obo-access-proof"],
                "fixture-obo-proof"
            );
        }
    }
    let reserve: Value = requests[0].body_json().unwrap();
    assert_eq!(reserve["content_type"], "image/png");
    assert_eq!(reserve["name"], format!("logo-{op}.png"));
    assert_eq!(reserve["parent_path"], "apps/tos>honeycomb/public");
    assert_eq!(reserve["operation_id"], op);
    assert_eq!(requests[1].headers["x-org-id"], "acme");
    assert_eq!(requests[1].body, bytes);
    assert_eq!(
        requests[1].headers["x-briefcase-upload-capability"],
        "private-transfer-capability"
    );
    assert_eq!(
        requests[2].body_json::<Value>().unwrap()["operation_id"],
        op
    );
    let iam_requests = iam.received_requests().await.unwrap();
    for request in &iam_requests {
        assert_eq!(
            request
                .headers
                .get("x-testing-environment-key")
                .map(|v| v.to_str().unwrap()),
            environment
        );
    }
    let exchanges: Vec<Value> = iam_requests
        .iter()
        .filter(|r| r.method == "POST")
        .map(|r| r.body_json().unwrap())
        .collect();
    for (exchange, downstream) in exchanges
        .iter()
        .zip(requests.iter().filter(|r| r.method == "POST"))
    {
        assert_eq!(exchange["org_id"], "acme");
        assert_eq!(exchange["subject_token"], "oat_actor");
        assert_eq!(
            exchange["request"]["body_sha256"],
            silicon_iam_client::api::obo::body_sha256(&downstream.body)
        );
    }
}

#[tokio::test]
async fn logo_obo_upload_binds_org_and_exact_content_before_public_sharing() {
    logo_roundtrip(false).await;
}
#[tokio::test]
async fn logo_obo_upload_preserves_test_authentication_and_public_link_plane() {
    logo_roundtrip(true).await;
}
