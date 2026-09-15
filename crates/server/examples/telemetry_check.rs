//! Explicit operator check: sends one synthetic record using the configured table key.
#[cfg(unix)]
fn main() -> anyhow::Result<()> {
    use std::time::Duration;
    let key = std::env::var("HONEYCOMB_SPACE_STATION_TABLE_KEY")
        .map_err(|_| anyhow::anyhow!("HONEYCOMB_SPACE_STATION_TABLE_KEY is required"))?;
    let home = tempfile::tempdir()?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = space_station::SpaceClient::builder(&key)
        .url("https://backend.spacestation.teamofsilicons.com")
        .home(home.path())
        .flush_timeout(Duration::from_secs(15))
        .on_error(|_| {})
        .build()
        .map_err(|_| anyhow::anyhow!("Unable to initialize Space Station"))?;
    let check_id = uuid::Uuid::new_v4();
    client.record(serde_json::json!({
        "schema_version": 1, "service": "silicon-honeycomb",
        "version": env!("CARGO_PKG_VERSION"), "source": "backend",
        "event": "telemetry_verified", "step": "operator_check", "progress": 1,
        "recorded_at": silicon_honeycomb_server::now(),
        "environment": "production",
        "context": {"synthetic": true, "check_id": check_id}
    }));
    anyhow::ensure!(
        client.flush(),
        "Space Station did not acknowledge the check within 15 seconds"
    );
    println!("Space Station acknowledged synthetic check {check_id}");
    Ok(())
}
#[cfg(not(unix))]
fn main() {
    eprintln!("Run this backend operator check on Linux or macOS.");
    std::process::exit(1);
}
