//! Recoverable public activation: IAM acceptance, archive sharing, then current-state reconciliation.
use crate::{
    State,
    api::{Context, context, find_app, key, revision},
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    extract::{Path, State as S},
    http::{HeaderMap, StatusCode},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;

pub async fn activate(
    S(s): S<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Value>)> {
    let c = context(&s, &h).await?;
    let k = key(&h)?;
    let expected = revision(&h)?;
    let publication = sqlx::query("SELECT * FROM publication_requests WHERE id=? AND plane=?")
        .bind(&id)
        .bind(&c.plane)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)?;
    let app_id: String = publication.get("app_id");
    let app = find_app(&s, &c, &app_id, true).await?;
    let digest = hex::encode(Sha256::digest(
        json!({"kind":"publication.activate","request_id":id,"revision":expected}).to_string(),
    ));
    let mut tx = s.db.begin().await?;
    let existing=sqlx::query("SELECT id,request_hash,state FROM operations WHERE plane=? AND actor=? AND idempotency_key=?").bind(&c.plane).bind(&c.identity()?.principal_id).bind(k).fetch_optional(&mut *tx).await?;
    let op = if let Some(row) = existing {
        if row.get::<String, _>("request_hash") != digest {
            return Err(Error::conflict(
                "Idempotency key already used for a different operation",
            ));
        }
        row.get::<String, _>("id")
    } else {
        if publication.get::<String, _>("state") != "awaiting_activation"
            || expected != app.revision
            || expected != publication.get::<i64, _>("revision")
        {
            return Err(Error::conflict(
                "Publication requires all current approvals; resume an existing activation with its original idempotency key",
            ));
        }
        let unapproved: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM review_gates WHERE request_id=? AND state!='approved')",
        )
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
        let gates: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM review_gates WHERE request_id=?")
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
        if unapproved || gates == 0 {
            return Err(Error::conflict(
                "All provider and Honeycomb approvals must be accepted",
            ));
        }
        let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE plane=? AND resource=? AND state='pending' AND kind IN ('webhook.approve','webhook.rotate','secret.rotate','release','publication.activate'))").bind(&c.plane).bind(&app_id).fetch_one(&mut *tx).await?;
        if busy {
            return Err(Error::conflict(
                "Finish the application's pending release or secret operation first",
            ));
        }
        let stable:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM applications WHERE plane=? AND app_id=? AND revision=? AND iam_revision=?)")
            .bind(&c.plane).bind(&app_id).bind(expected).bind(app.iam_revision).fetch_one(&mut *tx).await?;
        if !stable {
            return Err(Error::conflict(
                "Application changed before activation could start",
            ));
        }
        let configuration_operations:Vec<String>=sqlx::query_scalar("SELECT id FROM operations WHERE plane=? AND resource=? AND kind='configure' AND revision=? AND state='pending'")
            .bind(&c.plane).bind(&app_id).bind(expected).fetch_all(&mut *tx).await?;
        let releases =
            sqlx::query("SELECT version,storage_ref FROM releases WHERE plane=? AND app_id=?")
                .bind(&c.plane)
                .bind(&app_id)
                .fetch_all(&mut *tx)
                .await?;
        if releases.is_empty() {
            return Err(Error::conflict("Publication requires a CLI release"));
        }
        let approvals: Vec<String> = sqlx::query_scalar(
            "SELECT decision_id FROM review_gates WHERE request_id=? ORDER BY provider",
        )
        .bind(&id)
        .fetch_all(&mut *tx)
        .await?;
        let op = uuid::Uuid::new_v4().to_string();
        let configuration: Value =
            serde_json::from_str(&publication.get::<String, _>("config_snapshot"))
                .map_err(|e| anyhow::anyhow!(e))?;
        let request = json!({"operation_id":op,"request_id":id,"plan_id":publication.get::<String,_>("plan_id"),"app_id":app_id,"configuration_revision":expected,"expected_iam_revision":app.iam_revision,"configuration":configuration,"visibility":"public","decisions":approvals,"configuration_operations":configuration_operations});
        sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,request_json,created_at) VALUES(?,?,?,?,'publication.activate',?,?,?,?,?)")
            .bind(&op).bind(&c.plane).bind(&c.identity()?.principal_id).bind(k).bind(&app_id).bind(digest).bind(expected).bind(request.to_string()).bind(now()).execute(&mut *tx).await?;
        for release in releases {
            sqlx::query(
                "INSERT INTO publication_archives(operation_id,version,source_ref) VALUES(?,?,?)",
            )
            .bind(&op)
            .bind(release.get::<String, _>("version"))
            .bind(release.get::<String, _>("storage_ref"))
            .execute(&mut *tx)
            .await?;
        }
        let updated=sqlx::query("UPDATE publication_requests SET state='activating',activation_operation=? WHERE id=? AND activation_operation IS NULL").bind(&op).bind(&id).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(Error::conflict("Publication is already activating"));
        }
        op
    };
    tx.commit().await?;
    let lease = uuid::Uuid::new_v4().to_string();
    let acquired=sqlx::query("UPDATE operations SET lease_token=?,lease_until=? WHERE id=? AND state='pending' AND (lease_token IS NULL OR lease_until<?)").bind(&lease).bind(now()+600).bind(&op).bind(now()).execute(&s.db).await?;
    if acquired.rows_affected() == 1 {
        let result = coordinate(&s, &c, &op, &lease).await;
        if let Err(error) = result {
            sqlx::query(
                "UPDATE operations SET error=? WHERE id=? AND state='pending' AND lease_token=?",
            )
            .bind(error.1.message)
            .bind(&op)
            .bind(&lease)
            .execute(&s.db)
            .await?;
        }
        sqlx::query(
            "UPDATE operations SET lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
        )
        .bind(&op)
        .bind(&lease)
        .execute(&s.db)
        .await?;
    }
    let row = sqlx::query("SELECT state,error,idempotency_key FROM operations WHERE id=?")
        .bind(&op)
        .fetch_one(&s.db)
        .await?;
    let archives=sqlx::query("SELECT version,state,error FROM publication_archives WHERE operation_id=? ORDER BY version").bind(&op).fetch_all(&s.db).await?;
    let archives:Vec<Value>=archives.iter().map(|r|json!({"version":r.get::<String,_>("version"),"state":r.get::<String,_>("state"),"error":r.get::<Option<String>,_>("error")})).collect();
    Ok((
        StatusCode::ACCEPTED,
        Json(
            json!({"id":op,"request_id":id,"state":row.get::<String,_>("state"),"idempotency_key":row.get::<String,_>("idempotency_key"),"error":row.get::<Option<String>,_>("error"),"archives":archives}),
        ),
    ))
}
async fn renew(s: &State, id: &str, lease: &str) -> Result<()> {
    let held=sqlx::query("UPDATE operations SET lease_until=? WHERE id=? AND lease_token=? AND lease_until>? AND state='pending'").bind(now()+600).bind(id).bind(lease).bind(now()).execute(&s.db).await?;
    if held.rows_affected() != 1 {
        return Err(Error::conflict("Publication coordinator lease expired"));
    }
    Ok(())
}
fn configuration(request: &Value, response: &Value) -> Result<Value> {
    let mut config = response["effective_configuration"]
        .as_object()
        .cloned()
        .ok_or_else(|| Error::unavailable("IAM omitted its effective configuration"))?;
    if config.get("org_id") != request["configuration"].get("org_id")
        || config.get("local_app_id") != request["configuration"].get("local_app_id")
    {
        return Err(Error::unavailable(
            "IAM returned another application's configuration",
        ));
    }
    config.retain(|field, _| {
        request["configuration"].get(field).is_some()
            && field != "app_secret"
            && field != "webhook_secret"
    });
    Ok(Value::Object(config))
}
async fn coordinate(s: &State, c: &Context, id: &str, lease: &str) -> Result<()> {
    let op = sqlx::query("SELECT * FROM operations WHERE id=?")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    let request: Value = serde_json::from_str(&op.get::<String, _>("request_json"))
        .map_err(|e| anyhow::anyhow!(e))?;
    let app = request["app_id"].as_str().unwrap();
    let publication = request["request_id"].as_str().unwrap();
    let revision = request["configuration_revision"].as_i64().unwrap();
    let (current, org): (i64, String) =
        sqlx::query_as("SELECT revision,org_id FROM applications WHERE plane=? AND app_id=?")
            .bind(&c.plane)
            .bind(app)
            .fetch_one(&s.db)
            .await?;
    if current != revision {
        return Err(Error::conflict(
            "A newer configuration supersedes publication",
        ));
    }
    let token = c.token.as_deref().ok_or_else(Error::unauthorized)?;
    let accepted = if let Some(result) = op.get::<Option<String>, _>("result") {
        serde_json::from_str::<Value>(&result).map_err(|e| anyhow::anyhow!(e))?
    } else {
        let receipt = s
            .management
            .activate_publication(&request, token, c.environment.as_deref())
            .await?;
        if receipt["state"] != "accepted"
            || receipt["operation_id"] != id
            || receipt["request_id"] != publication
            || receipt["app_id"] != app
            || receipt["configuration_revision"] != revision
            || receipt["visibility"] != "public"
        {
            return Err(Error::unavailable(
                "IAM has not confirmed this publication operation",
            ));
        }
        let iam = receipt["iam_revision"]
            .as_i64()
            .filter(|r| *r > request["expected_iam_revision"].as_i64().unwrap())
            .ok_or_else(|| Error::unavailable("IAM returned an invalid publication revision"))?;
        let accepted =
            json!({"iam_revision":iam,"effective_configuration":configuration(&request,&receipt)?});
        let stored = sqlx::query(
            "UPDATE operations SET result=? WHERE id=? AND lease_token=? AND lease_until>?",
        )
        .bind(accepted.to_string())
        .bind(id)
        .bind(lease)
        .bind(now())
        .execute(&s.db)
        .await?;
        if stored.rows_affected() != 1 {
            return Err(Error::conflict(
                "Publication lease changed during IAM acceptance",
            ));
        }
        accepted
    };
    let archives=sqlx::query("SELECT version,source_ref FROM publication_archives WHERE operation_id=? AND state!='ready' ORDER BY version").bind(id).fetch_all(&s.db).await?;
    for archive in archives {
        renew(s, id, lease).await?;
        let version: String = archive.get("version");
        let reference: String = archive.get("source_ref");
        let archive_operation = uuid::Uuid::from_bytes(
            Sha256::digest(format!("{id}:{version}"))[..16]
                .try_into()
                .unwrap(),
        )
        .to_string();
        let published = match s
            .storage
            .publish(
                &reference,
                &org,
                token,
                c.environment.as_deref(),
                &archive_operation,
            )
            .await
        {
            Ok(reference) => reference,
            Err(error) => {
                sqlx::query(
                    "UPDATE publication_archives SET error=? WHERE operation_id=? AND version=?",
                )
                .bind(&error.1.message)
                .bind(id)
                .bind(&version)
                .execute(&s.db)
                .await?;
                return Err(error);
            }
        };
        let mut tx = s.db.begin().await?;
        let saved=sqlx::query("UPDATE publication_archives SET published_ref=?,state='ready',error=NULL WHERE operation_id=? AND version=? AND EXISTS(SELECT 1 FROM operations WHERE id=? AND lease_token=? AND lease_until>?)")
            .bind(&published).bind(id).bind(&version).bind(id).bind(lease).bind(now()).execute(&mut *tx).await?;
        if saved.rows_affected() != 1 {
            return Err(Error::conflict(
                "Publication lease changed during archive sharing",
            ));
        }
        let updated = sqlx::query("UPDATE releases SET storage_ref=? WHERE plane=? AND app_id=? AND version=? AND storage_ref=?").bind(published).bind(&c.plane).bind(app).bind(&version).bind(reference).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(Error::conflict("Release changed during archive sharing"));
        }
        tx.commit().await?;
    }
    renew(s, id, lease).await?;
    let live = s
        .management
        .application_state(app, token, c.environment.as_deref())
        .await?;
    if live["app_id"] != app
        || live["publication_request_id"] != publication
        || live["configuration_revision"] != revision
        || live["visibility"] != "public"
    {
        return Err(Error::unavailable(
            "IAM's current state no longer confirms this public revision; reconcile before publication",
        ));
    }
    let iam = live["iam_revision"]
        .as_i64()
        .filter(|r| *r >= accepted["iam_revision"].as_i64().unwrap())
        .ok_or_else(|| Error::unavailable("IAM returned stale publication state"))?;
    let effective = configuration(&request, &live)?;
    let mut tx = s.db.begin().await?;
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM operations WHERE id=? AND lease_token=? AND lease_until>? AND state='pending')").bind(id).bind(lease).bind(now()).fetch_one(&mut *tx).await?;
    if !held {
        return Err(Error::conflict("Publication coordinator lease changed"));
    }
    let newer: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM application_reconciliation WHERE plane=? AND app_id=? AND target_revision>?)")
        .bind(&c.plane).bind(app).bind(iam).fetch_one(&mut *tx).await?;
    if newer {
        return Err(Error::conflict(
            "A newer IAM notification must be reconciled before publication",
        ));
    }
    let updated=sqlx::query("UPDATE applications SET visibility='public',state='active',effective_config=?,effective_revision=?,iam_revision=?,updated_at=? WHERE plane=? AND app_id=? AND revision=? AND iam_revision<=?")
        .bind(effective.to_string()).bind(revision).bind(iam).bind(now()).bind(&c.plane).bind(app).bind(revision).bind(iam).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(Error::conflict(
            "A newer IAM snapshot supersedes publication",
        ));
    }
    sqlx::query("UPDATE operations SET state='accepted',error=NULL,result=? WHERE plane=? AND resource=? AND kind='configure' AND revision=? AND state='pending'")
        .bind(json!({"accepted_by_publication":id}).to_string()).bind(&c.plane).bind(app).bind(revision).execute(&mut *tx).await?;
    sqlx::query("UPDATE operations SET state='superseded',error='A newer publication configuration was accepted' WHERE plane=? AND resource=? AND kind='configure' AND revision<? AND state='pending'")
        .bind(&c.plane).bind(app).bind(revision).execute(&mut *tx).await?;
    sqlx::query("UPDATE publication_requests SET state='published',error=NULL WHERE id=?")
        .bind(publication)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE operations SET state='accepted',error=NULL WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT OR IGNORE INTO outbox(id,plane,event_key,kind,payload,created_at) VALUES(?,?,?,'publication.status',?,?)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&c.plane).bind(format!("publication:{publication}:published"))
        .bind(json!({"app_id":app,"org_id":effective["org_id"],"state":"published"}).to_string()).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
