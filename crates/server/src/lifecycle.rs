//! Durable shared-environment coordination. Only protected service responses establish readiness.
use crate::{
    State,
    error::{Error, Result},
    now,
};
use serde_json::{Value, json};
use sqlx::Row;

pub async fn coordinate(
    s: &State,
    id: &str,
    operation_id: &str,
    actor_token: &str,
) -> Result<Value> {
    coordinate_authorized(s, id, operation_id, actor_token, None).await
}
pub async fn coordinate_application(
    s: &State,
    id: &str,
    operation_id: &str,
    authority: &crate::integration::TestingApplicationAuthority,
) -> Result<Value> {
    coordinate_authorized(s, id, operation_id, "", Some(authority)).await
}
async fn coordinate_authorized(
    s: &State,
    id: &str,
    operation_id: &str,
    actor_token: &str,
    application: Option<&crate::integration::TestingApplicationAuthority>,
) -> Result<Value> {
    let lease = uuid::Uuid::new_v4().to_string();
    let acquired=sqlx::query("UPDATE operations SET lease_token=?,lease_until=? WHERE id=? AND resource=? AND plane='production' AND state='pending' AND (lease_token IS NULL OR lease_until<?)")
        .bind(&lease).bind(now()+600).bind(operation_id).bind(id).bind(now()).execute(&s.db).await?;
    if acquired.rows_affected() == 0 {
        let state: String = sqlx::query_scalar("SELECT state FROM environments WHERE id=?")
            .bind(id)
            .fetch_one(&s.db)
            .await?;
        let operation_state: String = sqlx::query_scalar(
            "SELECT state FROM operations WHERE id=? AND resource=? AND plane='production'",
        )
        .bind(operation_id)
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)?;
        return Ok(
            json!({"operation_id":operation_id,"environment_id":id,"state":state,"operation_state":operation_state,"message":"Operation is already coordinating or completed; inspect its current service receipts"}),
        );
    }
    let result = coordinate_inner(s, id, operation_id, actor_token, &lease, application).await;
    sqlx::query(
        "UPDATE operations SET lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
    )
    .bind(operation_id)
    .bind(lease)
    .execute(&s.db)
    .await?;
    result
}
async fn coordinate_inner(
    s: &State,
    id: &str,
    operation_id: &str,
    actor_token: &str,
    lease: &str,
    application: Option<&crate::integration::TestingApplicationAuthority>,
) -> Result<Value> {
    let environment = sqlx::query("SELECT * FROM environments WHERE id=?")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    let operation=sqlx::query("SELECT kind,revision,state,request_json FROM operations WHERE id=? AND resource=? AND plane='production'").bind(operation_id).bind(id).fetch_one(&s.db).await?;
    let revision: i64 = environment.get("revision");
    if operation.get::<i64, _>("revision") != revision {
        return Err(Error::conflict(
            "A newer lifecycle operation supersedes this operation",
        ));
    }
    if operation.get::<String, _>("state") == "accepted" {
        return Ok(
            json!({"operation_id":operation_id,"environment_id":id,"state":environment.get::<String,_>("state"),"revision":revision}),
        );
    }
    let kind: String = operation.get("kind");
    let action = kind
        .strip_prefix("environment.")
        .ok_or_else(|| Error::bad("Not an environment lifecycle operation"))?;
    let details: Value = serde_json::from_str(
        operation
            .get::<Option<String>, _>("request_json")
            .as_deref()
            .unwrap_or("{}"),
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let automatic = details["source"] == "retention";
    let retired_apps = if action == "retire" {
        details["retired_apps"].clone()
    } else {
        json!([])
    };
    if action == "retire"
        && retired_apps
            .as_array()
            .is_none_or(|a| a.is_empty() || a.iter().any(|v| v.as_str().is_none()))
    {
        return Err(Error::bad("Retirement requires exact application IDs"));
    }
    let generation: i64 = environment.get("generation");
    let key_version: i64 = environment.get("key_version");
    let services=sqlx::query("SELECT app_id,snapshot,state FROM environment_services WHERE environment_id=? AND operation_id=? ORDER BY CASE WHEN app_id=? THEN 0 ELSE 1 END,app_id").bind(id).bind(operation_id).bind(&s.iam_app_id).fetch_all(&s.db).await?;
    if services.is_empty() {
        return Err(Error::conflict(
            "Lifecycle operation has no participating services",
        ));
    }
    let mut application_credential = None;
    for service in services {
        let held=sqlx::query("UPDATE operations SET lease_until=? WHERE id=? AND lease_token=? AND lease_until>? AND state='pending'")
            .bind(now()+600).bind(operation_id).bind(lease).bind(now()).execute(&s.db).await?;
        if held.rows_affected() != 1 {
            return Err(Error::conflict(
                "Lifecycle coordinator lease expired; retry the current operation",
            ));
        }

        if service.get::<String, _>("state") == "ready" {
            continue;
        }
        let app: String = service.get("app_id");
        let service_action = match action {
            "delete" => "disable",
            "retire" => "retire-applications",
            other => other,
        };
        let mut request = json!({"retired_apps":retired_apps,"reason":if automatic {"inactivity"} else {"requested"},"operation_id":operation_id,"action":service_action,"environment_id":id,"org_id":environment.get::<String,_>("org_id"),"name":environment.get::<String,_>("name"),"description":environment.get::<String,_>("description"),"environment_revision":revision,"generation":generation,"key_version":key_version,"testing_key":s.decrypt(&environment.get::<String,_>("encrypted_key"))?,"app_id":app,"snapshot":serde_json::from_str::<Value>(&service.get::<String,_>("snapshot")).map_err(|e|anyhow::anyhow!(e))?});
        if app == s.iam_app_id
            && let Some(previous) = details["previous_encrypted_key"].as_str()
        {
            request["authority_testing_key"] = json!(s.decrypt(previous)?);
        }
        let result = if app == s.app_id {
            apply_local(s,id,action,operation_id,lease).await.map(|_|json!({"state":"completed","operation_id":operation_id,"environment_id":id,"app_id":app,"environment_revision":revision,"generation":generation,"key_version":key_version,"retired_apps":retired_apps}))
        } else if !automatic && app == s.iam_app_id {
            if let Some(authority) = application {
                s.management
                    .application_lifecycle(
                        &request,
                        &authority.authorization,
                        authority.attachment_key.as_deref(),
                    )
                    .await
            } else {
                s.management.lifecycle(&request, actor_token).await
            }
        } else if automatic {
            s.management.retention_lifecycle(&app, &request).await
        } else {
            s.management
                .service_lifecycle(&app, &request, actor_token)
                .await
        };
        let mut stored_receipt = None;
        let mut finalization_required = false;
        let error=match result {
            Ok(receipt) if receipt["state"]=="completed" && receipt["operation_id"]==operation_id && receipt["environment_id"]==id && receipt["app_id"]==app && receipt["environment_revision"]==revision && receipt["generation"]==generation && receipt["key_version"]==key_version && (action!="retire" || receipt["retired_apps"]==retired_apps) => {
                finalization_required = app == s.iam_app_id && receipt["requires_finalization"] == true;
                if action=="import" && app==s.iam_app_id {
                    match accepted_imports(&request["snapshot"]["imports"], &receipt["imports"]) {
                        Ok(imports)=>{
                            stored_receipt=Some(imports.to_string());
                            if let Some(authority) = application {
                                let credential = &receipt["application_credential"];
                                if credential["app_id"] == authority.identity.app_id && credential["environment_id"] == id && credential["app_secret"].as_str().is_some_and(|s| !s.is_empty()) {
                                    application_credential = Some(json!({"app_id":credential["app_id"],"environment_id":id,"app_secret":credential["app_secret"]}));
                                }
                            }
                            None
                        },
                        Err(message)=>Some(message),
                    }
                } else { None }
            },
            Ok(_)=>Some("Service has not confirmed the exact operation, environment revision, generation and key version".to_string()),
            Err(e)=>Some(if application.is_some() {"Application lifecycle request failed; retry after checking IAM authority and participant readiness".to_owned()} else {e.1.message}),
        };
        let failed = error.is_some();
        let written=sqlx::query("UPDATE environment_services SET state=?,error=?,generation=?,receipt=?,finalization_required=? WHERE environment_id=? AND app_id=? AND operation_id=? AND EXISTS(SELECT 1 FROM operations WHERE id=? AND lease_token=? AND lease_until>?)")
            .bind(if failed{"failed"}else{"ready"}).bind(error).bind(generation).bind(stored_receipt).bind(finalization_required).bind(id).bind(&app).bind(operation_id).bind(operation_id).bind(lease).bind(now()).execute(&s.db).await?;
        if written.rows_affected() != 1 {
            return Err(Error::conflict("Lifecycle coordinator lease expired"));
        }
        // Import application services only after IAM has accepted their isolated records.
        if matches!(action, "import" | "retire") && app == s.iam_app_id && failed {
            break;
        }
    }
    let pending:i64=sqlx::query_scalar("SELECT COUNT(*) FROM environment_services WHERE environment_id=? AND operation_id=? AND state!='ready'").bind(id).bind(operation_id).fetch_one(&s.db).await?;
    if pending > 0 {
        let error = format!(
            "Waiting for {pending} service receipt(s); inspect environment services and retry this operation"
        );
        sqlx::query("UPDATE operations SET error=? WHERE id=? AND state='pending'")
            .bind(&error)
            .bind(operation_id)
            .execute(&s.db)
            .await?;
        return Ok(
            json!({"operation_id":operation_id,"environment_id":id,"state":environment.get::<String,_>("state"),"revision":revision,"generation":generation,"operation_state":"pending","error":error}),
        );
    }
    let finalization = sqlx::query("SELECT snapshot FROM environment_services WHERE environment_id=? AND app_id=? AND operation_id=? AND finalization_required=1")
        .bind(id).bind(&s.iam_app_id).bind(operation_id).fetch_optional(&s.db).await?;
    if let Some(finalization) = finalization {
        let request = json!({"operation_id":operation_id,"action":action,"environment_id":id,"app_id":s.iam_app_id,"environment_revision":revision,"generation":generation,"key_version":key_version,"snapshot":serde_json::from_str::<Value>(&finalization.get::<String,_>("snapshot")).map_err(|e|anyhow::anyhow!(e))?});
        let receipt = s.management.finalize_lifecycle(&request).await;
        let confirmed = receipt.as_ref().is_ok_and(|r| {
            r["state"] == "completed"
                && [
                    "operation_id",
                    "environment_id",
                    "app_id",
                    "environment_revision",
                    "generation",
                    "key_version",
                ]
                .iter()
                .all(|f| r[*f] == request[*f])
        });
        if !confirmed {
            let error = "IAM activation is pending after participant preparation; retry the same lifecycle operation";
            sqlx::query("UPDATE operations SET error=? WHERE id=? AND lease_token=?")
                .bind(error)
                .bind(operation_id)
                .bind(lease)
                .execute(&s.db)
                .await?;
            return Ok(
                json!({"operation_id":operation_id,"environment_id":id,"state":environment.get::<String,_>("state"),"revision":revision,"generation":generation,"operation_state":"pending","error":error}),
            );
        }
        let saved = sqlx::query("UPDATE environment_services SET finalization_required=0 WHERE environment_id=? AND app_id=? AND operation_id=? AND EXISTS(SELECT 1 FROM operations WHERE id=? AND lease_token=? AND lease_until>?)").bind(id).bind(&s.iam_app_id).bind(operation_id).bind(operation_id).bind(lease).bind(now()).execute(&s.db).await?;
        if saved.rows_affected() != 1 {
            return Err(Error::conflict(
                "Lifecycle coordinator lease expired during IAM activation",
            ));
        }
    }
    let final_state = match action {
        "delete" => "deleted",
        "purge" => "purged",
        _ => "ready",
    };
    let mut tx = s.db.begin().await?;
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE id=? AND lease_token=? AND lease_until>? AND state='pending')").bind(operation_id).bind(lease).bind(now()).fetch_one(&mut *tx).await?;
    if !held {
        return Err(Error::conflict("Lifecycle coordinator lease changed"));
    }
    let updated=sqlx::query("UPDATE environments SET state=?,last_activity=CASE WHEN ? THEN last_activity ELSE ? END,deleted_at=?,purge_after=? WHERE id=? AND revision=?")
        .bind(final_state).bind(matches!(action,"retire"|"delete"|"purge")).bind(now()).bind(if action=="delete"{Some(now())}else{None::<i64>}).bind(if action=="delete"{Some(now()+30*86400)}else{None::<i64>}).bind(id).bind(revision).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(Error::conflict(
            "Environment changed during lifecycle coordination",
        ));
    }
    if action == "import" {
        let row=sqlx::query("SELECT snapshot,receipt FROM environment_services WHERE environment_id=? AND app_id=? AND operation_id=?")
            .bind(id).bind(&s.iam_app_id).bind(operation_id).fetch_one(&mut *tx).await?;
        let payload: Value = serde_json::from_str(&row.get::<String, _>("snapshot"))
            .map_err(|e| anyhow::anyhow!(e))?;
        let imports: Value = serde_json::from_str(&row.get::<String, _>("receipt"))
            .map_err(|e| anyhow::anyhow!(e))?;
        for source in payload["imports"]
            .as_array()
            .ok_or_else(|| Error::conflict("Import snapshot missing"))?
        {
            let app = source["app_id"].as_str().unwrap();
            let receipt = imports
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["app_id"] == app)
                .ok_or_else(|| Error::conflict("Import receipt missing"))?;
            let config = source["configuration"].to_string();
            let installed=sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,visibility,state,revision,iam_revision,config,effective_config,effective_revision,webhook_secret,created_at,updated_at) VALUES(?,?,?,?,?,'private','active',?,?,?,?,?,'',?,?) ON CONFLICT(plane,app_id) DO UPDATE SET name=excluded.name,description=excluded.description,visibility='private',state='active',revision=excluded.revision,iam_revision=excluded.iam_revision,config=excluded.config,effective_config=excluded.effective_config,effective_revision=excluded.effective_revision,webhook_secret='',updated_at=excluded.updated_at WHERE applications.revision=excluded.revision-1")
                .bind(id).bind(app).bind(source["org_id"].as_str().unwrap()).bind(source["configuration"]["name"].as_str().unwrap_or(app)).bind(source["configuration"]["description"].as_str().unwrap_or(""))
                .bind(source["configuration_revision"].as_i64().unwrap()).bind(receipt["iam_revision"].as_i64().unwrap()).bind(&config).bind(&config).bind(source["configuration_revision"].as_i64().unwrap()).bind(now()).bind(now()).execute(&mut *tx).await?;
            if installed.rows_affected() != 1 {
                return Err(Error::conflict(
                    "Test application changed while import was coordinating; reconcile before refreshing",
                ));
            }
            sqlx::query("INSERT INTO environment_app_activity(environment_id,app_id,last_activity) VALUES(?,?,?) ON CONFLICT(environment_id,app_id) DO UPDATE SET last_activity=excluded.last_activity").bind(id).bind(app).bind(now()).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO environment_imports(environment_id,app_id,source_revision,snapshot,last_activity) VALUES(?,?,?,?,?) ON CONFLICT(environment_id,app_id) DO UPDATE SET source_revision=excluded.source_revision,snapshot=excluded.snapshot,last_activity=excluded.last_activity")
                .bind(id).bind(app).bind(source["source_revision"].as_i64().unwrap()).bind(source.to_string()).bind(now()).execute(&mut *tx).await?;
        }
    }
    if action == "retire" {
        // Keep the core coordinators and every surviving application linked.
        for app in retired_apps.as_array().unwrap() {
            if app.as_str() != Some(s.iam_app_id.as_str())
                && app.as_str() != Some(s.app_id.as_str())
            {
                sqlx::query("DELETE FROM environment_services WHERE environment_id=? AND app_id=? AND operation_id=?").bind(id).bind(app.as_str().unwrap()).bind(operation_id).execute(&mut *tx).await?;
            }
        }
    }
    if action == "clean" {
        // Cleaning removes imports and their data, but participants still own
        // environment fences and capacity. Keep them linked so later rotation,
        // deletion, restoration and purge reach every service that was prepared.
        sqlx::query("UPDATE environment_services SET snapshot='{}',source_revision=0,receipt=NULL,error=NULL WHERE environment_id=?").bind(id).execute(&mut *tx).await?;
    }
    if action == "retire" {
        let rows =
            sqlx::query("SELECT app_id,snapshot FROM environment_services WHERE environment_id=?")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
        for row in rows {
            let mut snapshot: Value = serde_json::from_str(&row.get::<String, _>("snapshot"))
                .map_err(|e| anyhow::anyhow!(e))?;
            if let Some(imports) = snapshot["imports"].as_array_mut() {
                imports.retain(|item| !retired_apps.as_array().unwrap().contains(&item["app_id"]));
                sqlx::query("UPDATE environment_services SET snapshot=?,receipt=NULL WHERE environment_id=? AND app_id=?").bind(snapshot.to_string()).bind(id).bind(row.get::<String,_>("app_id")).execute(&mut *tx).await?;
            }
        }
    }
    if action == "purge" {
        // Completed purge must also erase encrypted historical root material.
        sqlx::query("DELETE FROM iam_testing_phases WHERE environment_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE operations SET request_json=json_remove(request_json,'$.previous_encrypted_key') WHERE plane='production' AND resource=? AND request_json IS NOT NULL")
            .bind(id).execute(&mut *tx).await?;
        sqlx::query("UPDATE environments SET encrypted_key='',key_hash=?,name='Purged environment',description='' WHERE id=?").bind(format!("purged:{id}")).bind(id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM environment_services WHERE environment_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE operations SET state='accepted',error=NULL WHERE id=?")
        .bind(operation_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,'production','honeycomb-coordinator',?,?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(format!("environment.{action}.completed")).bind(id).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    let mut result = json!({"operation_id":operation_id,"environment_id":id,"state":final_state,"revision":revision,"generation":generation,"key_version":key_version,"operation_state":"accepted"});
    if let Some(credential) = application_credential {
        result["application_credential"] = credential;
    }
    Ok(result)
}
async fn apply_local(
    s: &State,
    id: &str,
    action: &str,
    operation_id: &str,
    lease: &str,
) -> Result<()> {
    if action == "retire" {
        return retire_local(s, id, operation_id, lease).await;
    }
    if !matches!(action, "clean" | "purge") {
        return Ok(());
    }
    if id == "production" {
        return Err(Error::forbidden());
    }
    let mut tx = s.db.begin().await?;
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE id=? AND lease_token=? AND lease_until>? AND state='pending')").bind(operation_id).bind(lease).bind(now()).fetch_one(&mut *tx).await?;
    if !held {
        return Err(Error::conflict("Lifecycle coordinator lease changed"));
    }

    sqlx::query("DELETE FROM environment_app_activity WHERE environment_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM environment_activity_events WHERE environment_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM environment_application_links WHERE environment_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM environment_imports WHERE environment_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM publication_archives WHERE operation_id IN (SELECT id FROM operations WHERE plane=?)").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM review_gates WHERE request_id IN (SELECT id FROM publication_requests WHERE plane=?)").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM decisions WHERE request_id IN (SELECT id FROM publication_requests WHERE plane=?)").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM discussions WHERE request_id IN (SELECT id FROM publication_requests WHERE plane=?)").bind(id).execute(&mut *tx).await?;
    // Fixed SQL literals keep destructive work explicitly limited to one test plane.
    for query in [
        "DELETE FROM publication_requests WHERE plane=?",
        "DELETE FROM releases WHERE plane=?",
        "DELETE FROM reviews WHERE plane=?",
        "DELETE FROM stars WHERE plane=?",
        "DELETE FROM downloads WHERE plane=?",
        "DELETE FROM applications WHERE plane=?",
        "DELETE FROM operations WHERE plane=?",
        "DELETE FROM drafts WHERE plane=?",
        "DELETE FROM webhook_events WHERE plane=?",
        "DELETE FROM reports WHERE plane=?",
        "DELETE FROM outbox WHERE plane=?",
        "DELETE FROM audit WHERE plane=?",
        "DELETE FROM telemetry_events WHERE plane=?",
    ] {
        sqlx::query(query).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

// Store only the public receipt fields needed to reconcile imports, never returned secrets.
fn accepted_imports(requested: &Value, received: &Value) -> std::result::Result<Value, String> {
    let missing =
        || "IAM has not confirmed every imported configuration and test visibility".to_string();
    let requested = requested.as_array().ok_or_else(missing)?;
    let received = received.as_array().ok_or_else(missing)?;
    let mut accepted = Vec::new();
    for source in requested {
        let matches: Vec<_> = received
            .iter()
            .filter(|r| r["app_id"] == source["app_id"])
            .collect();
        if matches.len() != 1 {
            return Err(missing());
        }
        let receipt = matches[0];
        if receipt["configuration_revision"] != source["configuration_revision"]
            || receipt["source_revision"] != source["source_revision"]
            || receipt["iam_revision"].as_i64().is_none_or(|r| r <= 0)
            || receipt["effective_configuration"] != source["configuration"]
            || receipt["visibility"] != "private"
        {
            return Err(missing());
        }
        accepted.push(json!({"app_id":receipt["app_id"],"iam_revision":receipt["iam_revision"]}));
    }
    Ok(json!(accepted))
}

async fn retire_local(s: &State, environment: &str, operation: &str, lease: &str) -> Result<()> {
    if environment == "production" {
        return Err(Error::forbidden());
    }
    let mut tx = s.db.begin().await?;
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE id=? AND resource=? AND kind='environment.retire' AND lease_token=? AND lease_until>? AND state='pending')")
        .bind(operation).bind(environment).bind(lease).bind(now()).fetch_one(&mut *tx).await?;
    if !held {
        return Err(Error::conflict("Retirement lease expired"));
    }
    let targets:Vec<String>=sqlx::query_scalar("SELECT app_id FROM retirement_targets WHERE operation_id=? AND environment_id=? ORDER BY app_id").bind(operation).bind(environment).fetch_all(&mut *tx).await?;
    if targets.is_empty() {
        return Err(Error::bad("Retirement has no application targets"));
    }
    for app in targets {
        for query in [
            "DELETE FROM publication_archives WHERE operation_id IN (SELECT id FROM operations WHERE plane=? AND resource=?)",
            "DELETE FROM review_gates WHERE request_id IN (SELECT id FROM publication_requests WHERE plane=? AND app_id=?)",
            "DELETE FROM decisions WHERE request_id IN (SELECT id FROM publication_requests WHERE plane=? AND app_id=?)",
            "DELETE FROM discussions WHERE request_id IN (SELECT id FROM publication_requests WHERE plane=? AND app_id=?)",
            "DELETE FROM publication_requests WHERE plane=? AND app_id=?",
            "DELETE FROM releases WHERE plane=? AND app_id=?",
            "DELETE FROM reviews WHERE plane=? AND app_id=?",
            "DELETE FROM stars WHERE plane=? AND app_id=?",
            "DELETE FROM downloads WHERE plane=? AND app_id=?",
            "DELETE FROM applications WHERE plane=? AND app_id=?",
            "DELETE FROM operations WHERE plane=? AND resource=?",
            "DELETE FROM webhook_events WHERE plane=? AND resource=?",
            "DELETE FROM audit WHERE plane=? AND resource=?",
            "DELETE FROM outbox WHERE plane=? AND json_extract(payload,'$.app_id')=?",
            "DELETE FROM environment_imports WHERE environment_id=? AND app_id=?",
            "DELETE FROM environment_application_links WHERE environment_id=? AND app_id=?",
            "DELETE FROM environment_app_activity WHERE environment_id=? AND app_id=?",
            "DELETE FROM environment_activity_events WHERE environment_id=? AND app_id=?",
        ] {
            sqlx::query(query)
                .bind(environment)
                .bind(&app)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
