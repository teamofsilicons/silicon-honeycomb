//! Durable IAM phases beneath one shared Honeycomb lifecycle operation.
use super::{IamManagement, decode, map_error};
use crate::{
    error::{Error, Result},
    now,
};
use secrecy::SecretString;
use serde_json::{Value, json};
use silicon_iam_client::{
    EnvironmentKey, IdempotencyKey, Mutation, honeycomb::ManagementAuthority, models,
};
use sqlx::Row;
use uuid::Uuid;

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .ok_or_else(|| Error::bad("Missing lifecycle identity"))
}
fn positive(value: &Value, field: &str) -> Result<i64> {
    value[field]
        .as_i64()
        .filter(|v| *v > 0)
        .ok_or_else(|| Error::bad("Invalid lifecycle version"))
}
fn failure() -> Error {
    Error::unavailable("IAM did not confirm the exact testing lifecycle phase")
}

impl IamManagement {
    pub(super) async fn testing_apply_user(&self, operation: &Value, actor: &str) -> Result<Value> {
        if !actor.is_empty() {
            let actor = SecretString::from(actor.to_owned());
            self.testing_apply(operation, Some(&ManagementAuthority::Actor(&actor)))
                .await
        } else if operation["reason"] == "inactivity"
            || !matches!(operation["action"].as_str(), Some("import" | "rotate-key"))
        {
            self.testing_apply(operation, None).await
        } else {
            let key = operation["authority_testing_key"]
                .as_str()
                .or_else(|| operation["testing_key"].as_str())
                .ok_or_else(Error::unauthorized)?;
            let key = EnvironmentKey::new(key.to_owned()).map_err(|_| Error::unauthorized())?;
            self.testing_apply(operation, Some(&ManagementAuthority::Environment(&key)))
                .await
        }
    }

    /// Application credentials stay request-scoped in the SDK authority, never in a phase body.
    pub(super) async fn testing_apply(
        &self,
        operation: &Value,
        authority: Option<&ManagementAuthority<'_>>,
    ) -> Result<Value> {
        let action = text(operation, "action")?;
        if action == "retire-applications" {
            return self.testing_retire(operation).await;
        }
        let receipt = self
            .testing_phase(operation, "prepare", action, authority)
            .await?;
        let mut result = normalized(operation);
        result["requires_finalization"] = json!(matches!(
            action,
            "prepare" | "import" | "clean" | "restore" | "rotate-key"
        ));
        result["iam_revision"] = receipt["iam_revision"].clone();
        if action == "import" {
            result["imports"] = map_imports(operation, &receipt)?;
            if let Some(secret) = receipt["app_secret"].as_str() {
                if receipt["app_id"] != operation["snapshot"]["root_app_id"] {
                    return Err(failure());
                }
                result["application_credential"] = json!({"app_id":receipt["app_id"],"environment_id":operation["environment_id"],"app_secret":secret});
            }
        }
        Ok(result)
    }

    /// The coordinator invokes this only after every configured participant has acknowledged.
    pub(super) async fn testing_finalize(&self, operation: &Value) -> Result<Value> {
        let initial: Option<String> = sqlx::query_scalar(
            "SELECT receipt FROM iam_testing_phases WHERE parent_operation=? AND phase='prepare'",
        )
        .bind(text(operation, "operation_id")?)
        .fetch_optional(&self.db)
        .await?
        .flatten();
        let initial: Value =
            serde_json::from_str(&initial.ok_or_else(failure)?).map_err(|_| failure())?;
        let action =
            if initial["environment"]["state"] == "active" && operation["action"] == "import" {
                "activate-apps"
            } else {
                "activate"
            };
        let receipt = self
            .testing_phase(operation, "activate", action, None)
            .await?;
        if receipt["environment"]["state"] != "active" {
            return Err(failure());
        }
        let mut result = normalized(operation);
        result["iam_revision"] = receipt["iam_revision"].clone();
        Ok(result)
    }

    /// Configuration callers must collect the application's matching participant
    /// receipt before invoking this exact-app activation.
    pub(super) async fn testing_activate_application(&self, operation: &Value) -> Result<Value> {
        let receipt = self
            .testing_phase(operation, "application-activate", "activate-apps", None)
            .await?;
        if receipt["environment"]["state"] != "active" {
            return Err(failure());
        }
        Ok(normalized(operation))
    }

    async fn testing_phase(
        &self,
        operation: &Value,
        phase: &str,
        action: &str,
        authority: Option<&ManagementAuthority<'_>>,
    ) -> Result<Value> {
        let parent = text(operation, "operation_id")?;
        let environment = text(operation, "environment_id")?;
        let environment_uuid =
            Uuid::parse_str(environment).map_err(|_| Error::bad("Invalid environment identity"))?;
        let existing = sqlx::query("SELECT operation_id,encrypted_body,receipt FROM iam_testing_phases WHERE parent_operation=? AND phase=?")
            .bind(parent).bind(phase).fetch_optional(&self.db).await?;
        if let Some(receipt) = existing
            .as_ref()
            .and_then(|r| r.get::<Option<String>, _>("receipt"))
        {
            let receipt = serde_json::from_str(&receipt).map_err(|_| failure())?;
            let body: Value = serde_json::from_str(&crate::decrypt(
                &self.encryption_key,
                &existing
                    .as_ref()
                    .unwrap()
                    .get::<String, _>("encrypted_body"),
            )?)
            .map_err(|_| failure())?;
            validate_phase(operation, &body, &receipt)?;
            return Ok(receipt);
        }
        let body = if let Some(existing) = existing {
            serde_json::from_str(&crate::decrypt(
                &self.encryption_key,
                &existing.get::<String, _>("encrypted_body"),
            )?)
            .map_err(|_| failure())?
        } else {
            let before = match self.client.testing_environment(environment_uuid).await {
                Ok(record) => record,
                Err(silicon_iam_client::Error::Api(error))
                    if error.status == 404 && action == "prepare" =>
                {
                    json!({"iam_revision":0,"generation":1,"key_version":1})
                }
                Err(error) => return Err(map_error(error)),
            };
            if before["iam_revision"] != 0 && before["environment_id"] != environment {
                return Err(failure());
            }
            let desired_generation = positive(operation, "generation")?;
            let desired_key = positive(operation, "key_version")?;
            let expected_generation = if action == "clean" {
                desired_generation - 1
            } else {
                desired_generation
            };
            let expected_key = if action == "rotate-key" {
                desired_key - 1
            } else {
                desired_key
            };
            if before["generation"] != expected_generation || before["key_version"] != expected_key
            {
                return Err(failure());
            }
            let id = Uuid::new_v4().to_string();
            let mut body = json!({"operation_id":id,"environment_id":environment,"expected_iam_revision":before["iam_revision"],"generation":expected_generation,"operation":action});
            if before["iam_revision"] != 0 {
                body["expected_key_version"] = json!(expected_key);
            }
            if action == "prepare" {
                body["org_id"] = operation["org_id"].clone();
                body["name"] = operation["name"].clone();
                body["description"] = operation["description"].clone();
            }
            if (action == "prepare" && before["iam_revision"] == 0) || action == "rotate-key" {
                body["testing_key"] = operation["testing_key"].clone();
                body["key_version"] = json!(desired_key);
            }
            if action == "import" {
                let root = text(&operation["snapshot"], "root_app_id")?;
                let graph = operation["snapshot"]["graph"].as_array().ok_or_else(|| {
                    Error::conflict("Import operation is missing its full IAM source graph")
                })?;
                let mut pins = serde_json::Map::new();
                for source in graph {
                    pins.insert(
                        text(source, "app_id")?.into(),
                        json!(positive(source, "source_iam_revision")?),
                    );
                }
                if !pins.contains_key(root) {
                    return Err(failure());
                }
                body["app_id"] = json!(root);
                body["source_revisions"] = Value::Object(pins);
                body["refresh_app_ids"] = operation["snapshot"]["refresh_app_ids"].clone();
            }
            if action == "activate-apps" {
                let imports = operation["snapshot"]["imports"]
                    .as_array()
                    .ok_or_else(failure)?;
                body["app_ids"] = json!(
                    imports
                        .iter()
                        .map(|v| text(v, "app_id"))
                        .collect::<Result<Vec<_>>>()?
                );
            }
            let encrypted = crate::encrypt(&self.encryption_key, &body.to_string())?;
            sqlx::query("INSERT OR IGNORE INTO iam_testing_phases(parent_operation,phase,operation_id,environment_id,encrypted_body,created_at) VALUES(?,?,?,?,?,?)")
                .bind(parent).bind(phase).bind(&id).bind(environment).bind(encrypted).bind(now()).execute(&self.db).await?;
            let saved: String = sqlx::query_scalar("SELECT encrypted_body FROM iam_testing_phases WHERE parent_operation=? AND phase=?").bind(parent).bind(phase).fetch_one(&self.db).await?;
            serde_json::from_str(&crate::decrypt(&self.encryption_key, &saved)?)
                .map_err(|_| failure())?
        };
        let id = text(&body, "operation_id")?;
        let mutation = Mutation::with_key(IdempotencyKey::parse(id).map_err(|_| failure())?);
        let input = decode::<models::HoneycombTestingInstruction>(body.clone())?;
        let receipt = match authority {
            Some(authority) => {
                self.client
                    .testing_instruction_as(authority, &input, &mutation)
                    .await
            }
            None => {
                self.client
                    .testing_instruction(None, &input, &mutation)
                    .await
            }
        }
        .map_err(map_error)?;
        let receipt = serde_json::to_value(receipt).map_err(|_| failure())?;
        validate_phase(operation, &body, &receipt)?;
        let mut public = receipt.clone();
        public.as_object_mut().ok_or_else(failure)?.remove("key");
        public
            .as_object_mut()
            .ok_or_else(failure)?
            .remove("app_secret");
        let mut tx = self.db.begin().await?;
        sqlx::query("UPDATE iam_testing_phases SET receipt=? WHERE parent_operation=? AND phase=? AND operation_id=?")
            .bind(public.to_string()).bind(parent).bind(phase).bind(id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO iam_testing_state(environment_id,iam_revision,generation,key_version,state) VALUES(?,?,?,?,?) ON CONFLICT(environment_id) DO UPDATE SET iam_revision=excluded.iam_revision,generation=excluded.generation,key_version=excluded.key_version,state=excluded.state WHERE iam_testing_state.iam_revision<=excluded.iam_revision")
            .bind(environment).bind(positive(&receipt,"iam_revision")?).bind(positive(&receipt["environment"],"generation")?).bind(positive(&receipt["environment"],"key_version")?).bind(text(&receipt["environment"],"state")?).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub(super) async fn testing_retire(&self, operation: &Value) -> Result<Value> {
        let parent = text(operation, "operation_id")?;
        let environment = text(operation, "environment_id")?;
        let existing = sqlx::query("SELECT encrypted_body,receipt FROM iam_testing_phases WHERE parent_operation=? AND phase='retire'").bind(parent).fetch_optional(&self.db).await?;
        if let Some(receipt) = existing
            .as_ref()
            .and_then(|r| r.get::<Option<String>, _>("receipt"))
        {
            let receipt: Value = serde_json::from_str(&receipt).map_err(|_| failure())?;
            validate_retirement(operation, &receipt)?;
            return Ok(normalized(operation));
        }
        let body: Value = if let Some(existing) = existing {
            serde_json::from_str(&crate::decrypt(
                &self.encryption_key,
                &existing.get::<String, _>("encrypted_body"),
            )?)
            .map_err(|_| failure())?
        } else {
            let state = self
                .client
                .testing_environment(Uuid::parse_str(environment).map_err(|_| failure())?)
                .await
                .map_err(map_error)?;
            if state["generation"] != operation["generation"]
                || state["key_version"] != operation["key_version"]
            {
                return Err(failure());
            }
            let id = Uuid::new_v4().to_string();
            let body = json!({"operation_id":id,"environment_id":environment,"environment_revision":operation["environment_revision"],"expected_iam_revision":state["iam_revision"],"generation":operation["generation"],"key_version":operation["key_version"],"retired_apps":operation["retired_apps"]});
            sqlx::query("INSERT OR IGNORE INTO iam_testing_phases(parent_operation,phase,operation_id,environment_id,encrypted_body,created_at) VALUES(?,'retire',?,?,?,?)").bind(parent).bind(&id).bind(environment).bind(crate::encrypt(&self.encryption_key,&body.to_string())?).bind(now()).execute(&self.db).await?;
            let saved:String=sqlx::query_scalar("SELECT encrypted_body FROM iam_testing_phases WHERE parent_operation=? AND phase='retire'").bind(parent).fetch_one(&self.db).await?;
            serde_json::from_str(&crate::decrypt(&self.encryption_key, &saved)?)
                .map_err(|_| failure())?
        };
        let mutation = Mutation::with_key(
            IdempotencyKey::parse(text(&body, "operation_id")?).map_err(|_| failure())?,
        );
        let response = self
            .client
            .retain_testing_applications(
                &decode::<models::HoneycombRetention>(body.clone())?,
                &mutation,
            )
            .await
            .map_err(map_error)?;
        let response = serde_json::to_value(response).map_err(|_| failure())?;
        if response["operation_id"] != body["operation_id"]
            || response["iam_revision"]
                .as_i64()
                .is_none_or(|r| r <= body["expected_iam_revision"].as_i64().unwrap_or(0))
        {
            return Err(failure());
        }
        validate_retirement(operation, &response)?;
        sqlx::query(
            "UPDATE iam_testing_phases SET receipt=? WHERE parent_operation=? AND phase='retire'",
        )
        .bind(response.to_string())
        .bind(parent)
        .execute(&self.db)
        .await?;
        Ok(normalized(operation))
    }
}

fn normalized(operation: &Value) -> Value {
    let mut result = json!({"state":"completed"});
    for field in [
        "operation_id",
        "environment_id",
        "app_id",
        "environment_revision",
        "generation",
        "key_version",
        "retired_apps",
    ] {
        if let Some(v) = operation.get(field) {
            result[field] = v.clone();
        }
    }
    result
}
fn validate_phase(operation: &Value, body: &Value, receipt: &Value) -> Result<()> {
    if receipt["operation_id"] != body["operation_id"]
        || receipt["environment_id"] != operation["environment_id"]
        || receipt["state"] != "accepted"
        || receipt["iam_completion"] != true
        || receipt["environment"]["environment_id"] != operation["environment_id"]
        || receipt["environment"]["generation"] != operation["generation"]
        || receipt["environment"]["key_version"] != operation["key_version"]
        || receipt["environment"]["iam_revision"] != receipt["iam_revision"]
        || receipt["iam_revision"]
            .as_i64()
            .is_none_or(|r| r <= body["expected_iam_revision"].as_i64().unwrap_or(-1))
    {
        return Err(failure());
    }
    if let Some(key) = receipt.get("key")
        && key != &operation["testing_key"]
    {
        return Err(failure());
    }
    let valid_state = match body["operation"].as_str().unwrap_or("") {
        "prepare" | "restore" | "rotate-key" => receipt["environment"]["state"] == "prepared",
        "clean" => receipt["environment"]["state"] == "cleaned",
        "disable" => receipt["environment"]["state"] == "disabled",
        "purge" => receipt["environment"]["state"] == "purged",
        "activate" | "activate-apps" => receipt["environment"]["state"] == "active",
        "import" => matches!(
            receipt["environment"]["state"].as_str(),
            Some("active" | "prepared")
        ),
        _ => false,
    };
    if !valid_state {
        return Err(failure());
    }
    Ok(())
}
fn validate_retirement(operation: &Value, receipt: &Value) -> Result<()> {
    if receipt["state"] != "accepted" || receipt["iam_completion"] != true {
        return Err(failure());
    }
    for f in ["environment_id", "environment_revision", "retired_apps"] {
        if receipt[f] != operation[f] {
            return Err(failure());
        }
    }
    // SDK receipts expose the same fences in the authoritative environment snapshot.
    for f in ["environment_id", "generation", "key_version"] {
        if receipt["environment"][f] != operation[f] {
            return Err(failure());
        }
    }
    Ok(())
}
fn map_imports(operation: &Value, receipt: &Value) -> Result<Value> {
    let changes = operation["snapshot"]["imports"]
        .as_array()
        .ok_or_else(failure)?;
    let received = receipt["imports"].as_array().ok_or_else(failure)?;
    let mut mapped = Vec::new();
    for source in changes {
        let candidates = received
            .iter()
            .filter(|r| r["app_id"] == source["app_id"])
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err(failure());
        }
        let accepted = candidates[0];
        if accepted["source_revision"] != source["source_iam_revision"]
            || positive(accepted, "iam_revision").is_err()
            || accepted["configuration_revision"]
                .as_i64()
                .is_none_or(|v| v < 0)
        {
            return Err(failure());
        }
        let config = &accepted["effective_configuration"];
        if config["org_id"] != source["org_id"]
            || config["app_id"] != source["app_id"]
            || config["visibility"] != source["source_visibility"]
        {
            return Err(failure());
        }
        for (wire, local) in [
            ("app_name", "name"),
            ("app_logo", "logo_url"),
            ("base_url", "base_url"),
            ("app_scope", "app_scope"),
            ("webhook_scope", "webhook_scope"),
        ] {
            if wire == "base_url"
                && ((config[wire].is_null() && source["configuration"][local] == "")
                    || (config[wire] == "" && source["configuration"][local].is_null()))
            {
                continue;
            }
            if config[wire] != source["configuration"][local] {
                return Err(failure());
            }
        }
        // The source IAM revision binds all remaining security fields. Catalog fields
        // and the isolated Honeycomb projection revision remain Honeycomb-owned.
        mapped.push(json!({"app_id":source["app_id"],"source_revision":source["source_revision"],"configuration_revision":source["configuration_revision"],"iam_configuration_revision":accepted["configuration_revision"],"iam_revision":accepted["iam_revision"],"effective_configuration":source["configuration"],"visibility":"private"}));
    }
    Ok(json!(mapped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, Request, Respond, ResponseTemplate,
        matchers::{method, path},
    };

    struct Phase;
    impl Respond for Phase {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let action = body["operation"].as_str().unwrap();
            let generation = body["generation"].as_i64().unwrap() + i64::from(action == "clean");
            let key_version = body["key_version"]
                .as_i64()
                .or_else(|| body["expected_key_version"].as_i64())
                .unwrap_or(1);
            let state = match action {
                "prepare" | "restore" | "rotate-key" => "prepared",
                "clean" => "cleaned",
                "disable" => "disabled",
                "purge" => "purged",
                _ => "active",
            };
            let revision = body["expected_iam_revision"].as_i64().unwrap() + 2;
            let mut response = json!({"operation_id":body["operation_id"],"state":"accepted","environment_id":body["environment_id"],"iam_revision":revision,"iam_completion":true,"environment":{"environment_id":body["environment_id"],"state":state,"generation":generation,"key_version":key_version,"iam_revision":revision}});
            if let Some(key) = body.get("testing_key") {
                response["key"] = key.clone();
            }
            ResponseTemplate::new(200).set_body_json(response)
        }
    }
    async fn fixture(
        action: &str,
        generation: i64,
        key: i64,
    ) -> (IamManagement, MockServer, Value) {
        let server = MockServer::start().await;
        let db = crate::database("sqlite::memory:").await.unwrap();
        let env = Uuid::new_v4().to_string();
        let op = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,created_at,last_activity) VALUES(?,'alpha','actor','Testing','','encrypted-fixture',?,1,1)").bind(&env).bind(&env).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,'production','actor',?,'environment.prepare',?,'fixture',1,1)").bind(&op).bind(&op).bind(&env).execute(&db).await.unwrap();
        let manager = IamManagement::new(
            &server.uri(),
            format!("hck_{}", "a".repeat(43)),
            db,
            [7; 32],
        )
        .unwrap();
        let operation = json!({"operation_id":op,"environment_id":env,"app_id":"platform>identity","org_id":"alpha","name":"Testing","description":"","environment_revision":8,"generation":generation,"key_version":key,"testing_key":"K".repeat(32),"action":action,"snapshot":{}});
        (manager, server, operation)
    }
    fn endpoint(op: &Value) -> String {
        format!(
            "/api/v1/honeycomb/testing-environments/{}",
            op["environment_id"].as_str().unwrap()
        )
    }
    async fn state(
        server: &MockServer,
        op: &Value,
        revision: i64,
        generation: i64,
        key: i64,
        phase: &str,
    ) {
        Mock::given(method("GET")).and(path(endpoint(op))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"environment_id":op["environment_id"],"iam_revision":revision,"generation":generation,"key_version":key,"state":phase}))).mount(server).await;
    }

    #[tokio::test]
    async fn prepare_requires_separate_final_activation_and_encrypted_exact_replay() {
        let (manager, server, op) = fixture("prepare", 1, 1).await;
        Mock::given(method("GET"))
            .and(path(endpoint(&op)))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({"error":{"code":"not_found","message":"missing"}})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(503)
                    .set_body_json(json!({"error":{"code":"unavailable","message":"lost"}})),
            )
            .mount(&server)
            .await;
        let actor = SecretString::from("oat_fixture".to_owned());
        assert!(
            manager
                .testing_apply(&op, Some(&ManagementAuthority::Actor(&actor)))
                .await
                .is_err()
        );
        let first = server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.method.as_str() == "POST")
            .unwrap()
            .body;
        let stored: String = sqlx::query_scalar("SELECT encrypted_body FROM iam_testing_phases")
            .fetch_one(&manager.db)
            .await
            .unwrap();
        assert!(!stored.contains(&"K".repeat(32)));
        server.reset().await;
        Mock::given(method("POST"))
            .respond_with(Phase)
            .expect(1)
            .mount(&server)
            .await;
        let prepared = manager
            .testing_apply(&op, Some(&ManagementAuthority::Actor(&actor)))
            .await
            .unwrap();
        assert_eq!(prepared["requires_finalization"], true);
        assert_eq!(server.received_requests().await.unwrap()[0].body, first);
        let public: String = sqlx::query_scalar("SELECT receipt FROM iam_testing_phases")
            .fetch_one(&manager.db)
            .await
            .unwrap();
        assert!(!public.contains(&"K".repeat(32)));
        assert!(!public.contains("oat_fixture"));
        let _ = manager
            .testing_apply(&op, Some(&ManagementAuthority::Actor(&actor)))
            .await
            .unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        server.reset().await;
        state(&server, &op, 2, 1, 1, "prepared").await;
        Mock::given(method("POST"))
            .respond_with(Phase)
            .expect(1)
            .mount(&server)
            .await;
        let completed = manager.testing_finalize(&op).await.unwrap();
        assert_eq!(completed["iam_revision"], 4);
        let requests = server.received_requests().await.unwrap();
        let activation = requests
            .iter()
            .find(|r| r.method.as_str() == "POST")
            .unwrap();
        assert!(!activation.headers.contains_key("x-honeycomb-actor-token"));
        let body: Value = serde_json::from_slice(&activation.body).unwrap();
        assert_eq!(body["operation"], "activate");
        assert_ne!(
            body["operation_id"],
            serde_json::from_slice::<Value>(&first).unwrap()["operation_id"]
        );
        assert!(body.get("testing_key").is_none());
    }

    #[tokio::test]
    async fn clean_and_rotation_use_old_fences_then_verify_new_fences() {
        for (action, generation, key) in [("clean", 2, 1), ("rotate-key", 1, 2)] {
            let (manager, server, mut op) = fixture(action, generation, key).await;
            op["authority_testing_key"] = json!("O".repeat(32));
            state(&server, &op, 7, 1, 1, "active").await;
            Mock::given(method("POST"))
                .respond_with(Phase)
                .mount(&server)
                .await;
            manager.testing_apply_user(&op, "").await.unwrap();
            let requests = server.received_requests().await.unwrap();
            let request = requests
                .iter()
                .find(|r| r.method.as_str() == "POST")
                .unwrap();
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["generation"], 1);
            assert_eq!(body["expected_key_version"], 1);
            if action == "rotate-key" {
                assert_eq!(body["key_version"], 2);
                assert_eq!(body["testing_key"], "K".repeat(32));
                assert_eq!(request.headers["x-honeycomb-testing-key"], "O".repeat(32));
            }
            let mut stale = op.clone();
            stale["generation"] = json!(999);
            assert!(manager.testing_apply_user(&stale, "").await.is_err());
        }
    }

    struct Finalizer {
        participant_ready: std::sync::atomic::AtomicBool,
        activation_ready: std::sync::atomic::AtomicBool,
        activations: std::sync::atomic::AtomicUsize,
        participants: std::sync::atomic::AtomicUsize,
    }
    #[async_trait::async_trait]
    impl crate::integration::Management for Finalizer {
        async fn configure(&self, _: &Value, _: &str, _: Option<&str>) -> Result<Value> {
            unreachable!()
        }
        async fn lifecycle(&self, op: &Value, _: &str) -> Result<Value> {
            let mut receipt = normalized(op);
            receipt["requires_finalization"] = json!(op["action"] == "prepare");
            Ok(receipt)
        }
        async fn service_lifecycle(&self, _: &str, op: &Value, _: &str) -> Result<Value> {
            use std::sync::atomic::Ordering::SeqCst;
            self.participants.fetch_add(1, SeqCst);
            if !self.participant_ready.load(SeqCst) {
                return Err(failure());
            }
            Ok(normalized(op))
        }
        async fn finalize_lifecycle(&self, op: &Value) -> Result<Value> {
            use std::sync::atomic::Ordering::SeqCst;
            self.activations.fetch_add(1, SeqCst);
            let mut receipt = normalized(op);
            if !self.activation_ready.load(SeqCst) {
                receipt["generation"] = json!(999);
            }
            Ok(receipt)
        }
    }
    #[tokio::test]
    async fn coordinator_waits_for_every_receipt_and_retries_only_final_activation() {
        use std::sync::{Arc, atomic::Ordering::SeqCst};
        let (manager, _, op) = fixture("prepare", 1, 1).await;
        let env = op["environment_id"].as_str().unwrap();
        let operation = op["operation_id"].as_str().unwrap();
        let encrypted = crate::encrypt(&[7; 32], &"K".repeat(32)).unwrap();
        sqlx::query("UPDATE environments SET encrypted_key=? WHERE id=?")
            .bind(encrypted)
            .bind(env)
            .execute(&manager.db)
            .await
            .unwrap();
        for app in ["platform>identity", "vendor>storage"] {
            sqlx::query("INSERT INTO environment_services(environment_id,app_id,state,operation_id,source_revision,snapshot,generation) VALUES(?,?,'pending',?,0,'{}',1)")
                .bind(env).bind(app).bind(operation).execute(&manager.db).await.unwrap();
        }
        let mock = Arc::new(Finalizer {
            participant_ready: false.into(),
            activation_ready: false.into(),
            activations: 0.into(),
            participants: 0.into(),
        });
        let identity =
            crate::auth::Iam::new("https://identity.example", "platform>catalog", "fixture")
                .unwrap();
        let storage = crate::storage::Briefcase {
            iam: identity.client.clone(),
            http: reqwest::Client::new(),
            base_url: "https://storage.example".into(),
            app_id: "platform>catalog".into(),
            audience: "vendor>storage".into(),
        };
        let state = crate::State {
            telemetry: Default::default(),
            db: manager.db,
            identity: Arc::new(identity),
            management: mock.clone(),
            storage: Arc::new(storage),
            app_id: "platform>catalog".into(),
            iam_app_id: "platform>identity".into(),
            iam_login_url: "https://identity.example".into(),
            encryption_key: [7; 32],
            webhook_secret: "fixture".into(),
        };
        let first = crate::lifecycle::coordinate(&state, env, operation, "actor")
            .await
            .unwrap();
        assert_eq!(first["operation_state"], "pending");
        assert_eq!(mock.activations.load(SeqCst), 0);
        mock.participant_ready.store(true, SeqCst);
        let second = crate::lifecycle::coordinate(&state, env, operation, "actor")
            .await
            .unwrap();
        assert_eq!(second["operation_state"], "pending");
        assert_eq!(mock.activations.load(SeqCst), 1);
        mock.activation_ready.store(true, SeqCst);
        let third = crate::lifecycle::coordinate(&state, env, operation, "actor")
            .await
            .unwrap();
        assert_eq!(third["operation_state"], "accepted");
        assert_eq!(third["state"], "ready");
        assert_eq!(mock.activations.load(SeqCst), 2);
        assert_eq!(mock.participants.load(SeqCst), 2);

        // A subsequent purge removes historical encrypted root material too.
        sqlx::query("INSERT INTO iam_testing_phases(parent_operation,phase,operation_id,environment_id,encrypted_body,created_at) VALUES(?,'prepare',?,?,?,1)")
            .bind(operation).bind(Uuid::new_v4().to_string()).bind(env)
            .bind(state.encrypt("historic root").unwrap()).execute(&state.db).await.unwrap();
        sqlx::query("UPDATE operations SET request_json=? WHERE id=?")
            .bind(json!({"previous_encrypted_key":state.encrypt("older root").unwrap(),"reason":"keep"}).to_string())
            .bind(operation).execute(&state.db).await.unwrap();
        let purge = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,'production','actor',?,'environment.purge',?,'purge',2,1)")
            .bind(&purge).bind(&purge).bind(env).execute(&state.db).await.unwrap();
        sqlx::query("UPDATE environments SET revision=2 WHERE id=?")
            .bind(env)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE environment_services SET operation_id=?,state='pending' WHERE environment_id=?",
        )
        .bind(&purge)
        .bind(env)
        .execute(&state.db)
        .await
        .unwrap();
        let purged = crate::lifecycle::coordinate(&state, env, &purge, "actor")
            .await
            .unwrap();
        assert_eq!(purged["state"], "purged");
        let phases: i64 =
            sqlx::query_scalar("SELECT count(*) FROM iam_testing_phases WHERE environment_id=?")
                .bind(env)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(phases, 0);
        let details: String = sqlx::query_scalar("SELECT request_json FROM operations WHERE id=?")
            .bind(operation)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&details).unwrap(),
            json!({"reason":"keep"})
        );
    }

    #[test]
    fn import_receipts_pin_iam_revision_but_keep_catalog_revision_separate() {
        let source = json!({"app_id":"alpha>app","org_id":"alpha","source_revision":3,"source_iam_revision":17,"source_visibility":"public","configuration_revision":2,"configuration":{"name":"App","logo_url":null,"base_url":"https://app.example","app_scope":{"iam":[],"external":[]},"webhook_scope":[],"description":"Catalog only"}});
        let mut config = source["configuration"].clone();
        config["app_name"] = config["name"].clone();
        config["app_logo"] = Value::Null;
        config["org_id"] = json!("alpha");
        config["app_id"] = json!("alpha>app");
        config["visibility"] = json!("public");
        let op = json!({"snapshot":{"imports":[source]}});
        let mut receipt = json!({"imports":[{"app_id":"alpha>app","source_revision":17,"configuration_revision":0,"iam_revision":4,"effective_configuration":config}]});
        let mapped = map_imports(&op, &receipt).unwrap();
        assert_eq!(mapped[0]["source_revision"], 3);
        assert_eq!(mapped[0]["configuration_revision"], 2);
        assert_eq!(
            mapped[0]["effective_configuration"]["description"],
            "Catalog only"
        );
        receipt["imports"][0]["source_revision"] = json!(3);
        assert!(map_imports(&op, &receipt).is_err());
        receipt["imports"][0]["source_revision"] = json!(17);
        let mut optional = op.clone();
        optional["snapshot"]["imports"][0]["configuration"]["base_url"] = Value::Null;
        receipt["imports"][0]["effective_configuration"]["base_url"] = json!("");
        assert!(map_imports(&optional, &receipt).is_ok());
        receipt["imports"][0]["effective_configuration"]["base_url"] =
            json!("https://different.example");
        assert!(map_imports(&optional, &receipt).is_err());
    }
}
