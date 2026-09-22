//! Official application identity, linked environments and isolated app contracts.
use super::{IamManagement, decode, map_error};
use crate::{
    error::{Error, Result},
    integration::TestingApplicationIdentity,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use secrecy::SecretString;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use silicon_iam_client::{
    EnvironmentKey, IdempotencyKey, Mutation, honeycomb::ManagementAuthority, models,
};
use sqlx::Row;
use std::collections::BTreeSet;
use uuid::Uuid;

fn credentials(authorization: &str) -> Result<(String, SecretString)> {
    let encoded = authorization
        .strip_prefix("Basic ")
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .ok_or_else(Error::unauthorized)?;
    let decoded = STANDARD
        .decode(encoded)
        .map_err(|_| Error::unauthorized())?;
    let decoded = String::from_utf8(decoded).map_err(|_| Error::unauthorized())?;
    let (app, secret) = decoded.split_once(':').ok_or_else(Error::unauthorized)?;
    if !honeycomb_core::valid_app_id(app)
        || !secret.starts_with("ask_")
        || secret.len() > 2048
        || !secret.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(Error::unauthorized());
    }
    Ok((app.to_owned(), SecretString::from(secret.to_owned())))
}
fn uuid(value: &Value, name: &str) -> Result<Uuid> {
    value[name]
        .as_str()
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or_else(|| Error::unavailable("IAM omitted a valid resource identity"))
}
fn positive(value: &Value, name: &str) -> Result<i64> {
    value[name]
        .as_i64()
        .filter(|v| *v > 0)
        .ok_or_else(|| Error::unavailable("IAM omitted a positive lifecycle version"))
}
fn mutation(id: Uuid) -> Result<Mutation> {
    Ok(Mutation::with_key(
        IdempotencyKey::parse(id.to_string()).map_err(|_| Error::bad("Invalid operation ID"))?,
    ))
}

impl IamManagement {
    pub(super) async fn testing_verify_application(
        &self,
        authorization: &str,
    ) -> Result<TestingApplicationIdentity> {
        let (app, secret) = credentials(authorization)?;
        let identity = self
            .client
            .application_identity(&app, &secret)
            .await
            .map_err(map_error)?;
        if identity.app_id != app
            || identity
                .app_id
                .split_once('>')
                .is_none_or(|(org, _)| org != identity.org_id)
            || identity.iam_revision <= 0
            || identity.application_id != app
            || identity.organization_id.is_nil()
        {
            return Err(Error::unavailable(
                "IAM returned another application identity",
            ));
        }
        Ok(TestingApplicationIdentity {
            application_id: identity.application_id.to_string(),
            app_id: identity.app_id,
            organization_id: identity.organization_id.to_string(),
            org_id: identity.org_id,
            iam_revision: identity.iam_revision,
        })
    }
    pub(super) async fn testing_environment_ids(&self, authorization: &str) -> Result<Vec<String>> {
        let (app, secret) = credentials(authorization)?;
        let mut cursor = None;
        let mut cursors = BTreeSet::new();
        let mut ids = BTreeSet::new();
        loop {
            let page = self
                .client
                .application_testing_environments(&app, &secret, cursor, Some(100), None)
                .await
                .map_err(map_error)?;
            let last = page
                .items
                .last()
                .map(|v| uuid(v, "environment_id"))
                .transpose()?;
            for item in page.items {
                let id = uuid(&item, "environment_id")?;
                if id.is_nil() || !ids.insert(id.to_string()) {
                    return Err(Error::unavailable(
                        "IAM returned duplicate or invalid environment links",
                    ));
                }
            }
            if !page.page.has_more {
                break;
            }
            let next = page
                .page
                .next_cursor
                .and_then(|v| Uuid::parse_str(&v).ok())
                .ok_or_else(|| Error::unavailable("IAM omitted the next environment cursor"))?;
            if Some(next) != last || !cursors.insert(next) {
                return Err(Error::unavailable(
                    "IAM environment pagination did not advance",
                ));
            }
            cursor = Some(next);
        }
        Ok(ids.into_iter().collect())
    }
    pub(super) async fn testing_apply_application(
        &self,
        operation: &Value,
        authorization: &str,
        attachment_key: Option<&str>,
    ) -> Result<Value> {
        let (app, secret) = credentials(authorization)?;
        let key = attachment_key
            .map(EnvironmentKey::new)
            .transpose()
            .map_err(|_| Error::bad("Invalid attachment key"))?;
        self.testing_apply(
            operation,
            Some(&ManagementAuthority::Application {
                app_id: &app,
                app_secret: &secret,
                environment_key: key.as_ref(),
            }),
        )
        .await
    }
    /// Derive lifecycle versions from current IAM state and match Honeycomb's selected plane.
    async fn testing_versions(
        &self,
        environment_id: Uuid,
    ) -> Result<models::HoneycombTestingAppVersion> {
        let local = sqlx::query("SELECT state,generation,key_version FROM environments WHERE id=?")
            .bind(environment_id.to_string())
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(Error::missing)?;
        if local.get::<String, _>("state") != "ready" {
            return Err(Error::conflict("Testing environment is not ready"));
        }
        let record = self
            .client
            .testing_environment(environment_id)
            .await
            .map_err(map_error)?;
        if uuid(&record, "environment_id")? != environment_id
            || !matches!(
                record["state"].as_str(),
                Some("active" | "prepared" | "cleaned")
            )
            || positive(&record, "generation")? != local.get::<i64, _>("generation")
            || positive(&record, "key_version")? != local.get::<i64, _>("key_version")
        {
            return Err(Error::conflict(
                "IAM testing context no longer matches Honeycomb",
            ));
        }
        Ok(models::HoneycombTestingAppVersion {
            generation: positive(&record, "generation")?,
            key_version: positive(&record, "key_version")?,
            expected_environment_revision: positive(&record, "iam_revision")?,
        })
    }
    async fn testing_plane(&self, key: &str) -> Result<Uuid> {
        EnvironmentKey::new(key).map_err(|_| Error::bad("Invalid testing environment key"))?;
        let id: Option<String> =
            sqlx::query_scalar("SELECT id FROM environments WHERE key_hash=? AND state='ready'")
                .bind(hex::encode(Sha256::digest(key)))
                .fetch_optional(&self.db)
                .await?;
        id.and_then(|v| Uuid::parse_str(&v).ok()).ok_or_else(|| {
            Error::bad("Testing context is unavailable; no production fallback is permitted")
        })
    }
    pub(super) async fn testing_recover_credential(
        &self,
        environment_id: &str,
        authorization: &str,
    ) -> Result<Value> {
        let identity = self.testing_verify_application(authorization).await?;
        let (app, secret) = credentials(authorization)?;
        let id =
            Uuid::parse_str(environment_id).map_err(|_| Error::bad("Invalid environment ID"))?;
        let versions = self.testing_versions(id).await?;
        // Obtain only the currently stored key of this exact local environment.
        // IAM still validates the immutable production source identity of the import.
        let encrypted: String =
            sqlx::query_scalar("SELECT encrypted_key FROM environments WHERE id=?")
                .bind(environment_id)
                .fetch_one(&self.db)
                .await?;
        let key = EnvironmentKey::new(crate::decrypt(&self.encryption_key, &encrypted)?)
            .map_err(|_| Error::unavailable("Invalid stored environment key"))?;
        let operation_id = Uuid::new_v4();
        let input = models::HoneycombTestingCredentialRecovery {
            operation_id,
            environment_id: id,
            generation: versions.generation,
            key_version: versions.key_version,
            expected_environment_revision: versions.expected_environment_revision,
        };
        let response = serde_json::to_value(
            self.client
                .recover_testing_application_credential(
                    &app,
                    &secret,
                    Some(&key),
                    &input,
                    &mutation(operation_id)?,
                )
                .await
                .map_err(map_error)?,
        )
        .map_err(|e| anyhow::anyhow!(e))?;
        if response["state"] != "accepted"
            || response["operation_id"] != operation_id.to_string()
            || response["environment_id"] != environment_id
            || response["app_id"] != identity.app_id
            || response["app_secret"].as_str().is_none_or(|v| v.is_empty())
        {
            return Err(Error::unavailable(
                "IAM did not return the requested application's testing credential",
            ));
        }
        // Check auth again after secret recovery so a replaced/revoked source cannot
        // inherit the handle while this request is in flight. IAM independently checks
        // source_application_id inside the selected test database.
        let current = self.testing_verify_application(authorization).await?;
        if current.application_id != identity.application_id {
            return Err(Error::unauthorized());
        }
        Ok(
            json!({"environment_id":environment_id,"app_id":identity.app_id,"app_secret":response["app_secret"],"credential_version":response["credential_version"]}),
        )
    }
    async fn testing_configuration(&self, app: &str, plane: Uuid, record: &Value) -> Result<Value> {
        let (org, local) = app
            .split_once('>')
            .ok_or_else(|| Error::bad("Invalid app ID"))?;
        if record["app_id"] != app || record["org_id"] != org {
            return Err(Error::unavailable("IAM returned another test application"));
        }
        let row = sqlx::query(
            "SELECT revision,config,effective_config FROM applications WHERE plane=? AND app_id=?",
        )
        .bind(plane.to_string())
        .bind(app)
        .fetch_one(&self.db)
        .await?;
        let accepted = self.testing_projection_revision(app, plane, record).await?;
        let base: String = if accepted == row.get::<i64, _>("revision") {
            row.get("config")
        } else {
            row.get::<Option<String>, _>("effective_config")
                .ok_or_else(|| Error::unavailable("No matching accepted test metadata exists"))?
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
            if record.get(from).is_some() {
                config[to] = record[from].clone();
            }
        }
        if let Some(url) = record.get("webhook_url") {
            config["webhook_url"] = url.clone();
        } else {
            config
                .as_object_mut()
                .ok_or_else(|| Error::unavailable("Invalid test configuration"))?
                .remove("webhook_url");
        }
        let granted: BTreeSet<&str> = record["effective_scopes"]
            .as_array()
            .ok_or_else(|| Error::unavailable("IAM omitted effective test scopes"))?
            .iter()
            .filter_map(|v| v["scope"].as_str())
            .collect();
        let mut scopes = record["app_scope"].clone();
        if let Some(scopes) = scopes["iam"].as_array_mut() {
            scopes.retain(|v| v.as_str().is_some_and(|v| granted.contains(v)));
        }
        if let Some(scopes) = scopes["external"].as_array_mut() {
            scopes.retain(|v| {
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
        Ok(config)
    }
    /// An import has its own accepted Honeycomb revision. IAM retains a separate
    /// configuration counter, including zero for an imported, unconfigured app.
    /// Only an accepted import may translate that counter into our projection.
    async fn testing_projection_revision(
        &self,
        app: &str,
        plane: Uuid,
        record: &Value,
    ) -> Result<i64> {
        let revision = record["configuration_revision"]
            .as_i64()
            .filter(|v| *v >= 0)
            .ok_or_else(|| {
                Error::unavailable("IAM omitted its accepted test configuration revision")
            })?;
        let imported: Option<(String, i64)> = sqlx::query_as(
            "SELECT i.snapshot,a.effective_revision FROM environment_imports i JOIN applications a ON a.plane=i.environment_id AND a.app_id=i.app_id WHERE i.environment_id=? AND i.app_id=? AND a.iam_revision>0 AND a.effective_config IS NOT NULL",
        )
        .bind(plane.to_string())
        .bind(app)
        .fetch_optional(&self.db)
        .await?;
        if let Some((snapshot, effective)) = imported {
            let snapshot: Value =
                serde_json::from_str(&snapshot).map_err(|e| anyhow::anyhow!(e))?;
            // Historical import receipts did not retain IAM's configuration
            // counter. Only their known zero baseline is safe to translate.
            let imported_iam = snapshot["iam_configuration_revision"].as_i64().unwrap_or(0);
            if snapshot["app_id"] == app
                && effective > 0
                && snapshot["configuration_revision"] == effective
                && revision == imported_iam
            {
                return Ok(effective);
            }
        }
        if revision == 0 {
            return Err(Error::conflict(
                "No accepted import matches IAM configuration revision zero",
            ));
        }
        Ok(revision)
    }
    pub(super) async fn testing_snapshot(&self, app: &str, environment_key: &str) -> Result<Value> {
        let plane = self.testing_plane(environment_key).await?;
        self.testing_snapshot_plane(app, &plane.to_string()).await
    }
    pub(super) async fn testing_snapshot_plane(
        &self,
        app: &str,
        environment_id: &str,
    ) -> Result<Value> {
        let plane =
            Uuid::parse_str(environment_id).map_err(|_| Error::bad("Invalid environment ID"))?;
        let version = self.testing_versions(plane).await?;
        let record = self
            .client
            .testing_application(plane, app, &version)
            .await
            .map_err(map_error)?;
        let configuration = self.testing_configuration(app, plane, &record).await?;
        let accepted = self
            .testing_projection_revision(app, plane, &record)
            .await?;
        let publication:Option<String>=sqlx::query_scalar("SELECT id FROM publication_requests WHERE plane=? AND app_id=? AND revision=? AND id=? AND state IN ('activating','published')").bind(plane.to_string()).bind(app).bind(record["configuration_revision"].as_i64().unwrap_or(-1)).bind(record["publication_request_id"].as_str().unwrap_or("")).fetch_optional(&self.db).await?;
        Ok(
            json!({"app_id":app,"environment_id":plane,"configuration_revision":accepted,"iam_revision":record["iam_revision"],"visibility":record["visibility"],"availability":if record["availability"]=="verified" && record["ready"]!=false {"active"} else {"disabled"},"effective_configuration":configuration,"publication_request_id":publication,"credential_version":record["credential_version"]}),
        )
    }
    pub(super) async fn testing_configure(
        &self,
        o: &Value,
        actor: &str,
        environment_key: &str,
    ) -> Result<Value> {
        self.testing_mutate(o, actor, None, environment_key, false)
            .await
    }
    pub(super) async fn testing_rotate_secret(
        &self,
        o: &Value,
        actor: &str,
        step_up: Option<&str>,
        environment_key: &str,
    ) -> Result<Value> {
        self.testing_mutate(o, actor, step_up, environment_key, true)
            .await
    }
    async fn testing_mutate(
        &self,
        o: &Value,
        _actor: &str,
        step_up: Option<&str>,
        environment_key: &str,
        rotation: bool,
    ) -> Result<Value> {
        let plane = self.testing_plane(environment_key).await?;
        let versions = self.testing_versions(plane).await?;
        let id = uuid(o, "operation_id")?;
        let app = o["app_id"]
            .as_str()
            .filter(|v| honeycomb_core::valid_app_id(v))
            .ok_or_else(|| Error::bad("Invalid app ID"))?;
        let c = &o["configuration"];
        let mut configuration_revision = positive(o, "configuration_revision")?;
        let saved: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM management_requests WHERE operation_id=?)",
        )
        .bind(id.to_string())
        .fetch_one(&self.db)
        .await?;
        if rotation && !saved {
            let record = self
                .client
                .testing_application(plane, app, &versions)
                .await
                .map_err(map_error)?;
            if record["app_id"] != app
                || record["iam_revision"] != o["expected_iam_revision"]
                || self
                    .testing_projection_revision(app, plane, &record)
                    .await?
                    != configuration_revision
            {
                return Err(Error::conflict(
                    "IAM test application changed before secret rotation; reconcile its accepted state",
                ));
            }
            configuration_revision = record["configuration_revision"].as_i64().unwrap();
        }
        let configuration = if rotation {
            None
        } else {
            Some(decode::<models::HoneycombConfiguration>(
                json!({"operation_id":id,"configuration_revision":o["configuration_revision"],"expected_iam_revision":o["expected_iam_revision"],"app_id":app,"org_id":c["org_id"],"name":c["name"],"logo_url":c["logo_url"],"base_url":c["base_url"],"visibility":o["visibility"],"availability":"active","publication_approved":false,"webhook":{"url":c["webhook_url"],"secret":o["webhook_secret"],"scope":c["webhook_scope"]},"app_scope":c["app_scope"],"obo_endpoints":c["obo_endpoints"],"obo_review_message":c["obo_review_message"],"testing_idle_days":c["testing_idle_days"].as_u64().unwrap_or(30)}),
            )?)
        };
        let input = models::HoneycombTestingAppMutation {
            operation_id: id,
            environment_id: plane,
            generation: versions.generation,
            key_version: versions.key_version,
            expected_environment_revision: versions.expected_environment_revision,
            expected_iam_revision: o["expected_iam_revision"]
                .as_i64()
                .filter(|v| *v >= 0)
                .ok_or_else(|| Error::bad("Missing IAM revision"))?,
            configuration_revision,
            configuration,
        };
        let body = self
            .saved(
                &id.to_string(),
                if rotation {
                    "testing.secret.rotate"
                } else {
                    "testing.configure"
                },
                app,
                serde_json::to_value(&input).map_err(|e| anyhow::anyhow!(e))?,
            )
            .await?;
        let input: models::HoneycombTestingAppMutation = decode(body)?;
        if input.environment_id != plane
            || input.generation != versions.generation
            || input.key_version != versions.key_version
        {
            return Err(Error::conflict(
                "Testing lifecycle changed before management retry",
            ));
        }
        self.testing_execute_mutation(app, &input, environment_key, step_up, rotation)
            .await
    }
    pub(super) async fn testing_operation_result(
        &self,
        id: &str,
        _actor: &str,
        environment_key: &str,
    ) -> Result<Value> {
        let plane = self.testing_plane(environment_key).await?;
        let versions = self.testing_versions(plane).await?;
        let row = sqlx::query(
            "SELECT kind,app_id,encrypted_body FROM management_requests WHERE operation_id=?",
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(Error::missing)?;
        let rotation = match row.get::<String, _>("kind").as_str() {
            "testing.configure" => false,
            "testing.secret.rotate" => true,
            _ => return Err(Error::bad("Not a test application operation")),
        };
        let input: models::HoneycombTestingAppMutation = serde_json::from_str(&crate::decrypt(
            &self.encryption_key,
            &row.get::<String, _>("encrypted_body"),
        )?)
        .map_err(|_| Error::unavailable("Stored testing operation is invalid"))?;
        if input.operation_id.to_string() != id
            || input.environment_id != plane
            || input.generation != versions.generation
            || input.key_version != versions.key_version
        {
            return Err(Error::conflict(
                "Testing lifecycle changed before management replay",
            ));
        }
        self.testing_execute_mutation(
            &row.get::<String, _>("app_id"),
            &input,
            environment_key,
            None,
            rotation,
        )
        .await
    }
    /// Reserve one monotonic participant revision per configuration operation. The
    /// accepted test configuration is sent to the exact participant, never reimported
    /// from production. IAM activates only after the protected participant receipt.
    async fn testing_configure_participant(
        &self,
        app: &str,
        input: &models::HoneycombTestingAppMutation,
        configuration: &Value,
    ) -> Result<()> {
        let parent = input.operation_id.to_string();
        let environment = input.environment_id.to_string();
        let phase = "application-participant";
        let mut tx = self.db.begin().await?;
        let existing=sqlx::query("SELECT encrypted_body,receipt FROM iam_testing_phases WHERE parent_operation=? AND phase=?")
            .bind(&parent).bind(phase).fetch_optional(&mut *tx).await?;
        let (operation, completed) = if let Some(existing) = existing {
            let operation: Value = serde_json::from_str(&crate::decrypt(
                &self.encryption_key,
                &existing.get::<String, _>("encrypted_body"),
            )?)
            .map_err(|_| Error::unavailable("Invalid participant operation"))?;
            (
                operation,
                existing.get::<Option<String>, _>("receipt").is_some(),
            )
        } else {
            let row=sqlx::query("SELECT org_id,state,revision,generation,key_version,encrypted_key FROM environments WHERE id=?")
                .bind(&environment).fetch_one(&mut *tx).await?;
            if row.get::<String, _>("state") != "ready"
                || row.get::<i64, _>("generation") != input.generation
                || row.get::<i64, _>("key_version") != input.key_version
            {
                return Err(Error::conflict(
                    "Environment changed during application configuration",
                ));
            }
            let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE plane='production' AND resource=? AND kind LIKE 'environment.%' AND state='pending')")
                .bind(&environment).fetch_one(&mut *tx).await?;
            if pending {
                return Err(Error::conflict(
                    "Finish the pending environment operation before configuring this application",
                ));
            }
            let revision = row.get::<i64, _>("revision") + 1;
            let operation = json!({"operation_id":parent,"environment_id":environment,"app_id":app,"org_id":row.get::<String,_>("org_id"),"action":"import","environment_revision":revision,"generation":input.generation,"key_version":input.key_version,"testing_key":crate::decrypt(&self.encryption_key,&row.get::<String,_>("encrypted_key"))?,"snapshot":{"app_id":app,"configuration":configuration,"configuration_revision":input.configuration_revision,"imports":[{"app_id":app}]}});
            sqlx::query("INSERT INTO iam_testing_phases(parent_operation,phase,operation_id,environment_id,encrypted_body,created_at) VALUES(?,?,?,?,?,?)")
                .bind(&parent).bind(phase).bind(Uuid::new_v4().to_string()).bind(&environment).bind(crate::encrypt(&self.encryption_key,&operation.to_string())?).bind(crate::now()).execute(&mut *tx).await?;
            sqlx::query(
                "UPDATE environments SET revision=?,last_activity=? WHERE id=? AND revision=?",
            )
            .bind(revision)
            .bind(crate::now())
            .bind(&environment)
            .bind(revision - 1)
            .execute(&mut *tx)
            .await?;
            (operation, false)
        };
        tx.commit().await?;
        let current = sqlx::query(
            "SELECT state,revision,generation,key_version FROM environments WHERE id=?",
        )
        .bind(&environment)
        .fetch_one(&self.db)
        .await?;
        if current.get::<String, _>("state") != "ready"
            || current.get::<i64, _>("revision") != positive(&operation, "environment_revision")?
            || current.get::<i64, _>("generation") != input.generation
            || current.get::<i64, _>("key_version") != input.key_version
        {
            return Err(Error::conflict(
                "A newer environment operation supersedes application configuration",
            ));
        }
        if !completed {
            let receipt = self.participants.apply(app, &operation).await?;
            if receipt["state"] != "completed" {
                return Err(Error::unavailable(
                    "Application participant setup remains pending",
                ));
            }
            // Registry verifies exact identity and lifecycle fences and strips secret fields.
            sqlx::query(
                "UPDATE iam_testing_phases SET receipt=? WHERE parent_operation=? AND phase=?",
            )
            .bind(receipt.to_string())
            .bind(&parent)
            .bind(phase)
            .execute(&self.db)
            .await?;
        }
        self.testing_activate_application(&operation).await?;
        Ok(())
    }
    async fn testing_execute_mutation(
        &self,
        app: &str,
        input: &models::HoneycombTestingAppMutation,
        environment_key: &str,
        step_up: Option<&str>,
        rotation: bool,
    ) -> Result<Value> {
        let id = input.operation_id;
        let plane = input.environment_id;
        let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE plane='production' AND resource=? AND kind LIKE 'environment.%' AND state='pending')")
            .bind(plane.to_string()).fetch_one(&self.db).await?;
        if pending {
            return Err(Error::conflict(
                "Finish the pending environment lifecycle operation first",
            ));
        }
        if self.testing_plane(environment_key).await? != plane {
            return Err(Error::forbidden());
        }
        let key = EnvironmentKey::new(environment_key)
            .map_err(|_| Error::bad("Invalid testing environment key"))?;
        let mut m = mutation(id)?;
        if let Some(proof) = step_up {
            m = m.step_up(proof);
        }
        // Honeycomb already checked the current test administrator. IAM receives
        // bounded root authority for this exact plane, never a test OAT presented
        // as a production management user token.
        let authority = ManagementAuthority::Environment(&key);
        let receipt = if rotation {
            self.client
                .rotate_testing_application_secret(app, &authority, input, &m)
                .await
        } else {
            self.client
                .configure_testing_application(app, &authority, input, &m)
                .await
        }
        .map_err(|error| match error {
            // IAM checks the exact target-plane operation receipt before this
            // precondition. This specific rejection proves this immutable request
            // did not rotate a secret. Other conflicts can hide an accepted target
            // transaction and must remain recoverable/pending.
            silicon_iam_client::Error::Api(api)
                if rotation && api.status == 409 && api.code == "configuration_revision_conflict" =>
            {
                Error::new(axum::http::StatusCode::CONFLICT, "testing_configuration_revision_conflict",
                    "IAM rejected this test rotation before changing the credential. Reconcile the application and start a new rotation with a new idempotency key.")
            }
            error => map_error(error),
        })?;
        let mut response = serde_json::to_value(receipt).map_err(|e| anyhow::anyhow!(e))?;
        if response["operation_id"] != id.to_string()
            || response["environment_id"] != plane.to_string()
            || response["app_id"] != app
            || response["configuration_revision"] != input.configuration_revision
        {
            return Err(Error::unavailable(
                "IAM returned another isolated application operation",
            ));
        }
        if !rotation && response["state"] == "accepted" {
            response["effective_configuration"] = self
                .testing_configuration(app, plane, &response["effective_configuration"])
                .await?;
        }
        // IAM explicitly gates fresh/reconfigured apps. Do not translate ready:false
        // into a completed usable application until its participant activates it.
        if response["state"] == "accepted" {
            let mut public = response.clone();
            if let Some(object) = public.as_object_mut() {
                object.remove("app_secret");
            }
            sqlx::query("INSERT INTO iam_testing_phases(parent_operation,phase,operation_id,environment_id,encrypted_body,receipt,created_at) VALUES(?,'application-configuration',?,?,?,?,?) ON CONFLICT(parent_operation,phase) DO UPDATE SET receipt=excluded.receipt")
                .bind(id.to_string()).bind(Uuid::new_v4().to_string()).bind(plane.to_string()).bind(crate::encrypt(&self.encryption_key,&serde_json::to_string(input).map_err(|e|anyhow::anyhow!(e))?)?).bind(public.to_string()).bind(crate::now()).execute(&self.db).await?;
        }
        if !rotation && response["state"] == "accepted" {
            self.testing_configure_participant(app, input, &response["effective_configuration"])
                .await?;
            response["ready"] = json!(true);
        }
        if rotation {
            // Keep the public, actor-bound Honeycomb revision while validating
            // IAM's distinct revision against the immutable wire request above.
            let revision: i64 = sqlx::query_scalar("SELECT revision FROM operations WHERE id=? AND plane=? AND resource=? AND kind='secret.rotate'")
                .bind(id.to_string()).bind(plane.to_string()).bind(app).fetch_one(&self.db).await?;
            response["configuration_revision"] = json!(revision);
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests;
