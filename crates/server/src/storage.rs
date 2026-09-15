//! Briefcase delegated storage. IAM signs exact request bodies through its official client.
use crate::{
    error::{Error, Result},
    integration::ArchiveStorage,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use silicon_iam_client::{Client, EnvironmentKey, Mutation, models};

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
    ) -> Result<(reqwest::Response, Option<String>)> {
        let bytes = serde_json::to_vec(&body).map_err(|e| anyhow::anyhow!(e))?;
        let iam = match environment {
            Some(k) => self.iam.with_environment(
                EnvironmentKey::new(k).map_err(|_| Error::bad("Invalid environment key"))?,
            ),
            None => self.iam.clone(),
        };
        let catalog =
            iam.obo().endpoints(&self.audience).await.map_err(|_| {
                Error::unavailable("Cannot discover Briefcase OBO endpoints in IAM")
            })?;
        let input:models::OboExchangeRequest=serde_json::from_value(json!({"subject_token":token,"audience":self.audience,"endpoint_id":endpoint,"metadata":{},"request":{"method":"POST","body_sha256":silicon_iam_client::api::obo::body_sha256(&bytes)}})).map_err(|e|anyhow::anyhow!(e))?;
        let proof=iam.obo().exchange_signed(&input,&catalog,&Mutation::new()).await.map_err(|_|Error::unavailable("IAM refused the Briefcase OBO proof. Check consent and effective external scopes."))?;
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
        let sha256 = honeycomb_core::package::sha256(path)?;
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        let body = json!({"operation_id":operation_id,"parent_path":format!("apps/{}/public",self.app_id),"name":format!("{}-{version}.tar.gz",app_id.replace('>',"--")),"content_type":"application/gzip","size":bytes.len(),"sha256":sha256});
        let (response, test_secret) = self
            .control(
                "briefcase.uploads.reserve",
                "/api/v1/obo/uploads/reserve",
                body,
                token,
                environment,
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
            let org_id = app_id
                .split_once('>')
                .ok_or_else(|| Error::bad("Invalid app ID"))?
                .0;
            let mut request = self
                .http
                .put(format!(
                    "{}/api/v1/obo/uploads/{upload_id}/content",
                    self.base_url
                ))
                .header("x-org-id", org_id)
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
    async fn read(
        &self,
        reference: &str,
        token: Option<&str>,
        environment: Option<&str>,
    ) -> Result<Vec<u8>> {
        let token = token.ok_or_else(|| {
            Error::unavailable(
                "Anonymous Briefcase download link is not yet recorded for this release",
            )
        })?;
        let (response, _) = self
            .control(
                "briefcase.files.read",
                "/api/v1/obo/files/read",
                json!({"entry_id":reference,"download":true}),
                token,
                environment,
            )
            .await?;
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
    async fn publish(&self, reference: &str, token: &str, environment: Option<&str>) -> Result<()> {
        self.control("briefcase.link_access.update","/api/v1/obo/link-access",json!({"operation_id":uuid::Uuid::new_v4().to_string(),"entry_id":reference,"enabled":true}),token,environment).await?;
        Ok(())
    }
}
