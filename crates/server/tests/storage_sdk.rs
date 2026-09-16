//! Real Briefcase adapter and official IAM SDK against explicit HTTP contract doubles.
use serde_json::{Value, json};
use silicon_honeycomb_server::{integration::ArchiveStorage, storage::Briefcase};
use silicon_iam_client::{Client, Credential};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, method, path},
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

async fn archive_read_and_publish(testing: bool) {
    let iam = MockServer::start().await;
    let storage = MockServer::start().await;
    let environment = testing.then_some("12345678901234567890123456789012");
    let test_id = "12345678-1234-1234-1234-123456789abc";
    let endpoints = [
        ("briefcase.files.read", "/api/v1/obo/files/read"),
        ("briefcase.link_access.update", "/api/v1/obo/link-access"),
    ];
    Mock::given(method("GET"))
        .and(path("/api/v1/obo-access/applications/tos%3Ebriefcase/endpoints"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"application":{"app_id":"tos>briefcase","org_id":"tos"},"endpoints":endpoints.iter().map(|(id,path)|json!({"endpoint_id":id,"path":path,"metadata":{},"critical":false})).collect::<Vec<_>>()})))
        .expect(2).mount(&iam).await;
    let mut proof = json!({"access_proof":"fixture-obo-proof","proof_id":"11111111-1111-1111-1111-111111111111","expires_in":60,"expires_at":"2030-01-01T00:00:00Z"});
    if testing {
        proof["testing_context"] = json!({"app_id":"tos>briefcase","app_secret":"fixture-test-secret","iam_test_key":environment});
    }
    // A multi-organization subject needs the resource organization, not the
    // provider/caller application's owning organization or an omitted choice.
    Mock::given(method("POST"))
        .and(path("/api/v1/obo-access/exchanges"))
        .and(body_partial_json(
            json!({"org_id":"acme","subject_token":"oat_multi_org_actor"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(proof))
        .expect(2)
        .mount(&iam)
        .await;
    Mock::given(method("POST"))
        .and(path(endpoints[0].1))
        .and(body_partial_json(
            json!({"entry_id":"existing-entry","download":true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"archive bytes"))
        .expect(1)
        .mount(&storage)
        .await;
    let mut share =
        "https://briefcase.example/org/acme/apps/tos%3Ehoneycomb/public/app.tar.gz".to_string();
    if testing {
        share.push_str(&format!("?test_environment={test_id}"));
    }
    Mock::given(method("POST"))
        .and(path(endpoints[1].1))
        .and(body_partial_json(
            json!({"entry_id":"existing-entry","enabled":true}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"effective":true,"url":share})),
        )
        .expect(1)
        .mount(&storage)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/public/acme/apps/tos%3Ehoneycomb/public/app.tar.gz",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"archive bytes"))
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
    // Existing private releases store only the entry ID; they must also work.
    assert_eq!(
        adapter
            .read(
                "existing-entry",
                "acme",
                Some("oat_multi_org_actor"),
                environment
            )
            .await
            .unwrap(),
        b"archive bytes"
    );
    let reference = adapter
        .publish(
            "existing-entry",
            "acme",
            "oat_multi_org_actor",
            environment,
            "98765432-1234-1234-1234-123456789abc",
        )
        .await
        .unwrap();
    assert_eq!(
        adapter
            .read(&reference, "acme", None, environment)
            .await
            .unwrap(),
        b"archive bytes"
    );
    let downstream = storage.received_requests().await.unwrap();
    let exchanges = iam.received_requests().await.unwrap();
    for (exchange, request) in exchanges
        .iter()
        .filter(|r| r.method == "POST")
        .zip(downstream.iter().filter(|r| r.method == "POST"))
    {
        let body: Value = exchange.body_json().unwrap();
        assert_eq!(
            body["request"]["body_sha256"],
            silicon_iam_client::api::obo::body_sha256(&request.body)
        );
        assert_eq!(
            exchange
                .headers
                .get("x-testing-environment-key")
                .map(|v| v.to_str().unwrap()),
            environment
        );
        assert_eq!(
            request
                .headers
                .get("x-briefcase-app-secret")
                .map(|v| v.to_str().unwrap()),
            testing.then_some("fixture-test-secret")
        );
    }
    let public = downstream.iter().find(|r| r.method == "GET").unwrap();
    assert!(!public.headers.contains_key("x-iam-obo-access-proof"));
    assert!(!public.headers.contains_key("x-briefcase-app-secret"));
    assert_eq!(
        public
            .url
            .query_pairs()
            .find(|(k, _)| k == "test_environment")
            .map(|(_, v)| v.into_owned()),
        testing.then(|| test_id.to_owned())
    );
}

#[tokio::test]
async fn archive_read_and_publication_select_the_resource_organization() {
    archive_read_and_publish(false).await;
}

#[tokio::test]
async fn archive_read_and_publication_preserve_the_test_plane() {
    archive_read_and_publish(true).await;
}

async fn exchange_failure(
    status: u16,
    code: &str,
    request_id: &str,
) -> silicon_honeycomb_server::error::Error {
    let iam = MockServer::start().await;
    let storage = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/obo-access/applications/vendor%3Estorage/endpoints"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"application":{"app_id":"vendor>storage","org_id":"vendor"},"endpoints":[{"endpoint_id":"briefcase.files.read","path":"/api/v1/obo/files/read","metadata":{},"critical":false}]})))
        .mount(&iam).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/obo-access/exchanges"))
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({"error":{"code":code,"message":"SECRET_UPSTREAM_MESSAGE","details":{"subject_token":"SECRET_UPSTREAM_DETAILS"},"request_id":request_id}})))
        .mount(&iam).await;
    let adapter = Briefcase {
        iam: Client::builder(&iam.uri())
            .unwrap()
            .telemetry(false)
            .credential(Credential::application(
                "vendor>catalog",
                "SECRET_APP_CREDENTIAL",
            ))
            .build()
            .unwrap(),
        http: reqwest::Client::new(),
        base_url: storage.uri(),
        app_id: "vendor>catalog".into(),
        audience: "vendor>storage".into(),
    };
    let error = adapter
        .read("entry", "vendor", Some("SECRET_SUBJECT_TOKEN"), None)
        .await
        .unwrap_err();
    assert!(storage.received_requests().await.unwrap().is_empty());
    let exposed = serde_json::to_string(&error.1).unwrap();
    assert!(
        !exposed.contains("SECRET_"),
        "upstream data escaped: {exposed}"
    );
    error
}

#[tokio::test]
async fn iam_server_failure_keeps_safe_diagnostics_without_consent_advice() {
    let id = "01a0a904-aee3-75c3-b60c-ad97b466c388";
    let error = exchange_failure(500, "internal_error", id).await;
    assert_eq!(error.0, axum::http::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error.1.code, "integration_unavailable");
    assert!(!error.1.message.contains("consent"));
    assert!(!error.1.message.contains("scopes"));
    assert!(
        error
            .1
            .details
            .contains(&"upstream_code=internal_error".to_owned())
    );
    assert!(
        error
            .1
            .details
            .contains(&format!("upstream_request_id={id}"))
    );
}

#[tokio::test]
async fn iam_denial_retains_authorization_recovery_guidance() {
    let error = exchange_failure(
        403,
        "obo_authority_revoked",
        "01a0a904-aee3-75c3-b60c-ad97b466c388",
    )
    .await;
    assert_eq!(error.0, axum::http::StatusCode::FORBIDDEN);
    assert_eq!(error.1.code, "storage_authorization_required");
    assert!(error.1.message.contains("consent"));
    assert!(error.1.message.contains("same operation"));
}

#[tokio::test]
async fn iam_unknown_codes_and_malformed_correlation_ids_are_not_exposed() {
    let error = exchange_failure(500, "SECRET_UNKNOWN_CODE", "SECRET_REQUEST_ID").await;
    assert_eq!(error.1.details, vec!["upstream_status=500"]);
}
