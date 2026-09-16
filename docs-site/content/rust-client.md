## Add the client
```toml
[dependencies]
silicon-honeycomb-client = "0.1.0"
anyhow = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```
The [compilable examples](https://github.com/teamofsilicons/silicon-honeycomb/blob/main/crates/client/examples/documentation.rs) are checked with the workspace.

The crate is imported as `silicon_honeycomb_client`. It re-exports the core model and package types. [Source and API signatures](https://github.com/teamofsilicons/silicon-honeycomb/blob/main/crates/client/src/lib.rs) provide the exact current interface.

## Search with typed results
```rust
use silicon_honeycomb_client::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        "https://backend.honeycomb.teamofsilicons.com"
    )?.with_telemetry(false);
    let page = client.search("briefcase", 1, false).await?;
    for app in page.items {
        println!("{}: {}", app.app_id, app.name);
    }
    Ok(())
}
```
`managed = false` searches the visible catalog; `true` asks for the caller's managed applications. `Client::new` validates the origin and requires HTTPS outside loopback. The client declares API/client versions automatically and does not follow redirects carrying credentials.

## Attach authorization
```rust
async fn example() -> anyhow::Result<()> {
use silicon_honeycomb_client::Client;
let client = Client::new("https://backend.honeycomb.teamofsilicons.com")?;
let authenticated = client.with_token(std::env::var("HONEYCOMB_ACCESS_TOKEN")?);
let status = authenticated.login_status().await?;
assert_eq!(status["authenticated"], true);
Ok(()) }
```
This token must be a Honeycomb application session access token. Your application owns secure storage, refreshing, and revocation. The SDK does not read the CLI's session files. `login(slt, mutation)`, `refresh(refresh_token, mutation)`, and `logout_session(...)` expose the corresponding exchanges. Keep credential-bearing response values out of ordinary logs.

## Explicit mutations
```rust
async fn example() -> anyhow::Result<()> {
use silicon_honeycomb_client::{Client, Mutation};
use std::path::Path;
let client = Client::new("https://backend.honeycomb.teamofsilicons.com")?
    .with_token(std::env::var("HONEYCOMB_ACCESS_TOKEN")?);
let app = client.app("my-org>my-app").await?;
let mutation = Mutation {
    idempotency_key: "my-app-release-upload-0001".into(),
    revision: Some(app.revision),
};
let result = client.upload_release(
    &app.app_id, Path::new("my-app-1.0.0.tar.gz"), &mutation
).await?;
// Inspect accepted/pending state; do not log arbitrary credential results.
let _ = result;
Ok(()) }
```
Persist the mutation key and input before a request when crash recovery matters. Reuse that exact mutation when its outcome is uncertain. `Mutation::new()` generates a fresh key; creating a new one for every retry defeats idempotency. `Mutation::at_revision(n)` generates a key with a revision precondition.

## Testing context
```rust
fn example() -> anyhow::Result<()> {
use silicon_honeycomb_client::Client;
let client = Client::new("https://backend.honeycomb.teamofsilicons.com")?;
let isolated = client.with_environment(std::env::var("HONEYCOMB_TEST_KEY")?)?;
let _ = isolated;
Ok(()) }
```
The environment root key must be exactly 32 alphanumeric characters. The SDK sends `X-Testing-Environment-Key`; it never falls back to production if testing authorization fails. Some operations also require an appropriate user session. The server's [integration availability](/availability/) still applies.

## Method families
| Area | Methods |
| --- | --- |
| Identity | `iam`, `login`, `refresh`, `login_status`, `logout`, `logout_session` |
| Applications | `search`, `app`, `create_app`, `update_app`, `upload_logo`, `reconcile_app` |
| Releases | `releases`, `upload_release`, `download` |
| Community | `review`, `star`, `report` |
| Publication | `request_publication`, `publication`, `publication_message`, `activate_publication` |
| Review work | `review_inbox`, `review_request`, `reply_review`, `decide_review`, `retry_review_plan` |
| Credentials | `rotate_app_secret`, `recover_operation_secret` |
| Webhooks | `webhook_state`, `approve_webhook`, `rotate_webhook_secret`, `retry_webhook_operation` |
| Recovery | `operation`, `retry_operation`, `drafts`, `save_draft` |
| Testing | `environments`, `create_environment`, `environment`, `environment_key`, `environment_action`, `import_application` |
| Retention | `environment_retention`, `set_environment_retention`, `report_environment_activity` |
| Generic transport | `get<T>`, `mutate<T>` |

## Local installation functions
`installer::install_archive(archive, state_dir, aliases, previous)` validates and stages an archive, checks command collisions, and returns an `Installed` record. Your application owns the installation registry and concurrency lock. Pass the previous record for an update; do not overwrite another package's command. `installer::uninstall(record, state_dir)` removes only owned installation artifacts.

`Client::download` verifies hash, size, identity, and version and requires a destination that does not already exist. The `maintenance` module provides CLI release/update and service-registration helpers. A Rust library cannot replace a compiled dependency inside your running application; update dependency versions through your own build/release process.
