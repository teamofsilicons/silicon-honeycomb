//! External changes are accepted only through a separately authenticated integration.
use crate::error::{Error, Result};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait Management: Send + Sync {
    async fn configure(
        &self,
        operation: &Value,
        actor_token: &str,
        environment: Option<&str>,
    ) -> Result<Value>;
    async fn lifecycle(&self, operation: &Value, actor_token: &str) -> Result<Value>;
}
/// Until IAM publishes its protected management contract, configuration stays pending.
/// Do not replace this with direct Carbon endpoints or an application's normal credentials.
pub struct AwaitingIamIntegration;
#[async_trait]
impl Management for AwaitingIamIntegration {
    async fn configure(&self, _: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        Err(Error::unavailable(
            "IAM Honeycomb management integration is not configured. Desired configuration is saved; the application remains pending until IAM accepts it.",
        ))
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::unavailable(
            "IAM and application testing lifecycle integrations must acknowledge this operation before the environment can become ready.",
        ))
    }
}
#[async_trait]
pub trait ArchiveStorage: Send + Sync {
    async fn put(
        &self,
        app_id: &str,
        version: &str,
        path: &std::path::Path,
        actor_token: &str,
        environment: Option<&str>,
        operation_id: &str,
    ) -> Result<String>;
    async fn read(
        &self,
        reference: &str,
        actor_token: Option<&str>,
        environment: Option<&str>,
    ) -> Result<Vec<u8>>;
    async fn publish(
        &self,
        reference: &str,
        actor_token: &str,
        environment: Option<&str>,
        operation_id: &str,
    ) -> Result<String>;
}
