use crate::{
    error::{Error, Result},
    integration::Management,
};
use async_trait::async_trait;
use base64::Engine as _;
use secrecy::ExposeSecret as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use silicon_iam_client::{Client, Credential, EnvironmentKey, IdempotencyKey, Mutation, models};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identity {
    pub principal_id: String,
    pub actor_type: Option<String>,
    pub organizations: BTreeMap<String, Option<String>>,
    pub testing_environment_id: Option<String>,
    pub validator: bool,
}
impl Identity {
    pub fn member(&self, org: &str) -> bool {
        self.organizations.contains_key(org)
    }
    pub fn admin(&self, org: &str) -> bool {
        self.organizations
            .get(org)
            .and_then(|r| r.as_deref())
            .is_some_and(|r| matches!(r, "org_owner" | "org_admin"))
    }
}
#[async_trait]
pub trait IdentityProvider: Send + Sync {
    async fn login(&self, slt: &str, key: &str, environment: Option<&str>) -> Result<Value>;
    async fn refresh(&self, token: &str, key: &str, environment: Option<&str>) -> Result<Value>;
    async fn authenticate(&self, token: &str, environment: Option<&str>) -> Result<Identity>;
    async fn revoke(&self, token: &str, key: &str, environment: Option<&str>) -> Result<()>;
}
pub struct Iam {
    pub client: Client,
    pub app_id: String,
    testing: Option<(SqlitePool, Arc<dyn Management>)>,
}
impl Iam {
    pub fn new(base: &str, app_id: &str, secret: &str) -> anyhow::Result<Self> {
        Ok(Self {
            client: Client::builder(base)?
                .credential(Credential::application(app_id, secret))
                .telemetry(false)
                .build()?,
            app_id: app_id.into(),
            testing: None,
        })
    }
    pub fn with_testing_context(mut self, db: SqlitePool, management: Arc<dyn Management>) -> Self {
        self.testing = Some((db, management));
        self
    }
    async fn scoped(&self, env: Option<&str>) -> Result<Client> {
        let Some(key) = env else {
            return Ok(self.client.clone());
        };
        let environment =
            EnvironmentKey::new(key).map_err(|_| Error::bad("Invalid testing environment key"))?;
        let (db, management) = self.testing.as_ref().ok_or_else(|| Error::unavailable(
            "Testing authentication is awaiting environment setup; production credentials cannot be used",
        ))?;
        let hash = hex::encode(Sha256::digest(key));
        let selected: Option<(String, i64, i64)> = sqlx::query_as(
            "SELECT id,generation,key_version FROM environments WHERE key_hash=? AND state='ready'",
        )
        .bind(&hash)
        .fetch_optional(db)
        .await?;
        let selected = selected.ok_or_else(|| Error::bad("Testing environment is unavailable"))?;
        let Credential::Application { app_id, secret } = self.client.credential() else {
            return Err(Error::unavailable(
                "Application authentication is not configured",
            ));
        };
        if app_id != &self.app_id {
            return Err(Error::unavailable(
                "Application authentication identity does not match",
            ));
        }
        let authorization = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD
                .encode(format!("{app_id}:{}", secret.expose_secret()))
        );
        let recovered = management.recover_testing_application_credential(&selected.0, &authorization)
            .await.map_err(|_| Error::unavailable(
                "Honeycomb authentication is not ready in this testing environment; retry its application setup",
            ))?;
        let test_secret = recovered["app_secret"]
            .as_str()
            .filter(|value| {
                value.len() == 47
                    && value.starts_with("ask_")
                    && value[4..]
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                    && *value != secret.expose_secret()
            })
            .ok_or_else(|| {
                Error::unavailable("IAM returned an invalid testing application credential")
            })?;
        if recovered["environment_id"].as_str() != Some(selected.0.as_str())
            || recovered["app_id"].as_str() != Some(self.app_id.as_str())
        {
            return Err(Error::unavailable(
                "IAM returned a different testing application context",
            ));
        }
        // Cleanup and root rotation can finish while recovery is in flight. Never
        // use a recovered credential after the caller's selected context changes.
        let current: Option<(String, i64, i64)> = sqlx::query_as(
            "SELECT id,generation,key_version FROM environments WHERE key_hash=? AND state='ready'",
        )
        .bind(&hash)
        .fetch_optional(db)
        .await?;
        if current.as_ref() != Some(&selected) {
            return Err(Error::conflict(
                "Testing context changed; retry with the current environment key",
            ));
        }
        Ok(self
            .client
            .with_credential(Credential::application(&self.app_id, test_secret))
            .with_environment(environment))
    }
}
fn mutation(key: &str) -> Result<Mutation> {
    Ok(Mutation::with_key(
        IdempotencyKey::parse(key).map_err(|e| Error::bad(e.to_string()))?,
    ))
}
fn iam_error(e: silicon_iam_client::Error) -> Error {
    match e {
        silicon_iam_client::Error::Api(a) if a.status == 400 && a.code == "invalid_grant" => {
            Error::new(
                axum::http::StatusCode::UNAUTHORIZED,
                "invalid_grant",
                "The IAM login token is invalid, expired, already used, or issued for another application. Obtain a new Honeycomb short-lived token and run honeycomb login <slt>.",
            )
        }
        silicon_iam_client::Error::Api(a) if a.status == 401 => Error::unauthorized(),
        silicon_iam_client::Error::Api(a) if a.status == 403 => Error::forbidden(),
        _ => Error::unavailable(
            "IAM could not complete the request. Retry with the same idempotency key; verify IAM credentials and connectivity.",
        ),
    }
}
#[async_trait]
impl IdentityProvider for Iam {
    async fn login(&self, slt: &str, key: &str, environment: Option<&str>) -> Result<Value> {
        let response = self
            .scoped(environment)
            .await?
            .oauth()
            .login(&self.app_id, slt, &mutation(key)?)
            .await
            .map_err(iam_error)?;
        serde_json::to_value(response).map_err(|e| anyhow::anyhow!(e).into())
    }
    async fn refresh(&self, token: &str, key: &str, environment: Option<&str>) -> Result<Value> {
        let response = self
            .scoped(environment)
            .await?
            .oauth()
            .refresh(&self.app_id, token, &mutation(key)?)
            .await
            .map_err(iam_error)?;
        serde_json::to_value(response).map_err(|e| anyhow::anyhow!(e).into())
    }
    async fn authenticate(&self, token: &str, environment: Option<&str>) -> Result<Identity> {
        let inspected = self
            .scoped(environment)
            .await?
            .oauth()
            .introspect(
                &models::TokenIntrospectionRequest {
                    token: token.into(),
                    token_type_hint: Some(
                        models::TokenIntrospectionRequestTokenTypeHint::AccessToken,
                    ),
                },
                None,
            )
            .await
            .map_err(iam_error)?;
        if !inspected.active || !token.starts_with("oat_") {
            return Err(Error::unauthorized());
        }
        let value = serde_json::to_value(&inspected).map_err(|e| anyhow::anyhow!(e))?;
        let audiences: Vec<&str> = [value["client_id"].as_str(), value["audience"].as_str()]
            .into_iter()
            .flatten()
            .collect();
        if audiences.is_empty() || audiences.iter().any(|audience| *audience != self.app_id) {
            return Err(Error::unauthorized());
        }
        let principal_id = inspected
            .principal_id
            .ok_or_else(Error::unauthorized)?
            .to_string();
        let mut identity = Identity {
            principal_id,
            actor_type: value["actor_type"].as_str().map(str::to_owned),
            organizations: BTreeMap::new(),
            testing_environment_id: None,
            validator: false,
        };
        let snapshots = inspected
            .authorization
            .into_iter()
            .chain(inspected.authorizations.unwrap_or_default());
        for a in snapshots {
            let a = serde_json::to_value(a).map_err(|e| anyhow::anyhow!(e))?;
            if a["audience"].as_str() != Some(&self.app_id)
                || a["principal_id"].as_str() != Some(&identity.principal_id)
            {
                return Err(Error::unauthorized());
            }
            let env = a["testing_environment_id"].as_str().map(str::to_owned);
            if environment.is_some() != env.is_some() {
                return Err(Error::unauthorized());
            }
            if identity.testing_environment_id.is_some() && identity.testing_environment_id != env {
                return Err(Error::unauthorized());
            }
            identity.testing_environment_id = env;
            if let Some(org) = a["org_id"].as_str() {
                // IAM's authorization snapshot uses database role values.
                // The Honeycomb API uses org-prefixed roles consistently with
                // its existing clients. Undisclosed/unknown roles grant nothing.
                let role = match a["org_role"].as_str() {
                    Some("owner") => Some("org_owner".to_owned()),
                    Some("admin") => Some("org_admin".to_owned()),
                    Some("member") => Some("org_member".to_owned()),
                    _ => None,
                };
                identity.organizations.insert(org.into(), role);
            }
        }
        // IAM 1.9 does not disclose Honeycomb's platform validator capability.
        // Keep false until the protected reviewer contract is available.
        Ok(identity)
    }
    async fn revoke(&self, token: &str, key: &str, environment: Option<&str>) -> Result<()> {
        self.scoped(environment)
            .await?
            .oauth()
            .revoke(
                &models::OAuthRevocationRequest {
                    token: token.into(),
                    token_type_hint: None,
                },
                &mutation(key)?,
            )
            .await
            .map_err(iam_error)
    }
}
