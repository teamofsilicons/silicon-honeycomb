use serde_json::json;
use silicon_honeycomb_client::Client;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn personal_workspace_keeps_actor_authority_and_partial_failures_with_scoped_compatibility() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        for path in [
            "/api/v1/ata-verifications",
            "/api/v1/apps/ting/ata-verifications",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                if request.windows(4).any(|part| part == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            assert!(
                request
                    .to_lowercase()
                    .contains("authorization: bearer current-actor\r\n")
            );
            assert!(
                !request
                    .to_lowercase()
                    .contains("x-testing-environment-key:")
            );
            let body = json!({"items":[],"failures":[{"app_id":"unavailable","message":"Retry this application"}]}).to_string();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    let client = Client::new(&origin)
        .unwrap()
        .with_token("current-actor")
        .with_telemetry(false);
    let personal = client.my_ata_verifications().await.unwrap();
    assert_eq!(personal["failures"][0]["app_id"], "unavailable");
    client.ata_verifications("ting").await.unwrap();
    server.await.unwrap();
}
