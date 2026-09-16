//! Read-only operator check against the configured IAM management service.
//! Load the protected IAM handoff into the environment; no credentials are printed.
use anyhow::{Context, ensure};
use secrecy::SecretString;
use serde_json::json;
use silicon_iam_client::honeycomb::ManagementClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let base = std::env::var("IAM_BASE_URL").context("IAM_BASE_URL is required")?;
    let app = std::env::var("HONEYCOMB_APP_ID").context("HONEYCOMB_APP_ID is required")?;
    let credential = std::env::var("IAM_HONEYCOMB_SERVICE_CREDENTIAL")
        .context("IAM_HONEYCOMB_SERVICE_CREDENTIAL is required")?;
    let client = ManagementClient::new(&base, SecretString::from(credential))?;
    let catalog = client.scope_catalog().await?;
    let scopes = catalog["items"]
        .as_array()
        .context("Scope catalog omitted items")?;
    let record = client.application(&app).await?;
    ensure!(record["app_id"] == app, "IAM returned another application");
    ensure!(
        record["iam_revision"].as_i64().is_some(),
        "IAM omitted its revision"
    );
    let effective: std::collections::BTreeSet<&str> = record["effective_scopes"]
        .as_array()
        .context("IAM omitted effective scopes")?
        .iter()
        .filter_map(|scope| scope["scope"].as_str())
        .collect();
    let storage_app = std::env::var("BRIEFCASE_APP_ID").context("BRIEFCASE_APP_ID is required")?;
    ensure!(
        honeycomb_core::valid_app_id(&storage_app),
        "Invalid storage application ID"
    );
    let mut required: Vec<String> = [
        "self.identity.read",
        "self.profile.read",
        "self.membership.read",
        "self.tags.read",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    required.extend(
        [
            "briefcase.uploads.reserve",
            "briefcase.uploads.commit",
            "briefcase.files.read",
            "briefcase.link_access.update",
        ]
        .into_iter()
        .map(|endpoint| format!("obo:{storage_app}:{endpoint}")),
    );
    let missing: Vec<_> = required
        .into_iter()
        .filter(|scope| !effective.contains(scope.as_str()))
        .collect();
    println!(
        "{}",
        json!({
            "check": "iam_management_read", "app_id": app,
            "scope_count": scopes.len(), "iam_revision": record["iam_revision"],
            "configuration_revision": record["configuration_revision"],
            "availability": record["availability"], "visibility": record["visibility"],
            "missing_effective_scopes": missing
        })
    );
    if std::env::args().any(|arg| arg == "--require-ready") {
        ensure!(
            record["availability"] == "verified",
            "Honeycomb's IAM app is not verified"
        );
        ensure!(
            missing.is_empty(),
            "Provision the reported IAM and Briefcase scopes before live acceptance testing"
        );
    }
    Ok(())
}
