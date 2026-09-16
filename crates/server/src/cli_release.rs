//! Public discovery of the latest complete native Honeycomb CLI release.
use crate::error::{Error, Result};
use axum::{
    Json,
    http::header,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::{
    sync::LazyLock,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const RELEASES: &str =
    "https://api.github.com/repos/teamofsilicons/silicon-honeycomb/releases/latest";
const CACHE_TTL: Duration = Duration::from_secs(300);
static CACHE: LazyLock<Mutex<Option<(Instant, Value)>>> = LazyLock::new(|| Mutex::new(None));

pub async fn latest() -> Result<Response> {
    // Serialize cache misses to avoid one upstream request per concurrent installer.
    let mut cached = CACHE.lock().await;
    if let Some((at, release)) = &*cached
        && at.elapsed() < CACHE_TTL
    {
        return Ok(reply(release.clone()));
    }
    let release = fetch(RELEASES).await?;
    *cached = Some((Instant::now(), release.clone()));
    Ok(reply(release))
}
fn reply(release: Value) -> Response {
    (
        [(header::CACHE_CONTROL, "public, max-age=60")],
        Json(release),
    )
        .into_response()
}
async fn fetch(url: &str) -> Result<Value> {
    let http = reqwest::Client::builder()
        .user_agent("honeycomb-release-discovery")
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| Error::unavailable("CLI release discovery is unavailable"))?;
    let mut response = http
        .get(url)
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| Error::unavailable("Could not check the latest CLI release; retry shortly"))?;
    if !response.status().is_success() {
        return Err(Error::unavailable(
            "Could not check the latest CLI release; retry shortly",
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::unavailable("CLI release metadata is incomplete"))?
    {
        if body.len() + chunk.len() > 1024 * 1024 {
            return Err(Error::unavailable(
                "CLI release metadata exceeds its size limit",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    let metadata = serde_json::from_slice(&body)
        .map_err(|_| Error::unavailable("CLI release metadata is invalid"))?;
    manifest(&metadata)
}
fn manifest(metadata: &Value) -> Result<Value> {
    let invalid = || Error::unavailable("The latest CLI release is not complete; retry shortly");
    if metadata["draft"] != false || metadata["prerelease"] != false {
        return Err(invalid());
    }
    let tag = metadata["tag_name"].as_str().ok_or_else(invalid)?;
    let version = tag
        .strip_prefix('v')
        .and_then(|s| semver::Version::parse(s).ok())
        .ok_or_else(invalid)?;
    if !version.pre.is_empty() || tag != format!("v{version}") {
        return Err(invalid());
    }
    let base =
        format!("https://github.com/teamofsilicons/silicon-honeycomb/releases/download/{tag}");
    let assets = metadata["assets"].as_array().ok_or_else(invalid)?;
    let mut targets = serde_json::Map::new();
    for target in [
        "linux-x86_64",
        "linux-aarch64",
        "macos-x86_64",
        "macos-aarch64",
        "windows-x86_64",
        "windows-aarch64",
    ] {
        let name = format!("honeycomb-{target}.tar.gz");
        for asset_name in [name.clone(), format!("{name}.sha256")] {
            let matches: Vec<_> = assets
                .iter()
                .filter(|asset| asset["name"] == asset_name)
                .collect();
            if matches.len() != 1
                || matches[0]["state"] != "uploaded"
                || matches[0]["size"].as_u64().is_none_or(|size| size == 0)
                || matches[0]["browser_download_url"] != format!("{base}/{asset_name}")
            {
                return Err(invalid());
            }
        }
        targets.insert(target.into(), json!({"archive_url":format!("{base}/{name}"), "checksum_url":format!("{base}/{name}.sha256")}));
    }
    Ok(
        json!({"version":version.to_string(), "release_url":format!("https://github.com/teamofsilicons/silicon-honeycomb/releases/tag/{tag}"), "targets":targets}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release() -> Value {
        let mut assets = vec![];
        for target in [
            "linux-x86_64",
            "linux-aarch64",
            "macos-x86_64",
            "macos-aarch64",
            "windows-x86_64",
            "windows-aarch64",
        ] {
            for suffix in ["", ".sha256"] {
                let name = format!("honeycomb-{target}.tar.gz{suffix}");
                assets.push(json!({"name":name,"state":"uploaded","size":100,"browser_download_url":format!("https://github.com/teamofsilicons/silicon-honeycomb/releases/download/v9.0.0/{name}")}));
            }
        }
        json!({"tag_name":"v9.0.0","draft":false,"prerelease":false,"assets":assets})
    }
    #[test]
    fn only_complete_stable_vendor_releases_are_advertised() {
        let source = release();
        let result = manifest(&source).unwrap();
        assert_eq!(result["version"], "9.0.0");
        assert_eq!(result["targets"].as_object().unwrap().len(), 6);
        for key in ["draft", "prerelease"] {
            let mut value = source.clone();
            value[key] = json!(true);
            assert!(manifest(&value).is_err());
        }
        let mut value = source.clone();
        value["assets"].as_array_mut().unwrap().pop();
        assert!(manifest(&value).is_err());
        let mut value = source.clone();
        value["assets"][0]["browser_download_url"] = json!("https://untrusted.example/binary");
        assert!(manifest(&value).is_err());
        let mut value = source.clone();
        value["assets"][0]["state"] = json!("starter");
        assert!(manifest(&value).is_err());
        let mut value = source;
        value["tag_name"] = json!("v9.0.0-rc.1");
        assert!(manifest(&value).is_err());
    }
    #[tokio::test]
    async fn discovery_fetches_fresh_metadata_and_fails_closed_on_upstream_errors() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(release()))
            .mount(&server)
            .await;
        assert_eq!(fetch(&server.uri()).await.unwrap()["version"], "9.0.0");
        server.reset().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;
        assert!(fetch(&server.uri()).await.is_err());
    }
}
