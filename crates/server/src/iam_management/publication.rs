//! Official IAM publication contracts; exact request bodies remain encrypted for replay.
use super::{IamManagement, decode, map_error, production};
use crate::error::{Error, Result};
use secrecy::SecretString;
use serde_json::{Value, json};
use silicon_iam_client::{IdempotencyKey, Mutation, models};
use sqlx::Row;
use std::collections::BTreeSet;
use uuid::Uuid;

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .ok_or_else(|| Error::bad("Publication operation is missing an identity"))
}
fn uuid(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(|_| Error::bad("Invalid publication identity"))
}
fn mutation(id: &str) -> Result<Mutation> {
    uuid(id)?;
    Ok(Mutation::with_key(IdempotencyKey::parse(id).map_err(
        |_| Error::bad("Invalid publication operation ID"),
    )?))
}
fn value<T: serde::Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|e| anyhow::anyhow!(e).into())
}
impl IamManagement {
    pub(super) async fn publication_plan_sdk(
        &self,
        request: &Value,
        actor: &str,
        environment: Option<&str>,
    ) -> Result<Value> {
        production(environment)?;
        let id = string(request, "request_id")?;
        let app = string(request, "app_id")?;
        let c = &request["configuration"];
        let encrypted: String = sqlx::query_scalar(
            "SELECT webhook_secret FROM applications WHERE plane='production' AND app_id=?",
        )
        .bind(app)
        .fetch_one(&self.db)
        .await?;
        let secret = crate::decrypt(&self.encryption_key, &encrypted)?;
        let configuration = json!({"operation_id":id,"expected_iam_revision":0,"configuration_revision":request["configuration_revision"],"app_id":app,"org_id":c["org_id"],"name":c["name"],"logo_url":c["logo_url"],"base_url":c["base_url"],"visibility":"public","availability":"active","publication_approved":true,"webhook":{"url":c["webhook_url"],"secret":secret,"scope":c["webhook_scope"]},"app_scope":c["app_scope"],"obo_endpoints":c["obo_endpoints"],"obo_review_message":c["obo_review_message"],"testing_idle_days":c["testing_idle_days"].as_i64().unwrap_or(30)});
        let body=self.saved(id,"publication.plan",app,json!({"request_id":id,"app_id":app,"configuration_revision":request["configuration_revision"],"visibility":"public","configuration":configuration})).await?;
        let input = decode::<models::HoneycombPublicationPlan>(body)?;
        let receipt = self
            .client
            .publication_plan(
                &SecretString::from(actor.to_owned()),
                &input,
                &mutation(id)?,
            )
            .await
            .map_err(map_error)?;
        if receipt.request_id != input.request_id
            || receipt.app_id != input.app_id
            || receipt.configuration_revision != input.configuration_revision
            || receipt.visibility != "public"
            || receipt.state != "accepted"
        {
            return Err(Error::unavailable(
                "IAM returned a different publication plan",
            ));
        }
        value(receipt)
    }
    pub(super) async fn publication_decision_sdk(
        &self,
        request: &Value,
        actor: &str,
        environment: Option<&str>,
    ) -> Result<Value> {
        production(environment)?;
        let id = string(request, "operation_id")?;
        let app = string(request, "app_id")?;
        let mut body = request.clone();
        if body["reason"].as_str().is_some_and(|s| s.trim().is_empty()) {
            body["reason"] = Value::Null;
        }
        let body = self.saved(id, "publication.decision", app, body).await?;
        let input = decode::<models::HoneycombPublicationDecision>(body.clone())?;
        let receipt = self
            .client
            .publication_decision(
                &SecretString::from(actor.to_owned()),
                &input,
                &mutation(id)?,
            )
            .await
            .map_err(map_error)?;
        if receipt.decision_id != input.operation_id
            || receipt.operation_id != Some(input.operation_id)
        {
            return Err(Error::unavailable(
                "IAM returned a different publication decision",
            ));
        }
        let receipt = value(receipt)?;
        for key in [
            "request_id",
            "plan_id",
            "app_id",
            "configuration_revision",
            "provider",
            "scopes",
            "decision",
        ] {
            if receipt[key] != body[key] {
                return Err(Error::unavailable(
                    "IAM returned a different publication decision",
                ));
            }
        }
        Ok(receipt)
    }
    pub(super) async fn publication_activate_sdk(
        &self,
        request: &Value,
        actor: &str,
        environment: Option<&str>,
    ) -> Result<Value> {
        production(environment)?;
        let id = string(request, "operation_id")?;
        let app = string(request, "app_id")?;
        let request_id = string(request, "request_id")?;
        let saved=sqlx::query("SELECT encrypted_body FROM management_requests WHERE operation_id=? AND kind='publication.plan' AND app_id=?")
            .bind(request_id).bind(app).fetch_optional(&self.db).await?.ok_or_else(||Error::conflict("Publication has no immutable IAM configuration"))?;
        let plan: Value = serde_json::from_str(&crate::decrypt(
            &self.encryption_key,
            &saved.get::<String, _>("encrypted_body"),
        )?)
        .map_err(|e| anyhow::anyhow!(e))?;
        if plan["request_id"] != request_id
            || plan["app_id"] != app
            || plan["configuration_revision"] != request["configuration_revision"]
        {
            return Err(Error::conflict(
                "Publication configuration no longer matches its plan",
            ));
        }
        let mut config = plan["configuration"].clone();
        config["operation_id"] = json!(id);
        config["expected_iam_revision"] = request["expected_iam_revision"].clone();
        let body = json!({"operation_id":id,"request_id":request_id,"plan_id":request["plan_id"],"app_id":app,"configuration_revision":request["configuration_revision"],"expected_iam_revision":request["expected_iam_revision"],"visibility":"public","configuration":config,"decision_ids":request["decisions"],"configuration_operations":request["configuration_operations"]});
        let body = self.saved(id, "publication.activate", app, body).await?;
        let input = decode::<models::HoneycombPublicationActivation>(body.clone())?;
        let receipt = self
            .client
            .publication_activate(
                &SecretString::from(actor.to_owned()),
                &input,
                &mutation(id)?,
            )
            .await
            .map_err(map_error)?;
        let mut receipt = value(receipt)?;
        for key in [
            "operation_id",
            "request_id",
            "plan_id",
            "app_id",
            "configuration_revision",
        ] {
            if receipt[key] != body[key] {
                return Err(Error::unavailable(
                    "IAM returned a different publication activation",
                ));
            }
        }
        if receipt["state"] != "accepted"
            || receipt["publication_request_id"] != request_id
            || receipt["iam_revision"]
                .as_i64()
                .is_none_or(|r| r <= input.expected_iam_revision)
        {
            return Err(Error::unavailable(
                "IAM has not accepted this publication activation",
            ));
        }
        let record = &receipt["effective_configuration"];
        if record["visibility"] != "public"
            || record["availability"] != "verified"
            || record["configuration_revision"] != body["configuration_revision"]
            || record["publication_request_id"] != request_id
        {
            return Err(Error::unavailable(
                "IAM returned an unapproved public configuration",
            ));
        }
        let effective = self.configuration(app, record).await?;
        receipt["effective_configuration"] = effective;
        receipt["visibility"] = json!("public");
        Ok(receipt)
    }
    pub(super) async fn publication_eligibility_sdk(
        &self,
        plan: &str,
        provider: &str,
        actor: &str,
        environment: Option<&str>,
    ) -> Result<bool> {
        production(environment)?;
        let plan = uuid(plan)?;
        let result = self
            .client
            .reviewer_eligibility(plan, provider, &SecretString::from(actor.to_owned()))
            .await
            .map_err(map_error)?;
        if result.plan_id != plan || result.provider != provider {
            return Err(Error::unavailable(
                "IAM returned another review gate's eligibility",
            ));
        }
        Ok(result.eligible)
    }
    pub(super) async fn publication_recipients_sdk(
        &self,
        plan: &str,
        provider: &str,
    ) -> Result<Vec<String>> {
        let plan = uuid(plan)?;
        let mut cursor: Option<String> = None;
        let mut seen = BTreeSet::new();
        let mut emails = BTreeSet::new();
        loop {
            let page = self
                .client
                .notification_recipients(plan, provider, cursor.as_deref(), Some(50))
                .await
                .map_err(map_error)?;
            if page.plan_id != plan || page.provider != provider {
                return Err(Error::unavailable(
                    "IAM returned another review gate's recipients",
                ));
            }
            for recipient in page.recipients {
                emails.insert(recipient.email);
            }
            if emails.len() > 50 {
                return Err(Error::unavailable("Notification recipient limit exceeded"));
            }
            let Some(next) = page.next_cursor else {
                return Ok(emails.into_iter().collect());
            };
            if !seen.insert(next.clone()) {
                return Err(Error::unavailable("IAM repeated a recipient cursor"));
            }
            cursor = Some(next);
        }
    }
    pub(super) async fn organization_recipients_sdk(&self, org: &str) -> Result<Vec<String>> {
        let mut cursor: Option<String> = None;
        let mut seen = BTreeSet::new();
        let mut emails = BTreeSet::new();
        loop {
            let page = self
                .client
                .organization_recipients(org, cursor.as_deref(), Some(50))
                .await
                .map_err(map_error)?;
            if page.org_id != org {
                return Err(Error::unavailable(
                    "IAM returned another organization's recipients",
                ));
            }
            for recipient in page.recipients {
                emails.insert(recipient.email);
            }
            if emails.len() > 50 {
                return Err(Error::unavailable("Notification recipient limit exceeded"));
            }
            let Some(next) = page.next_cursor else {
                return Ok(emails.into_iter().collect());
            };
            if !seen.insert(next.clone()) {
                return Err(Error::unavailable("IAM repeated a recipient cursor"));
            }
            cursor = Some(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path, query_param},
    };

    const ACTOR: &str = "oat_fixture_actor";
    const APP: &str = "app";
    async fn setup() -> (MockServer, IamManagement, Value) {
        let server = MockServer::start().await;
        let db = crate::database("sqlite::memory:").await.unwrap();
        let config = json!({"org_id":"vendor","app_id":"app","name":"Example","description":"Catalog description stays local","webhook_url":"https://example.com/webhook/","webhook_scope":["membership"],"app_scope":{"iam":["self.identity.read"],"external":[]},"obo_endpoints":[],"testing_idle_days":30});
        sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,config,webhook_secret,created_at,updated_at) VALUES('production',?,'vendor','Example','Catalog description stays local',?,?,1,1)")
            .bind(APP).bind(config.to_string()).bind(crate::encrypt(&[7;32],"fixture-webhook-secret-keep-private").unwrap()).execute(&db).await.unwrap();
        let adapter = IamManagement::new(
            &server.uri(),
            format!("hck_{}", "a".repeat(43)),
            db,
            [7; 32],
        )
        .unwrap();
        (
            server,
            adapter,
            json!({"request_id":Uuid::new_v4(),"app_id":APP,"configuration_revision":1,"configuration":config,"visibility":"public"}),
        )
    }
    async fn operation_row(adapter: &IamManagement, id: &str) {
        sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,'production','actor',?,'publication.request',?,'test',1,1)")
            .bind(id).bind(id).bind(APP).execute(&adapter.db).await.unwrap();
    }
    async fn plan_mock(server: &MockServer, request: &Value, plan: Uuid, count: u64) {
        Mock::given(method("POST")).and(path("/api/v1/honeycomb/applications/app/publication-plans"))
            .and(header("x-honeycomb-actor-token",ACTOR)).and(header("idempotency-key",request["request_id"].as_str().unwrap()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"state":"accepted","request_id":request["request_id"],"plan_id":plan,"app_id":APP,"configuration_revision":1,"visibility":"public","gates":[{"provider":"honeycomb","scopes":[]}],"reused_approvals":[]})))
            .expect(count).mount(server).await;
    }
    #[tokio::test]
    async fn publication_wire_reuses_encrypted_plan_config_and_normalizes_activation() {
        let (server, adapter, request) = setup().await;
        let plan = Uuid::new_v4();
        operation_row(&adapter, request["request_id"].as_str().unwrap()).await;
        plan_mock(&server, &request, plan, 2).await;
        adapter
            .publication_plan_sdk(&request, ACTOR, None)
            .await
            .unwrap();
        sqlx::query("UPDATE applications SET webhook_secret=?")
            .bind(crate::encrypt(&[7; 32], "newer-webhook-secret-not-in-plan").unwrap())
            .execute(&adapter.db)
            .await
            .unwrap();
        adapter
            .publication_plan_sdk(&request, ACTOR, None)
            .await
            .unwrap();
        let calls = server.received_requests().await.unwrap();
        assert_eq!(calls[0].body, calls[1].body);
        let wire: Value = serde_json::from_slice(&calls[0].body).unwrap();
        assert_eq!(
            wire["configuration"]["webhook"]["secret"],
            "fixture-webhook-secret-keep-private"
        );
        assert!(wire["configuration"].get("description").is_none());
        assert!(wire["configuration"].get("local_app_id").is_none());
        let encrypted: String =
            sqlx::query_scalar("SELECT encrypted_body FROM management_requests")
                .fetch_one(&adapter.db)
                .await
                .unwrap();
        assert!(!encrypted.contains("fixture-webhook"));
        let operation = Uuid::new_v4();
        operation_row(&adapter, &operation.to_string()).await;
        let decision = Uuid::new_v4();
        let record = json!({"app_id":APP,"org_id":"vendor","app_name":"Accepted name","app_logo":null,"base_url":null,"configuration_revision":1,"iam_revision":4,"visibility":"public","availability":"verified","publication_request_id":request["request_id"],"app_scope":{"iam":["self.identity.read"],"external":[]},"effective_scopes":[{"scope":"self.identity.read","basis":"policy"}],"obo_endpoints":[],"webhook_scope":["membership"],"testing_idle_days":30});
        let receipt = json!({"state":"accepted","operation_id":operation,"request_id":request["request_id"],"publication_request_id":request["request_id"],"plan_id":plan,"app_id":APP,"configuration_revision":1,"iam_revision":4,"visibility":"public","effective_configuration":record});
        Mock::given(method("POST"))
            .and(path(
                "/api/v1/honeycomb/applications/app/publication-activations",
            ))
            .and(header("idempotency-key", operation.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(receipt))
            .expect(1)
            .mount(&server)
            .await;
        let activation = json!({"operation_id":operation,"request_id":request["request_id"],"plan_id":plan,"app_id":APP,"configuration_revision":1,"expected_iam_revision":3,"configuration":request["configuration"],"decisions":[decision],"configuration_operations":[]});
        let result = adapter
            .publication_activate_sdk(&activation, ACTOR, None)
            .await
            .unwrap();
        assert_eq!(result["visibility"], "public");
        assert_eq!(result["effective_configuration"]["name"], "Accepted name");
        assert_eq!(
            result["effective_configuration"]["description"],
            "Catalog description stays local"
        );
        let calls = server.received_requests().await.unwrap();
        let activation_wire: Value = serde_json::from_slice(&calls.last().unwrap().body).unwrap();
        assert_eq!(activation_wire["decision_ids"], json!([decision]));
        assert!(activation_wire.get("decisions").is_none());
        let mut normalized = activation_wire["configuration"].clone();
        normalized["operation_id"] = wire["configuration"]["operation_id"].clone();
        normalized["expected_iam_revision"] =
            wire["configuration"]["expected_iam_revision"].clone();
        assert_eq!(normalized, wire["configuration"]);
        assert!(
            adapter
                .publication_plan_sdk(&request, ACTOR, Some("test-key"))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn publication_decision_rejects_wrong_identity_and_preserves_exact_scopes() {
        let (server, adapter, request) = setup().await;
        let id = Uuid::new_v4();
        operation_row(&adapter, &id.to_string()).await;
        let operation = json!({"operation_id":id,"request_id":request["request_id"],"plan_id":Uuid::new_v4(),"app_id":APP,"configuration_revision":1,"provider":"storage","scopes":["obo:storage:files.read"],"decision":"approve","reason":""});
        let mut receipt = operation.clone();
        receipt["state"] = json!("accepted");
        receipt["reason"] = Value::Null;
        receipt["decision_id"] = json!(Uuid::new_v4());
        Mock::given(method("POST"))
            .and(path(
                "/api/v1/honeycomb/applications/app/publication-decisions",
            ))
            .and(header("idempotency-key", id.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(receipt.clone()))
            .expect(1)
            .mount(&server)
            .await;
        assert!(
            adapter
                .publication_decision_sdk(&operation, ACTOR, None)
                .await
                .is_err()
        );
        server.reset().await;
        receipt["decision_id"] = json!(id);
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(receipt))
            .expect(1)
            .mount(&server)
            .await;
        let accepted = adapter
            .publication_decision_sdk(&operation, ACTOR, None)
            .await
            .unwrap();
        assert_eq!(accepted["scopes"], operation["scopes"]);
        let calls = server.received_requests().await.unwrap();
        let wire: Value = serde_json::from_slice(&calls[0].body).unwrap();
        assert!(wire.get("reason").is_none());
    }
    #[tokio::test]
    async fn publication_recipients_are_plan_bound_paginated_and_fail_closed() {
        let (server, adapter, _) = setup().await;
        let plan = Uuid::new_v4();
        let cursor = Uuid::new_v4();
        let endpoint =
            format!("/api/v1/honeycomb/publication-plans/{plan}/notification-recipients");
        Mock::given(method("GET")).and(path(&endpoint)).and(query_param("provider","owners"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"plan_id":plan,"provider":"owners","recipients":[{"principal_id":cursor,"email":"owner@example.com"}],"next_cursor":cursor}))).with_priority(2).mount(&server).await;
        Mock::given(method("GET")).and(path(&endpoint)).and(query_param("after",cursor.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"plan_id":plan,"provider":"owners","recipients":[{"principal_id":Uuid::new_v4(),"email":"second@example.com"}],"next_cursor":null}))).with_priority(1).mount(&server).await;
        assert_eq!(
            adapter
                .publication_recipients_sdk(&plan.to_string(), "owners")
                .await
                .unwrap(),
            vec!["owner@example.com", "second@example.com"]
        );
        server.reset().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"plan_id":Uuid::new_v4(),"provider":"owners","recipients":[],"next_cursor":null}))).mount(&server).await;
        assert!(
            adapter
                .publication_recipients_sdk(&plan.to_string(), "owners")
                .await
                .is_err()
        );
        server.reset().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"org_id":"another","recipients":[],"next_cursor":null})),
            )
            .mount(&server)
            .await;
        assert!(adapter.organization_recipients_sdk("vendor").await.is_err());
    }
    #[tokio::test]
    async fn publication_eligibility_requires_matching_plan_and_provider() {
        let (server, adapter, _) = setup().await;
        let plan = Uuid::new_v4();
        Mock::given(method("GET")).and(header("x-honeycomb-actor-token",ACTOR)).respond_with(ResponseTemplate::new(200).set_body_json(json!({"plan_id":plan,"provider":"honeycomb","actor_id":Uuid::new_v4(),"eligible":false}))).mount(&server).await;
        assert!(
            !adapter
                .publication_eligibility_sdk(&plan.to_string(), "honeycomb", ACTOR, None)
                .await
                .unwrap()
        );
        assert!(
            adapter
                .publication_eligibility_sdk(&plan.to_string(), "iam", ACTOR, None)
                .await
                .is_err()
        );
    }
}
