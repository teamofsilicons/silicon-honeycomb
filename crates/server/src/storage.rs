//! Briefcase delegated storage. IAM signs exact request bodies through its official client.
use crate::{
    error::{Error, Result},
    integration::ArchiveStorage,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use silicon_iam_client::{Client, EnvironmentKey, Mutation, models};

// Only documented error codes and validated correlation IDs may cross this boundary.
// Upstream messages/details can contain credentials or request data and are never copied.
fn iam_storage_error(error: silicon_iam_client::Error) -> Error {
    use silicon_iam_client::Error as IamError;
    let (status, api, request_id) = match &error {
        IamError::Api(api) => (
            Some(api.status),
            Some(api.as_ref()),
            api.request_id.as_deref(),
        ),
        IamError::RateLimited { source, .. } => (
            Some(429),
            Some(source.as_ref()),
            source.request_id.as_deref(),
        ),
        IamError::UnstructuredResponse { status, request_id } => {
            (Some(*status), None, request_id.as_deref())
        }
        _ => (None, None, None),
    };
    let mut result = if api.is_some() && status == Some(401) {
        Error::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "authentication_required",
            "IAM could not authenticate the storage request. Sign in again, then retry the same operation.",
        )
    } else if api.is_some() && status == Some(403) {
        Error::new(
            axum::http::StatusCode::FORBIDDEN,
            "storage_authorization_required",
            "IAM denied delegated storage access. Check current organization access, consent and effective external scopes, then retry the same operation.",
        )
    } else if api.is_some_and(|e| e.code == "invalid_subject_token") && status == Some(400) {
        Error::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "authentication_required",
            "The IAM login token is no longer valid. Sign in again, then retry the same operation.",
        )
    } else if status == Some(429) {
        Error::unavailable(
            "IAM temporarily rate-limited storage authorization. Wait and retry the same operation.",
        )
    } else {
        Error::unavailable(
            "IAM could not complete storage authorization. Retry the same operation; if it persists, report the diagnostic details.",
        )
    };
    if let Some(status) = status {
        result.1.details.push(format!("upstream_status={status}"));
    }
    if let Some(api) = api
        && matches!(
            api.code.as_str(),
            "internal_error"
                | "service_unavailable"
                | "rate_limited"
                | "unauthorized"
                | "authentication_required"
                | "invalid_credentials"
                | "forbidden"
                | "invalid_subject_token"
                | "obo_subject_token_forbidden"
                | "obo_authority_revoked"
                | "obo_organization_not_authorized"
                | "not_found"
                | "insufficient_scope"
                | "consent_required"
                | "idempotency_conflict"
                | "idempotency_response_expired"
        )
    {
        result.1.details.push(format!("upstream_code={}", api.code));
    }
    if let Some(id) = request_id.and_then(|id| uuid::Uuid::parse_str(id).ok()) {
        result.1.details.push(format!("upstream_request_id={id}"));
    }
    result
}

pub struct Briefcase {
    pub iam: Client,
    pub http: reqwest::Client,
    pub base_url: String,
    pub app_id: String,
    pub audience: String,
}
impl Briefcase {
    async fn control(
        &self,
        endpoint: &str,
        path: &str,
        body: Value,
        token: &str,
        environment: Option<&str>,
        org: &str,
    ) -> Result<(reqwest::Response, Option<String>)> {
        let bytes = serde_json::to_vec(&body).map_err(|e| anyhow::anyhow!(e))?;
        let iam = match environment {
            Some(k) => self.iam.with_environment(
                EnvironmentKey::new(k).map_err(|_| Error::bad("Invalid environment key"))?,
            ),
            None => self.iam.clone(),
        };
        let catalog = iam
            .obo()
            .endpoints(&self.audience)
            .await
            .map_err(iam_storage_error)?;
        let input:models::OboExchangeRequest=serde_json::from_value(json!({"org_id":org,"subject_token":token,"audience":self.audience,"endpoint_id":endpoint,"metadata":{},"request":{"method":"POST","body_sha256":silicon_iam_client::api::obo::body_sha256(&bytes)}})).map_err(|e|anyhow::anyhow!(e))?;
        let proof = iam
            .obo()
            .exchange_signed(&input, &catalog, &Mutation::new())
            .await
            .map_err(iam_storage_error)?;
        let mut request = self
            .http
            .post(format!("{}{path}", self.base_url))
            .header("x-app-id", &self.app_id)
            .header("x-iam-obo-access-proof", proof.access_proof)
            .header("content-type", "application/json");
        let test_secret = proof.testing_context.map(|c| c.app_secret);
        if let Some(secret) = &test_secret {
            request = request.header("x-briefcase-app-secret", secret);
        }
        let response = request.body(bytes).send().await.map_err(|_| {
            Error::unavailable(
                "Briefcase request failed; reconcile the same operation before retrying",
            )
        })?;
        if !response.status().is_success() {
            return Err(Error::unavailable(format!(
                "Briefcase returned HTTP {}; verify delegated permissions and retry the same operation",
                response.status().as_u16()
            )));
        }
        Ok((response, test_secret))
    }
}
struct UploadFile<'a> {
    org: &'a str,
    name: &'a str,
    content_type: &'a str,
}
impl Briefcase {
    async fn put_file(
        &self,
        file: UploadFile<'_>,
        path: &std::path::Path,
        token: &str,
        environment: Option<&str>,
        operation_id: &str,
    ) -> Result<String> {
        let sha256 = honeycomb_core::package::sha256(path)?;
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        let body = json!({"operation_id":operation_id,"parent_path":format!("apps/{}/public",self.app_id),"name":file.name,"content_type":file.content_type,"size":bytes.len(),"sha256":sha256});
        let (response, test_secret) = self
            .control(
                "briefcase.uploads.reserve",
                "/api/v1/obo/uploads/reserve",
                body,
                token,
                environment,
                file.org,
            )
            .await?;
        let reservation: Value = response
            .json()
            .await
            .map_err(|_| Error::unavailable("Invalid Briefcase reservation response"))?;
        if reservation["state"] == "committed" {
            return reservation["published_entry_id"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| Error::unavailable("Briefcase committed without an entry ID"));
        }
        let upload_id = reservation["upload_id"]
            .as_str()
            .ok_or_else(|| Error::unavailable("Briefcase did not return upload_id"))?;
        if reservation["state"] == "reserved" {
            let capability = reservation["capability"].as_str().ok_or_else(|| {
                Error::unavailable("Briefcase did not return a transfer capability")
            })?;
            let mut request = self
                .http
                .put(format!(
                    "{}/api/v1/obo/uploads/{upload_id}/content",
                    self.base_url
                ))
                .header("x-org-id", file.org)
                .header("x-briefcase-upload-capability", capability)
                .header("content-type", "application/octet-stream");
            if let Some(secret) = test_secret {
                request = request.header("x-briefcase-app-secret", secret);
            }
            let response = request.body(bytes).send().await.map_err(|_| {
                Error::unavailable(
                    "Briefcase transfer failed; retry this release operation to reconcile",
                )
            })?;
            if !response.status().is_success() {
                return Err(Error::unavailable("Briefcase transfer did not complete"));
            }
        } else if reservation["state"] != "staged" {
            return Err(Error::unavailable(
                "Briefcase upload is in progress or requires cleanup; retry the same operation later",
            ));
        }
        let (response, _) = self
            .control(
                "briefcase.uploads.commit",
                "/api/v1/obo/uploads/commit",
                json!({"operation_id":operation_id,"upload_id":upload_id}),
                token,
                environment,
                file.org,
            )
            .await?;
        let committed: Value = response
            .json()
            .await
            .map_err(|_| Error::unavailable("Invalid Briefcase commit response"))?;
        committed["published_entry_id"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::unavailable("Briefcase did not confirm the published entry ID"))
    }
}

#[async_trait]
impl ArchiveStorage for Briefcase {
    async fn put(
        &self,
        app_id: &str,
        version: &str,
        path: &std::path::Path,
        token: &str,
        environment: Option<&str>,
        operation_id: &str,
    ) -> Result<String> {
        let org = app_id
            .split_once('>')
            .ok_or_else(|| Error::bad("Invalid app ID"))?
            .0;
        let name = format!("{}-{version}.tar.gz", app_id.replace('>', "--"));
        self.put_file(
            UploadFile {
                org,
                name: &name,
                content_type: "application/gzip",
            },
            path,
            token,
            environment,
            operation_id,
        )
        .await
    }
    async fn upload_logo(
        &self,
        org: &str,
        path: &std::path::Path,
        token: &str,
        environment: Option<&str>,
        operation_id: &str,
    ) -> Result<String> {
        let name = format!("logo-{operation_id}.png");
        let reference = self
            .put_file(
                UploadFile {
                    org,
                    name: &name,
                    content_type: "image/png",
                },
                path,
                token,
                environment,
                operation_id,
            )
            .await?;
        let published = self
            .publish_file(&reference, token, environment, operation_id, org)
            .await?;
        let published: Value = serde_json::from_str(&published).map_err(|e| anyhow::anyhow!(e))?;
        let path = published["public_path"]
            .as_str()
            .ok_or_else(|| Error::unavailable("Briefcase omitted the public image path"))?;
        let mut url = url::Url::parse(&format!("{}{path}", self.base_url))
            .map_err(|_| Error::unavailable("Invalid Briefcase image URL"))?;
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(key, _)| key != "view")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        url.set_query(None);
        url.query_pairs_mut()
            .extend_pairs(pairs)
            .append_pair("view", "inline");
        Ok(url.to_string())
    }
    async fn read(
        &self,
        reference: &str,
        org: &str,
        token: Option<&str>,
        environment: Option<&str>,
    ) -> Result<Vec<u8>> {
        let stored: Option<Value> = serde_json::from_str(reference).ok();
        let response = if let Some(path) = stored.as_ref().and_then(|s| s["public_path"].as_str()) {
            // Only a path on the configured Briefcase API is stored. No arbitrary fetch URL.
            if !path.starts_with("/api/v1/public/") || path.contains("..") {
                return Err(Error::unavailable("Invalid stored public archive path"));
            }
            self.http
                .get(format!("{}{path}", self.base_url))
                .send()
                .await
                .map_err(|_| Error::unavailable("Briefcase public download interrupted"))?
        } else {
            let token = token.ok_or_else(Error::unauthorized)?;
            let entry = stored
                .as_ref()
                .and_then(|s| s["entry_id"].as_str())
                .unwrap_or(reference);
            self.control(
                "briefcase.files.read",
                "/api/v1/obo/files/read",
                json!({"entry_id":entry,"download":true}),
                token,
                environment,
                org,
            )
            .await?
            .0
        };
        if !response.status().is_success() {
            return Err(Error::unavailable("Briefcase could not serve this archive"));
        }
        if response
            .content_length()
            .is_some_and(|n| n > honeycomb_core::package::MAX_ARCHIVE_BYTES)
        {
            return Err(Error::bad("Stored archive exceeds size limit"));
        }
        let mut response = response;
        let mut data = vec![];
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Error::unavailable("Briefcase download interrupted"))?
        {
            if data.len() + chunk.len() > honeycomb_core::package::MAX_ARCHIVE_BYTES as usize {
                return Err(Error::bad("Stored archive exceeds size limit"));
            }
            data.extend_from_slice(&chunk);
        }
        Ok(data)
    }
    async fn publish(
        &self,
        reference: &str,
        org: &str,
        token: &str,
        environment: Option<&str>,
        operation_id: &str,
    ) -> Result<String> {
        self.publish_file(reference, token, environment, operation_id, org)
            .await
    }
}

impl Briefcase {
    async fn publish_file(
        &self,
        reference: &str,
        token: &str,
        environment: Option<&str>,
        operation_id: &str,
        org: &str,
    ) -> Result<String> {
        let stored: Option<Value> = serde_json::from_str(reference).ok();
        let entry = stored
            .as_ref()
            .and_then(|s| s["entry_id"].as_str())
            .unwrap_or(reference);
        // A stable operation UUID avoids repeated side effects after a lost link response.
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(format!("{operation_id}:publish:{entry}"));
        let link_operation =
            uuid::Uuid::from_slice(&digest[..16]).map_err(|e| anyhow::anyhow!(e))?;
        let (response, _) = self
            .control(
                "briefcase.link_access.update",
                "/api/v1/obo/link-access",
                json!({"operation_id":link_operation,"entry_id":entry,"enabled":true}),
                token,
                environment,
                org,
            )
            .await?;
        let link: Value = response
            .json()
            .await
            .map_err(|_| Error::unavailable("Invalid Briefcase sharing response"))?;
        public_reference(entry, &link, environment.is_some())
    }
}

fn public_reference(entry: &str, link: &Value, testing: bool) -> Result<String> {
    if link["effective"] != true {
        return Err(Error::unavailable(
            "Briefcase did not enable public archive access",
        ));
    }
    let url = link["url"]
        .as_str()
        .and_then(|u| url::Url::parse(u).ok())
        .ok_or_else(|| Error::unavailable("Briefcase returned no public archive URL"))?;
    let path = url
        .path()
        .strip_prefix("/org/")
        .filter(|p| !p.is_empty() && !p.contains(".."))
        .ok_or_else(|| Error::unavailable("Briefcase public URL has an invalid path"))?;
    let mut download = url::Url::parse("https://briefcase.invalid").unwrap();
    download.set_path(&format!("/api/v1/public/{path}"));
    download.query_pairs_mut().append_pair("view", "attachment");
    let test_id = url
        .query_pairs()
        .find(|(key, _)| key == "test_environment")
        .map(|(_, v)| v.into_owned());
    if testing != test_id.is_some() {
        return Err(Error::unavailable(
            "Briefcase sharing response belongs to a different environment plane",
        ));
    }
    if let Some(id) = test_id {
        uuid::Uuid::parse_str(&id)
            .map_err(|_| Error::unavailable("Invalid Briefcase test environment"))?;
        download
            .query_pairs_mut()
            .append_pair("test_environment", &id);
    }
    Ok(json!({"entry_id":entry,"public_path":format!("{}?{}",download.path(),download.query().unwrap())}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_download_uses_configured_backend_and_preserves_encoded_segments() {
        let value:Value = serde_json::from_str(&public_reference("entry", &json!({"effective":true,"url":"https://briefcase.teamofsilicons.com/org/tos/apps/tos%3Ehoneycomb/public/package.tar.gz"}), false).unwrap()).unwrap();
        assert_eq!(
            value["public_path"],
            "/api/v1/public/tos/apps/tos%3Ehoneycomb/public/package.tar.gz?view=attachment"
        );
        assert!(!value.to_string().contains("https:"));
    }
    #[test]
    fn public_download_rejects_cross_plane_links() {
        let link = json!({"effective":true,"url":"https://briefcase.teamofsilicons.com/org/tos/file?test_environment=12345678-1234-1234-1234-123456789abc"});
        assert!(public_reference("entry", &link, false).is_err());
        assert!(
            public_reference("entry", &link, true)
                .unwrap()
                .contains("test_environment=")
        );
        assert!(public_reference("entry", &json!({"effective":false}), false).is_err());
    }
}
