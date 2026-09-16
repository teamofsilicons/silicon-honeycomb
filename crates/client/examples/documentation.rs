// Executable/type-checked counterparts of the Rust documentation examples.
use silicon_honeycomb_client::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new("https://backend.honeycomb.teamofsilicons.com")?.with_telemetry(false);
    let page = client.search("briefcase", 1, false).await?;
    for app in page.items {
        println!("{}: {}", app.app_id, app.name);
    }
    Ok(())
}

pub async fn authentication_example() -> anyhow::Result<()> {
    use silicon_honeycomb_client::Client;
    let client = Client::new("https://backend.honeycomb.teamofsilicons.com")?;
    let authenticated = client.with_token(std::env::var("HONEYCOMB_ACCESS_TOKEN")?);
    let status = authenticated.login_status().await?;
    assert_eq!(status["authenticated"], true);
    Ok(())
}

pub async fn upload_example() -> anyhow::Result<()> {
    use silicon_honeycomb_client::{Client, Mutation};
    use std::path::Path;
    let client = Client::new("https://backend.honeycomb.teamofsilicons.com")?
        .with_token(std::env::var("HONEYCOMB_ACCESS_TOKEN")?);
    let app = client.app("my-org>my-app").await?;
    let mutation = Mutation {
        idempotency_key: "my-app-release-upload-0001".into(),
        revision: Some(app.revision),
    };
    let result = client
        .upload_release(&app.app_id, Path::new("my-app-1.0.0.tar.gz"), &mutation)
        .await?;
    // Inspect accepted/pending state; do not log arbitrary credential results.
    let _ = result;
    Ok(())
}

pub fn testing_example() -> anyhow::Result<()> {
    use silicon_honeycomb_client::Client;
    let client = Client::new("https://backend.honeycomb.teamofsilicons.com")?;
    let isolated = client.with_environment(std::env::var("HONEYCOMB_TEST_KEY")?)?;
    let _ = isolated;
    Ok(())
}
