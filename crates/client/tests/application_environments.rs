use silicon_honeycomb_client::{Client, Mutation};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn application_setup_uses_production_basic_auth_and_body_attachment_key() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let key = "K".repeat(32);
    let expected_key = key.clone();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length: usize = headers
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|v| v.parse().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() < end + 4 + length {
                        continue;
                    }
                    assert!(!headers.contains("x-testing-environment-key:"));
                    if step == 0 {
                        assert!(headers.contains("authorization: Basic "));
                        assert!(!headers.contains("production-secret"));
                        let body: serde_json::Value =
                            serde_json::from_slice(&request[end + 4..]).unwrap();
                        assert_eq!(body["testing_key"], expected_key);
                    } else {
                        assert!(headers.contains("authorization: Bearer user-token"));
                        assert!(!headers.contains("authorization: Basic "));
                    }
                    break;
                }
            }
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"items\":[]}").await.unwrap();
        }
    });
    let client = Client::new(&origin)
        .unwrap()
        .with_application("app", "production-secret")
        .unwrap();
    assert!(client.with_environment(&key).is_err());
    assert!(
        Client::new(&origin)
            .unwrap()
            .with_environment(&key)
            .unwrap()
            .with_application("app", "production-secret")
            .is_err()
    );
    client
        .create_application_environment("Testing", "", Some(&key), &Mutation::new())
        .await
        .unwrap();
    client
        .with_token("user-token")
        .environments()
        .await
        .unwrap();
    server.await.unwrap();
}
