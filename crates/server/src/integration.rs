//! External changes are accepted only through a separately authenticated integration.
use crate::error::{Error, Result};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait Management: Send + Sync {
    /// Implement with each service's protected lifecycle API, independently of test sessions.
    async fn service_lifecycle(
        &self,
        app_id: &str,
        operation: &Value,
        actor_token: &str,
    ) -> Result<Value> {
        if app_id == "tos>iam" {
            self.lifecycle(operation, actor_token).await
        } else {
            Err(Error::unavailable(format!(
                "Protected lifecycle transport is not configured for {app_id}"
            )))
        }
    }
    /// Service-authorized retention; must not require or manufacture a user session.
    async fn retention_lifecycle(&self, _app_id: &str, _operation: &Value) -> Result<Value> {
        Err(Error::unavailable(
            "Protected automatic retention transport is not configured for this service",
        ))
    }
    /// Rotate through the dedicated service API, with current actor authority and IAM step-up.
    async fn rotate_secret(
        &self,
        _operation: &Value,
        _actor_token: &str,
        _step_up_assertion: Option<&str>,
        _environment: Option<&str>,
    ) -> Result<Value> {
        Err(Error::unavailable(
            "IAM protected application secret rotation is not yet available",
        ))
    }
    /// Secret-free authoritative webhook state, visible only to current application managers.
    async fn webhook_state(&self, _app: &str, _environment: Option<&str>) -> Result<Value> {
        Err(Error::unavailable("IAM webhook state is not configured"))
    }
    /// Approve a pending destination or rotate signing material with transient step-up.
    async fn webhook_mutation(
        &self,
        _operation: &Value,
        _actor: &str,
        _step_up: Option<&str>,
        _environment: Option<&str>,
    ) -> Result<Value> {
        Err(Error::unavailable(
            "IAM webhook management is not configured",
        ))
    }
    /// Read a protected operation's result within IAM's short secret replay window.
    async fn operation_result(
        &self,
        _operation_id: &str,
        _actor_token: &str,
        _environment: Option<&str>,
    ) -> Result<Value> {
        Err(Error::unavailable(
            "IAM protected operation recovery is not yet available",
        ))
    }
    /// IAM atomically accepts a reviewed public configuration.
    async fn activate_publication(
        &self,
        _operation: &Value,
        _actor_token: &str,
        _environment: Option<&str>,
    ) -> Result<Value> {
        Err(Error::unavailable(
            "IAM protected publication activation is not yet available",
        ))
    }
    async fn application_state(
        &self,
        _app_id: &str,
        _actor_token: &str,
        _environment: Option<&str>,
    ) -> Result<Value> {
        Err(Error::unavailable(
            "IAM protected application reconciliation is not yet available",
        ))
    }
    /// Read-only service authority for notification reconciliation; never grants a mutation.
    async fn application_snapshot(
        &self,
        _app_id: &str,
        _environment: Option<&str>,
    ) -> Result<Value> {
        Err(Error::unavailable(
            "IAM protected notification reconciliation is not yet available",
        ))
    }
    /// IAM derives critical scope gates from its current catalog and eligibility rules.
    async fn scope_catalog(&self, _org: &str, _provider: Option<&str>) -> Result<Value> {
        Err(Error::unavailable("IAM scope discovery is not configured"))
    }
    async fn publication_plan(
        &self,
        _request: &Value,
        _actor_token: &str,
        _environment: Option<&str>,
    ) -> Result<Value> {
        Err(Error::unavailable(
            "IAM publication review planning is not yet available",
        ))
    }
    async fn iam_scope_reviewer(
        &self,
        _actor_token: &str,
        _environment: Option<&str>,
    ) -> Result<bool> {
        Ok(false)
    }
    async fn review_decision(
        &self,
        _operation: &Value,
        _actor_token: &str,
        _environment: Option<&str>,
    ) -> Result<Value> {
        Err(Error::unavailable(
            "IAM protected scope decisions are not yet available",
        ))
    }
    async fn review_notification_recipients(&self, _provider: &str) -> Result<Vec<String>> {
        Err(Error::unavailable(
            "IAM scoped reviewer notification recipients are not yet available",
        ))
    }
    async fn notification_recipients(&self, _org_id: &str) -> Result<Vec<String>> {
        Err(Error::unavailable(
            "IAM protected notification recipient discovery is not yet available",
        ))
    }
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
