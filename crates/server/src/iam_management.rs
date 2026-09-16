//! Adapter for the official IAM management SDK.
mod application;
mod publication;
mod testing;
use crate::{
    error::{Error, Result},
    integration::Management,
    now,
};
use async_trait::async_trait;
use secrecy::SecretString;
use serde_json::{Value, json};
use silicon_iam_client::{IdempotencyKey, Mutation, honeycomb::ManagementClient, models};
use sqlx::Row;
use std::collections::BTreeSet;

pub struct IamManagement {
    client: ManagementClient,
    identity_app_id: Option<String>,
    participants: crate::participant_management::ParticipantRegistry,
    db: sqlx::SqlitePool,
    encryption_key: [u8; 32],
}
impl IamManagement {
    pub fn new(
        base: &str,
        credential: String,
        db: sqlx::SqlitePool,
        encryption_key: [u8; 32],
    ) -> anyhow::Result<Self> {
        Ok(Self {
            client: ManagementClient::new(base, SecretString::from(credential))?,
            identity_app_id: None,
            participants: Default::default(),
            db,
            encryption_key,
        })
    }
    pub fn with_identity_app(mut self, app_id: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            honeycomb_core::valid_app_id(app_id),
            "Invalid identity application ID"
        );
        self.identity_app_id = Some(app_id.to_owned());
        Ok(self)
    }
    pub fn with_participants(
        mut self,
        participants: crate::participant_management::ParticipantRegistry,
    ) -> Self {
        self.participants = participants;
        self
    }
    async fn saved(&self, id: &str, kind: &str, app: &str, body: Value) -> Result<Value> {
        let encrypted = crate::encrypt(&self.encryption_key, &body.to_string())?;
        sqlx::query("INSERT OR IGNORE INTO management_requests(operation_id,kind,app_id,encrypted_body,created_at) VALUES(?,?,?,?,?)")
            .bind(id).bind(kind).bind(app).bind(encrypted).bind(now()).execute(&self.db).await?;
        let row = sqlx::query(
            "SELECT kind,app_id,encrypted_body FROM management_requests WHERE operation_id=?",
        )
        .bind(id)
        .fetch_one(&self.db)
        .await?;
        if row.get::<String, _>("kind") != kind || row.get::<String, _>("app_id") != app {
            return Err(Error::conflict("Management operation identity changed"));
        }
        serde_json::from_str(&crate::decrypt(
            &self.encryption_key,
            &row.get::<String, _>("encrypted_body"),
        )?)
        .map_err(|e| anyhow::anyhow!(e).into())
    }
    async fn execute(
        &self,
        id: &str,
        kind: &str,
        app: &str,
        body: Value,
        actor: &str,
        step_up: Option<&str>,
    ) -> Result<Value> {
        let mut mutation = Mutation::with_key(
            IdempotencyKey::parse(id).map_err(|_| Error::bad("Invalid management operation ID"))?,
        );
        if let Some(proof) = step_up {
            mutation = mutation.step_up(proof);
        }
        let actor = SecretString::from(actor.to_owned());
        let receipt = match kind {
            "configure" => {
                self.client
                    .configure_application(
                        app,
                        &actor,
                        &decode::<models::HoneycombConfiguration>(body)?,
                        &mutation,
                    )
                    .await
            }
            "secret.rotate" => {
                self.client
                    .rotate_secret(
                        app,
                        &actor,
                        &decode::<models::HoneycombSecretRotation>(body)?,
                        &mutation,
                    )
                    .await
            }
            "webhook.approve" => {
                self.client
                    .approve_webhook(
                        app,
                        &actor,
                        &decode::<models::HoneycombWebhookApproval>(body)?,
                        &mutation,
                    )
                    .await
            }
            "webhook.rotate" => {
                self.client
                    .rotate_webhook_secret(
                        app,
                        &actor,
                        &decode::<models::HoneycombWebhookSecretRotation>(body)?,
                        &mutation,
                    )
                    .await
            }
            _ => {
                return Err(Error::unavailable(
                    "Management operation is not supported by this adapter",
                ));
            }
        }
        .map_err(map_error)?;
        let mut response = serde_json::to_value(receipt).map_err(|e| anyhow::anyhow!(e))?;
        if response["operation_id"] != id {
            return Err(Error::unavailable("IAM returned another operation"));
        }
        let local = sqlx::query("SELECT revision FROM operations WHERE id=? AND resource=?")
            .bind(id)
            .bind(app)
            .fetch_one(&self.db)
            .await?;
        if kind == "configure" && response["state"] == "accepted" {
            response["effective_configuration"] = self
                .configuration(app, &response["effective_configuration"])
                .await?;
        }
        if kind == "secret.rotate" {
            // Configuration identity is carried by our actor-bound immutable request;
            // IAM returns application/credential version for the accepted rotation.
            if response["app_id"] != app {
                return Err(Error::unavailable(
                    "IAM returned another application's rotation",
                ));
            }
            response["configuration_revision"] = json!(local.get::<i64, _>("revision"));
        }
        Ok(response)
    }
    async fn configuration(&self, app: &str, record: &Value) -> Result<Value> {
        let (org, local) = app
            .split_once('>')
            .ok_or_else(|| Error::bad("Invalid app ID"))?;
        if record["app_id"] != app || record["org_id"] != org {
            return Err(Error::unavailable(
                "IAM returned another application's record",
            ));
        }
        let row = sqlx::query("SELECT revision,config,effective_config FROM applications WHERE plane='production' AND app_id=?")
            .bind(app).fetch_one(&self.db).await?;
        let accepted_revision = record["configuration_revision"]
            .as_i64()
            .ok_or_else(|| Error::unavailable("IAM omitted its accepted configuration revision"))?;
        let base: String = if accepted_revision == row.get::<i64, _>("revision") {
            row.get("config")
        } else {
            row.get::<Option<String>, _>("effective_config")
                .ok_or_else(|| Error::unavailable("No matching accepted catalog metadata exists"))?
        };
        let mut config: Value = serde_json::from_str(&base).map_err(|e| anyhow::anyhow!(e))?;
        config["org_id"] = json!(org);
        config["local_app_id"] = json!(local);
        for (to, from) in [
            ("name", "app_name"),
            ("logo_url", "app_logo"),
            ("base_url", "base_url"),
            ("obo_endpoints", "obo_endpoints"),
            ("obo_review_message", "obo_review_message"),
            ("testing_idle_days", "testing_idle_days"),
            ("webhook_scope", "webhook_scope"),
        ] {
            config[to] = record[from].clone();
        }
        let granted: BTreeSet<&str> = record["effective_scopes"]
            .as_array()
            .ok_or_else(|| Error::unavailable("IAM omitted effective scopes"))?
            .iter()
            .filter_map(|v| v["scope"].as_str())
            .collect();
        let mut scopes = record["app_scope"].clone();
        if let Some(iam) = scopes["iam"].as_array_mut() {
            iam.retain(|v| v.as_str().is_some_and(|s| granted.contains(s)));
        }
        if let Some(external) = scopes["external"].as_array_mut() {
            external.retain(|v| {
                granted.contains(
                    format!(
                        "obo:{}:{}",
                        v["app_id"].as_str().unwrap_or(""),
                        v["endpoint_id"].as_str().unwrap_or("")
                    )
                    .as_str(),
                )
            });
        }
        config["app_scope"] = scopes;
        // IAM's current record omits the accepted webhook destination. Never label
        // the requested URL as accepted while destination approval is pending.
        config.as_object_mut().unwrap().remove("webhook_url");
        Ok(config)
    }
    async fn snapshot(&self, app: &str) -> Result<Value> {
        let record = self.client.application(app).await.map_err(map_error)?;
        let configuration = self.configuration(app, &record).await?;
        // A locally published request cannot substitute for IAM's current approval.
        // Null means the authority no longer confirms a valid publication plan.
        let publication:Option<String>=sqlx::query_scalar("SELECT id FROM publication_requests WHERE plane='production' AND app_id=? AND revision=? AND id=? AND state IN ('activating','published')")
            .bind(app).bind(record["configuration_revision"].as_i64().unwrap_or(-1))
            .bind(record["publication_request_id"].as_str().unwrap_or(""))
            .fetch_optional(&self.db).await?;
        Ok(
            json!({"app_id":app,"configuration_revision":record["configuration_revision"],"iam_revision":record["iam_revision"],"visibility":record["visibility"],"availability":if record["availability"]=="verified"{"active"}else{"disabled"},"effective_configuration":configuration,"publication_request_id":publication,"credential_version":record["credential_version"]}),
        )
    }
}
fn decode<T: serde::de::DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v)
        .map_err(|_| Error::bad("Honeycomb configuration does not match the IAM management schema"))
}
fn production(environment: Option<&str>) -> Result<()> {
    if environment.is_some() {
        Err(Error::unavailable(
            "This IAM management operation is not available in a testing environment",
        ))
    } else {
        Ok(())
    }
}
fn map_error(error: silicon_iam_client::Error) -> Error {
    match error {
        silicon_iam_client::Error::Api(api) if api.code == "step_up_required" => Error::new(
            axum::http::StatusCode::FORBIDDEN,
            "step_up_required",
            "IAM requires verified-channel step-up for this application action.",
        ),
        silicon_iam_client::Error::Api(api) if api.status == 401 => Error::unauthorized(),
        silicon_iam_client::Error::Api(api) if api.status == 403 => Error::forbidden(),
        silicon_iam_client::Error::Api(api) if api.status == 409 => Error::conflict(
            "IAM rejected the revision or operation state; refresh accepted IAM state before a new operation.",
        ),
        _ => Error::unavailable(
            "IAM management could not complete the request; retry its original operation.",
        ),
    }
}
#[async_trait]
impl Management for IamManagement {
    async fn verify_testing_application(
        &self,
        authorization: &str,
    ) -> Result<crate::integration::TestingApplicationIdentity> {
        self.testing_verify_application(authorization).await
    }
    async fn testing_application_environment_ids(
        &self,
        authorization: &str,
    ) -> Result<Vec<String>> {
        self.testing_environment_ids(authorization).await
    }
    async fn application_lifecycle(
        &self,
        operation: &Value,
        authorization: &str,
        attachment_key: Option<&str>,
    ) -> Result<Value> {
        self.testing_apply_application(operation, authorization, attachment_key)
            .await
    }
    async fn recover_testing_application_credential(
        &self,
        environment: &str,
        authorization: &str,
    ) -> Result<Value> {
        self.testing_recover_credential(environment, authorization)
            .await
    }
    async fn finalize_lifecycle(&self, operation: &Value) -> Result<Value> {
        self.testing_finalize(operation).await
    }
    async fn publication_plan(
        &self,
        request: &Value,
        actor: &str,
        environment: Option<&str>,
    ) -> Result<Value> {
        self.publication_plan_sdk(request, actor, environment).await
    }
    async fn review_decision(
        &self,
        operation: &Value,
        actor: &str,
        environment: Option<&str>,
    ) -> Result<Value> {
        self.publication_decision_sdk(operation, actor, environment)
            .await
    }
    async fn activate_publication(
        &self,
        operation: &Value,
        actor: &str,
        environment: Option<&str>,
    ) -> Result<Value> {
        self.publication_activate_sdk(operation, actor, environment)
            .await
    }
    async fn review_eligibility(
        &self,
        plan: &str,
        provider: &str,
        actor: &str,
        environment: Option<&str>,
    ) -> Result<bool> {
        self.publication_eligibility_sdk(plan, provider, actor, environment)
            .await
    }
    async fn review_notification_recipients(
        &self,
        plan: &str,
        provider: &str,
    ) -> Result<Vec<String>> {
        self.publication_recipients_sdk(plan, provider).await
    }
    async fn notification_recipients(&self, org: &str) -> Result<Vec<String>> {
        self.organization_recipients_sdk(org).await
    }
    async fn service_lifecycle(&self, app: &str, operation: &Value, _actor: &str) -> Result<Value> {
        self.participants.apply(app, operation).await
    }
    async fn retention_lifecycle(&self, app: &str, operation: &Value) -> Result<Value> {
        if self.identity_app_id.as_deref() == Some(app) {
            if operation["action"] == "retire-applications" {
                self.testing_retire(operation).await
            } else {
                self.testing_apply(operation, None).await
            }
        } else {
            self.participants.apply(app, operation).await
        }
    }
    async fn scope_catalog(&self, org: &str, provider: Option<&str>) -> Result<Value> {
        self.client
            .scope_catalog_for(org, provider)
            .await
            .map_err(map_error)
    }
    async fn configure(&self, o: &Value, actor: &str, environment: Option<&str>) -> Result<Value> {
        if let Some(environment) = environment {
            return self.testing_configure(o, actor, environment).await;
        }
        let id = o["operation_id"]
            .as_str()
            .ok_or_else(|| Error::bad("Missing operation ID"))?;
        let app = o["app_id"]
            .as_str()
            .ok_or_else(|| Error::bad("Missing app ID"))?;
        let c = &o["configuration"];
        let body = json!({"operation_id":id,"configuration_revision":o["configuration_revision"],"expected_iam_revision":o["expected_iam_revision"],"app_id":app,"org_id":c["org_id"],"name":c["name"],"logo_url":c["logo_url"],"base_url":c["base_url"],"visibility":o["visibility"],"availability":"active","publication_approved":false,"webhook":{"url":c["webhook_url"],"secret":o["webhook_secret"],"scope":c["webhook_scope"]},"app_scope":c["app_scope"],"obo_endpoints":c["obo_endpoints"],"obo_review_message":c["obo_review_message"],"testing_idle_days":c["testing_idle_days"].as_u64().unwrap_or(30)});
        let body = self.saved(id, "configure", app, body).await?;
        self.execute(id, "configure", app, body, actor, None).await
    }
    async fn rotate_secret(
        &self,
        o: &Value,
        actor: &str,
        step_up: Option<&str>,
        environment: Option<&str>,
    ) -> Result<Value> {
        if let Some(environment) = environment {
            return self
                .testing_rotate_secret(o, actor, step_up, environment)
                .await;
        }
        let id = o["operation_id"]
            .as_str()
            .ok_or_else(|| Error::bad("Missing operation ID"))?;
        let app = o["app_id"]
            .as_str()
            .ok_or_else(|| Error::bad("Missing app ID"))?;
        let body = self
            .saved(
                id,
                "secret.rotate",
                app,
                json!({"operation_id":id,"expected_iam_revision":o["expected_iam_revision"]}),
            )
            .await?;
        self.execute(id, "secret.rotate", app, body, actor, step_up)
            .await
    }
    async fn webhook_state(&self, app: &str, environment: Option<&str>) -> Result<Value> {
        production(environment)?;
        let record = self.client.application(app).await.map_err(map_error)?;
        if record["app_id"] != app {
            return Err(Error::unavailable("IAM returned another application"));
        }
        Ok(
            json!({"app_id":app,"application_id":record["application_id"],"iam_revision":record["iam_revision"],"configuration_revision":record["configuration_revision"],"pending_endpoint_id":record["pending_webhook_endpoint_id"]}),
        )
    }
    async fn webhook_mutation(
        &self,
        o: &Value,
        actor: &str,
        step_up: Option<&str>,
        environment: Option<&str>,
    ) -> Result<Value> {
        production(environment)?;
        let id = o["operation_id"]
            .as_str()
            .ok_or_else(|| Error::bad("Missing operation ID"))?;
        let app = o["app_id"]
            .as_str()
            .ok_or_else(|| Error::bad("Missing app ID"))?;
        let kind = o["kind"]
            .as_str()
            .ok_or_else(|| Error::bad("Missing webhook action"))?;
        let mut body =
            json!({"operation_id":id,"expected_iam_revision":o["expected_iam_revision"]});
        match kind {
            "webhook.approve" => body["pending_endpoint_id"] = o["pending_endpoint_id"].clone(),
            "webhook.rotate" => body["webhook_secret"] = o["webhook_secret"].clone(),
            _ => return Err(Error::bad("Unknown webhook action")),
        }
        let body = self.saved(id, kind, app, body).await?;
        self.execute(id, kind, app, body, actor, step_up).await
    }
    async fn operation_result(
        &self,
        id: &str,
        actor: &str,
        environment: Option<&str>,
    ) -> Result<Value> {
        if let Some(environment) = environment {
            return self.testing_operation_result(id, actor, environment).await;
        }
        let row = sqlx::query(
            "SELECT kind,app_id,encrypted_body FROM management_requests WHERE operation_id=?",
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(Error::missing)?;
        let body = serde_json::from_str(&crate::decrypt(
            &self.encryption_key,
            &row.get::<String, _>("encrypted_body"),
        )?)
        .map_err(|e| anyhow::anyhow!(e))?;
        self.execute(
            id,
            &row.get::<String, _>("kind"),
            &row.get::<String, _>("app_id"),
            body,
            actor,
            None,
        )
        .await
    }
    async fn application_snapshot(&self, app: &str, environment: Option<&str>) -> Result<Value> {
        if let Some(environment) = environment {
            return self.testing_snapshot_plane(app, environment).await;
        }
        self.snapshot(app).await
    }
    async fn application_state(
        &self,
        app: &str,
        _: &str,
        environment: Option<&str>,
    ) -> Result<Value> {
        if let Some(environment) = environment {
            return self.testing_snapshot(app, environment).await;
        }
        self.snapshot(app).await
    }
    async fn lifecycle(&self, operation: &Value, actor: &str) -> Result<Value> {
        self.testing_apply_user(operation, actor).await
    }
}

/// Separate receiver for IAM 1.10 service notifications (not application webhooks).
pub fn notification_router(state: crate::State, key: SecretString) -> axum::Router {
    axum::Router::new().route(
        "/management/webhook/",
        axum::routing::post(
            move |headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                let state = state.clone();
                let key = key.clone();
                async move { notification(&state, &key, &headers, &body).await }
            },
        )
        .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024)),
    )
}
pub async fn notification(
    s: &crate::State,
    key: &SecretString,
    headers: &axum::http::HeaderMap,
    body: &[u8],
) -> Result<axum::Json<Value>> {
    let signatures: Vec<_> = headers
        .get_all("x-iam-management-signature")
        .iter()
        .collect();
    if signatures.len() != 1 {
        return Err(Error::unauthorized());
    }
    let signature = signatures[0].to_str().map_err(|_| Error::unauthorized())?;
    silicon_iam_client::honeycomb::verify_notification(
        key,
        signature,
        body,
        now(),
        std::time::Duration::from_secs(300),
    )
    .map_err(|_| Error::unauthorized())?;
    let event: Value =
        serde_json::from_slice(body).map_err(|_| Error::bad("Invalid management notification"))?;
    let event_id = event["event_id"]
        .as_str()
        .filter(|s| uuid::Uuid::parse_str(s).is_ok())
        .ok_or_else(|| Error::bad("Invalid management event ID"))?;
    let resource = event["resource_id"]
        .as_str()
        .ok_or_else(|| Error::bad("Missing management resource"))?;
    let revision = event["revision"]
        .as_i64()
        .filter(|r| *r > 0)
        .ok_or_else(|| Error::bad("Invalid management revision"))?;
    let application = event["event_type"]
        .as_str()
        .is_some_and(|t| t.starts_with("application."));
    if application && !event["environment_id"].is_null() {
        return Err(Error::unavailable(
            "IAM test-application management snapshots are not available yet",
        ));
    }
    let normalized = json!({"event_id":event_id,"event_type":if application{"honeycomb.application.changed.v1"}else{"management.other.v1"},"aggregate":{"type":if application{"application"}else{"management"},"id":resource,"version":revision},"data":{"app_id":resource}});
    let received = crate::reconciliation::receive(s, "production", &normalized).await?;
    Ok(axum::Json(json!({"received":true,"duplicate":!received})))
}

#[cfg(test)]
mod participant_tests {
    use super::*;

    #[tokio::test]
    async fn unconfigured_participants_never_inherit_identity_authority() {
        let db = crate::database("sqlite::memory:").await.unwrap();
        let adapter = IamManagement::new(
            "https://identity.example",
            format!("hck_{}", "a".repeat(43)),
            db,
            [7; 32],
        )
        .unwrap();
        for app in [
            "vendor>storage",
            "vendor>identity",
            "tos>briefcase",
            "tos>iam",
        ] {
            assert!(
                adapter
                    .service_lifecycle(app, &json!({}), "actor")
                    .await
                    .is_err()
            );
            assert!(adapter.retention_lifecycle(app, &json!({})).await.is_err());
        }
    }
}
