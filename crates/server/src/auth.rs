use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use silicon_iam_client::{Client, Credential, EnvironmentKey, IdempotencyKey, Mutation, models};
use std::collections::BTreeMap;

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
}
impl Iam {
    pub fn new(base: &str, app_id: &str, secret: &str) -> anyhow::Result<Self> {
        Ok(Self {
            client: Client::new(base)?.with_credential(Credential::application(app_id, secret)),
            app_id: app_id.into(),
        })
    }
    fn scoped(&self, env: Option<&str>) -> Result<Client> {
        match env {
            Some(key) => Ok(self.client.with_environment(
                EnvironmentKey::new(key)
                    .map_err(|_| Error::bad("Invalid testing environment key"))?,
            )),
            None => Ok(self.client.clone()),
        }
    }
}
fn mutation(key: &str) -> Result<Mutation> {
    Ok(Mutation::with_key(
        IdempotencyKey::parse(key).map_err(|e| Error::bad(e.to_string()))?,
    ))
}
fn iam_error(e: silicon_iam_client::Error) -> Error {
    match e {
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
            .scoped(environment)?
            .oauth()
            .login(&self.app_id, slt, &mutation(key)?)
            .await
            .map_err(iam_error)?;
        serde_json::to_value(response).map_err(|e| anyhow::anyhow!(e).into())
    }
    async fn refresh(&self, token: &str, key: &str, environment: Option<&str>) -> Result<Value> {
        let response = self
            .scoped(environment)?
            .oauth()
            .refresh(&self.app_id, token, &mutation(key)?)
            .await
            .map_err(iam_error)?;
        serde_json::to_value(response).map_err(|e| anyhow::anyhow!(e).into())
    }
    async fn authenticate(&self, token: &str, environment: Option<&str>) -> Result<Identity> {
        let inspected = self
            .scoped(environment)?
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
        if value["client_id"].as_str() != Some(&self.app_id)
            && value["audience"].as_str() != Some(&self.app_id)
        {
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
                identity
                    .organizations
                    .insert(org.into(), a["org_role"].as_str().map(str::to_owned));
            }
        }
        // IAM 1.9 does not disclose Honeycomb's platform validator capability.
        // Keep false until the protected reviewer contract is available.
        Ok(identity)
    }
    async fn revoke(&self, token: &str, key: &str, environment: Option<&str>) -> Result<()> {
        self.scoped(environment)?
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
