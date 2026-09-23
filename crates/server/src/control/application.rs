//! Application-owned setup uses verified production identity, never human membership.
use super::*;
use crate::integration::TestingApplicationAuthority;

pub(crate) fn presented(h: &HeaderMap) -> Result<bool> {
    Ok(header_value(h, "authorization")?.is_some_and(|v| v.starts_with("Basic ")))
}
pub(crate) async fn authenticate(s: &State, h: &HeaderMap) -> Result<TestingApplicationAuthority> {
    if header_value(h, "x-testing-environment-key")?.is_some()
        || header_value(h, "x-honeycomb-actor-token")?.is_some()
    {
        return Err(Error::bad(
            "Application environment control requires production credentials; supply an attachment key in the request body",
        ));
    }
    let authorization = header_value(h, "authorization")?
        .filter(|v| v.starts_with("Basic ") && v.len() <= 4096)
        .ok_or_else(Error::unauthorized)?;
    let identity = s
        .management
        .verify_testing_application(authorization)
        .await?;
    if identity.application_id != identity.app_id
        || uuid::Uuid::parse_str(&identity.organization_id)
            .ok()
            .is_none_or(|id| id.is_nil())
        || !honeycomb_core::valid_app_id(&identity.app_id)
        || !honeycomb_core::valid_org_id(&identity.org_id)
        || identity.iam_revision <= 0
    {
        return Err(Error::unavailable(
            "IAM returned an invalid production application identity",
        ));
    }
    Ok(TestingApplicationAuthority {
        identity,
        authorization: authorization.to_owned(),
        attachment_key: None,
    })
}
pub(crate) fn owns(row: &SqliteRow, authority: &TestingApplicationAuthority) -> bool {
    row.get::<Option<String>, _>("creator_application_id")
        .as_deref()
        == Some(&authority.identity.application_id)
        && row.get::<Option<String>, _>("creator_app").as_deref()
            == Some(&authority.identity.app_id)
}
pub(crate) fn actor(authority: &TestingApplicationAuthority) -> String {
    format!("application:{}", authority.identity.application_id)
}
fn create_digest(application_id: &str, org: &str, body: &CreateEnvironment) -> String {
    hex::encode(Sha256::digest(json!({"kind":"application.environment.create","application_id":application_id,"org":org,"name":body.name,"description":body.description}).to_string()))
}
pub(crate) async fn coordinate(
    s: &State,
    h: &HeaderMap,
    id: &str,
    operation: &str,
) -> Result<Value> {
    if presented(h)? {
        let authority = authenticate(s, h).await?;
        crate::lifecycle::coordinate_application(s, id, operation, &authority).await
    } else {
        let token = header_value(h, "authorization")?
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or("");
        crate::lifecycle::coordinate(s, id, operation, token).await
    }
}
pub(crate) async fn list(s: &State, h: &HeaderMap) -> Result<Json<Value>> {
    let authority = authenticate(s, h).await?;
    // Current IAM links include apps imported only as dependencies. A listing link
    // never confers ownership, root-key access, or authority over another app.
    let discovered = s
        .management
        .testing_application_environment_ids(&authority.authorization)
        .await;
    let verified_ids = discovered.ok().and_then(|ids| {
        let mut unique = std::collections::BTreeSet::new();
        for id in ids {
            let id = uuid::Uuid::parse_str(&id).ok()?.to_string();
            if !unique.insert(id) {
                return None;
            }
        }
        Some(unique)
    });
    let discovery_complete = verified_ids.is_some();
    let ids = json!(verified_ids.unwrap_or_default()).to_string();
    let rows = sqlx::query("SELECT DISTINCT e.* FROM environments e LEFT JOIN environment_application_links l ON l.environment_id=e.id WHERE e.creator_application_id=? OR l.application_id=? OR e.id IN (SELECT value FROM json_each(?)) ORDER BY e.created_at DESC,e.id")
        .bind(&authority.identity.application_id).bind(&authority.identity.application_id).bind(ids).fetch_all(&s.db).await?;
    let mut items = vec![];
    for row in rows {
        let mut value = environment_value(&row);
        value["can_manage"] = json!(owns(&row, &authority));
        if let Some(link) = sqlx::query("SELECT state,last_activity FROM environment_application_links WHERE environment_id=? AND application_id=?")
            .bind(row.get::<String,_>("id")).bind(&authority.identity.application_id).fetch_optional(&s.db).await? {
            value["application_state"] = json!(link.get::<String,_>("state"));
            value["application_last_activity"] = json!(link.get::<i64,_>("last_activity"));
        }

        let services = sqlx::query("SELECT app_id,state,error FROM environment_services WHERE environment_id=? ORDER BY app_id").bind(row.get::<String,_>("id")).fetch_all(&s.db).await?;
        value["services"] = json!(services.iter().map(|r|json!({"app_id":r.get::<String,_>("app_id"),"state":r.get::<String,_>("state"),"error":r.get::<Option<String>,_>("error")})).collect::<Vec<_>>());
        items.push(value);
    }
    let mut response = json!({"items":items,"linkage_discovery":if discovery_complete {"complete"} else {"pending"}});
    if !discovery_complete {
        response["linkage_error"] = json!(
            "IAM application linkage discovery is pending; this list currently contains only locally verified links"
        );
    }
    Ok(Json(response))
}
pub(crate) async fn link(
    s: &State,
    id: &str,
    authority: &TestingApplicationAuthority,
    state: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO environment_application_links(environment_id,application_id,app_id,last_activity,state) VALUES(?,?,?,?,?) ON CONFLICT(environment_id,application_id) DO UPDATE SET last_activity=excluded.last_activity,state=excluded.state")
        .bind(id).bind(&authority.identity.application_id).bind(&authority.identity.app_id).bind(now()).bind(state).execute(&s.db).await?;
    Ok(())
}
pub(crate) async fn create(
    s: &State,
    h: &HeaderMap,
    body: CreateEnvironment,
) -> Result<(StatusCode, Json<Value>)> {
    let k = key(h)?;
    let mut authority = authenticate(s, h).await?;
    if !body.org_id.is_empty() && body.org_id != authority.identity.org_id {
        return Err(Error::forbidden());
    }
    if body.name.trim().is_empty() || body.name.len() > 100 || body.description.len() > 10000 {
        return Err(Error::bad(
            "name requires 1–100 characters; description allows up to 10000",
        ));
    }
    let attachment = body.attachment_key()?;
    let owner_actor = actor(&authority);
    let id;
    let operation;
    if let Some(key) = attachment {
        // A failed lookup is an error, never a request to create another environment.
        let row = sqlx::query("SELECT * FROM environments WHERE key_hash=? AND state='ready'")
            .bind(hex::encode(Sha256::digest(key)))
            .fetch_optional(&s.db)
            .await?
            .ok_or_else(|| {
                Error::bad("Attachment key is invalid or its environment is not active")
            })?;
        id = row.get::<String, _>("id");
        operation = k.to_owned();
        authority.attachment_key = Some(key.to_owned());
    } else {
        let digest = create_digest(
            &authority.identity.application_id,
            &authority.identity.org_id,
            &body,
        );
        let existing = sqlx::query("SELECT id,resource,request_hash FROM operations WHERE plane='production' AND actor=? AND idempotency_key=?").bind(&owner_actor).bind(k).fetch_optional(&s.db).await?;
        if let Some(existing) = existing {
            if existing.get::<String, _>("request_hash") != digest {
                // This old identity is cryptographic input only. Authentication,
                // operation ownership and the context binding are canonical.
                let context: Option<String> = sqlx::query_scalar("SELECT legacy_hash_application_id FROM application_create_hash_contexts WHERE operation_id=? AND application_id=?")
                    .bind(existing.get::<String,_>("id")).bind(&authority.identity.application_id).fetch_optional(&s.db).await?;
                if context.is_none_or(|context| {
                    create_digest(&context, &authority.identity.org_id, &body)
                        != existing.get::<String, _>("request_hash")
                }) {
                    return Err(Error::conflict(
                        "Idempotency key already used for different input",
                    ));
                }
            }
            id = existing.get("resource");
            operation = existing.get("id");
        } else {
            id = uuid::Uuid::new_v4().to_string();
            operation = uuid::Uuid::new_v4().to_string();
            let root: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(32)
                .map(char::from)
                .collect();
            let mut tx = s.db.begin().await?;
            sqlx::query("INSERT INTO environments(id,org_id,creator,creator_app,creator_application_id,name,description,encrypted_key,key_hash,created_at,last_activity) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                .bind(&id).bind(&authority.identity.org_id).bind(&owner_actor).bind(&authority.identity.app_id).bind(&authority.identity.application_id).bind(&body.name).bind(&body.description).bind(s.encrypt(&root)?).bind(hex::encode(Sha256::digest(&root))).bind(now()).bind(now()).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,'production',?,?,'environment.prepare',?,?,1,?)")
                .bind(&operation).bind(&owner_actor).bind(k).bind(&id).bind(digest).bind(now()).execute(&mut *tx).await?;
            for participant in [&s.iam_app_id, &s.app_id] {
                sqlx::query("INSERT INTO environment_services(environment_id,app_id,source_revision,snapshot,state,operation_id,generation) VALUES(?,?,0,'{}','pending',?,1)").bind(&id).bind(participant).bind(&operation).execute(&mut *tx).await?;
            }
            tx.commit().await?;
        }
        let prepared =
            crate::lifecycle::coordinate_application(s, &id, &operation, &authority).await?;
        if prepared["state"] != "ready" || prepared["operation_state"] == "pending" {
            return Ok((StatusCode::ACCEPTED, Json(prepared)));
        }
    }
    let row = sqlx::query("SELECT * FROM environments WHERE id=?")
        .bind(&id)
        .fetch_one(&s.db)
        .await?;
    if authority.attachment_key.is_none() && !owns(&row, &authority) {
        return Err(Error::forbidden());
    }
    let mut headers = h.clone();
    headers.insert(
        "if-match",
        row.get::<i64, _>("revision")
            .to_string()
            .parse()
            .map_err(|_| Error::bad("Invalid revision"))?,
    );
    // A stable second operation allows create retries to resume additive import.
    if authority.attachment_key.is_none() {
        headers.insert(
            "idempotency-key",
            format!("{operation}:import")
                .parse()
                .map_err(|_| Error::bad("Invalid operation key"))?,
        );
    }
    let (_, Json(mut result)) = crate::imports::import_application(
        s.clone(),
        headers,
        id.clone(),
        crate::imports::Import {
            app_id: authority.identity.app_id.clone(),
            release: None,
            refresh: false,
        },
        &authority,
    )
    .await?;
    result["environment_id"] = json!(id);
    result["org_id"] = json!(row.get::<String, _>("org_id"));
    result["app_id"] = json!(authority.identity.app_id);
    result["can_manage"] = json!(owns(&row, &authority));
    if result["state"] == "ready"
        && (result["operation_state"] == "accepted" || result["unchanged"] == true)
    {
        // The protected recovery contract may not yet be wired. Never return another
        // app's secret, invent credentials, or mark credential delivery complete.
        let delivered = result
            .as_object_mut()
            .and_then(|object| object.remove("application_credential"));
        let credential = match delivered {
            Some(value) => Ok(value),
            None => {
                s.management
                    .recover_testing_application_credential(&id, &authority.authorization)
                    .await
            }
        };
        match credential {
            Ok(credentials)
                if credentials["app_id"] == authority.identity.app_id
                    && credentials["environment_id"] == id
                    && credentials["app_secret"]
                        .as_str()
                        .is_some_and(|s| !s.is_empty()) =>
            {
                result["app_secret"] = credentials["app_secret"].clone();
                result["credential_state"] = json!("ready");
                link(s, &id, &authority, "ready").await?;
            }
            _ => {
                link(s, &id, &authority, "credential_pending").await?;
                result["credential_state"] = json!("pending");
                result["error"] = json!(
                    "Application test credential recovery is pending; retry the same request after IAM integration is available"
                );
            }
        }
    }
    Ok((StatusCode::ACCEPTED, Json(result)))
}
