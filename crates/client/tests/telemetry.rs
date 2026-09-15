use silicon_honeycomb_client::Client;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test]
async fn opt_out_reaches_backend_and_suppresses_diagnostic_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = Client::new(&format!("http://{}", listener.local_addr().unwrap()))
        .unwrap()
        .with_token("fixture-token")
        .with_telemetry(false);
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = vec![0; 4096];
        let count = stream.read(&mut bytes).await.unwrap();
        let request = String::from_utf8_lossy(&bytes[..count]);
        assert!(request.contains("x-honeycomb-telemetry: false\r\n"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .await
            .unwrap();
        listener
    });
    let _: serde_json::Value = client.get(&["iam"]).await.unwrap();
    let listener = server.await.unwrap();
    client
        .diagnostic("cli", "command_completed", "apps", 10, true)
        .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn authenticated_diagnostics_preserve_the_testing_context() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = Client::new(&format!("http://{}", listener.local_addr().unwrap()))
        .unwrap()
        .with_token("fixture-token")
        .with_environment("A".repeat(32))
        .unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let count = stream.read(&mut chunk).await.unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let size: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if bytes.len() < end + 4 + size {
                    continue;
                }
                assert!(headers.starts_with("POST /api/v1/telemetry HTTP/1.1"));
                assert!(headers.contains("authorization: Bearer fixture-token"));
                assert!(
                    headers.contains(&format!("x-testing-environment-key: {}", "A".repeat(32)))
                );
                let body: serde_json::Value = serde_json::from_slice(&bytes[end + 4..]).unwrap();
                assert_eq!(
                    body,
                    serde_json::json!({"source":"daemon","event":"update_completed","action":"self_update","duration_ms":12,"success":false})
                );
                break;
            }
        }
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
    });
    client
        .diagnostic("daemon", "update_completed", "self_update", 12, false)
        .await;
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
}
