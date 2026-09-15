//! Isolated local E2E fixture only. Never part of the production server or release image.
use async_trait::async_trait;
use axum::{extract::Query, response::Redirect, routing::get};
use serde_json::{Value, json};
use silicon_honeycomb_server::{
    State,
    auth::{Identity, IdentityProvider},
    error::{Error, Result},
    integration::{ArchiveStorage, Management},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct FixtureIam(Mutex<BTreeMap<String, FixtureSession>>);
struct FixtureSession {
    role: String,
    refresh: String,
    expires: std::time::Instant,
}
impl FixtureIam {
    fn issue(sessions: &mut BTreeMap<String, FixtureSession>, role: &str, seconds: u64) -> Value {
        let access = format!("oat_fixture_{}", uuid::Uuid::new_v4());
        let refresh = format!("ort_fixture_{}", uuid::Uuid::new_v4());
        sessions.insert(
            access.clone(),
            FixtureSession {
                role: role.into(),
                refresh: refresh.clone(),
                expires: std::time::Instant::now() + std::time::Duration::from_secs(seconds),
            },
        );
        json!({"access_token":access,"refresh_token":refresh,"expires_in":seconds,"token_type":"Bearer"})
    }
}
#[async_trait]
impl IdentityProvider for FixtureIam {
    async fn login(&self, slt: &str, _: &str, _: Option<&str>) -> Result<Value> {
        let role = match slt {
            "fixture-owner" => "org_owner",
            "fixture-member" => "org_member",
            "fixture-outsider" => "outsider",
            "fixture-validator" => "validator",
            _ => return Err(Error::unauthorized()),
        };
        let seconds = if std::env::var("HONEYCOMB_FIXTURE_SHORT_SESSION").as_deref() == Ok("1") {
            1
        } else {
            3600
        };
        Ok(Self::issue(&mut self.0.lock().unwrap(), role, seconds))
    }
    async fn refresh(&self, token: &str, _: &str, _: Option<&str>) -> Result<Value> {
        let mut sessions = self.0.lock().unwrap();
        let access = sessions
            .iter()
            .find(|(_, s)| s.refresh == token)
            .map(|(access, _)| access.clone())
            .ok_or_else(Error::unauthorized)?;
        let previous = sessions.remove(&access).unwrap();
        Ok(Self::issue(&mut sessions, &previous.role, 3600))
    }
    async fn revoke(&self, token: &str, _: &str, _: Option<&str>) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .retain(|access, session| access != token && session.refresh != token);
        Ok(())
    }
    async fn authenticate(&self, token: &str, _: Option<&str>) -> Result<Identity> {
        let sessions = self.0.lock().unwrap();
        let session = sessions
            .get(token)
            .filter(|s| s.expires > std::time::Instant::now())
            .ok_or_else(Error::unauthorized)?;
        let role = &session.role;
        Ok(Identity {
            principal_id: format!("fixture-{role}"),
            actor_type: Some("carbon".into()),
            organizations: BTreeMap::from([(
                if role == "outsider" { "other" } else { "tos" }.into(),
                Some(if role == "validator" {
                    "org_member".into()
                } else {
                    role.clone()
                }),
            )]),
            testing_environment_id: None,
            validator: role == "validator",
        })
    }
}
#[derive(Default)]
struct FixtureManagement(Mutex<BTreeMap<String, Value>>);
#[async_trait]
impl Management for FixtureManagement {
    async fn scope_catalog(&self, _: &str, provider: Option<&str>) -> Result<Value> {
        Ok(json!({"items": if provider.is_some(){json!([])}else{json!([
            {"scope":"self.identity.read","description":"Read the signed-in user's identity.","critical":false,"eligible":true},
            {"scope":"self.profile.read","description":"Read the signed-in user's profile.","critical":false,"eligible":true},
            {"scope":"directory.carbons.read","description":"Read the Carbon directory.","critical":true,"eligible":false}
        ])}}))
    }
    async fn configure(&self, body: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        Ok(
            json!({"state":"accepted","configuration_revision":body["configuration_revision"],"iam_revision":body["expected_iam_revision"].as_i64().unwrap()+1,"effective_configuration":body["configuration"]}),
        )
    }
    async fn publication_plan(&self, r: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        Ok(
            json!({"state":"accepted","request_id":r["request_id"],"app_id":r["app_id"],"configuration_revision":r["configuration_revision"],"plan_id":format!("fixture-plan-{}",r["request_id"].as_str().unwrap()),"gates":[]}),
        )
    }
    async fn activate_publication(&self, o: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        let result = json!({"state":"accepted","operation_id":o["operation_id"],"request_id":o["request_id"],"publication_request_id":o["request_id"],"app_id":o["app_id"],"configuration_revision":o["configuration_revision"],"iam_revision":o["expected_iam_revision"].as_i64().unwrap()+1,"visibility":"public","effective_configuration":o["configuration"]});
        self.0
            .lock()
            .unwrap()
            .insert(o["app_id"].as_str().unwrap().into(), result.clone());
        Ok(result)
    }
    async fn application_state(&self, app: &str, _: &str, _: Option<&str>) -> Result<Value> {
        self.0
            .lock()
            .unwrap()
            .get(app)
            .cloned()
            .ok_or_else(Error::missing)
    }
    async fn review_decision(&self, operation: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        let mut receipt = operation.clone();
        receipt["state"] = json!("accepted");
        Ok(receipt)
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        Err(Error::unavailable(
            "Fixture deliberately leaves shared lifecycle provisioning pending",
        ))
    }
}
#[derive(Default)]
struct FixtureStorage(Mutex<BTreeMap<String, Vec<u8>>>);
#[async_trait]
impl ArchiveStorage for FixtureStorage {
    async fn put(
        &self,
        app: &str,
        version: &str,
        path: &std::path::Path,
        _: &str,
        _: Option<&str>,
        _: &str,
    ) -> Result<String> {
        let id = format!("{app}/{version}");
        self.0.lock().unwrap().insert(
            id.clone(),
            std::fs::read(path).map_err(|e| anyhow::anyhow!(e))?,
        );
        Ok(id)
    }
    async fn read(&self, id: &str, _: Option<&str>, _: Option<&str>) -> Result<Vec<u8>> {
        self.0
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(Error::missing)
    }
    async fn publish(&self, reference: &str, _: &str, _: Option<&str>, _: &str) -> Result<String> {
        Ok(reference.into())
    }
}
async fn fixture_login(Query(q): Query<BTreeMap<String, String>>) -> Result<Redirect> {
    let mut callback = url::Url::parse(
        q.get("redirect_uri")
            .ok_or_else(|| Error::bad("Missing redirect_uri"))?,
    )
    .map_err(|_| Error::bad("Invalid callback"))?;
    if !matches!(callback.host_str(), Some("localhost" | "127.0.0.1")) {
        return Err(Error::bad("Fixture callbacks must use loopback"));
    }
    callback
        .query_pairs_mut()
        .append_pair("slt", "fixture-owner");
    Ok(Redirect::to(callback.as_str()))
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = std::env::var("HONEYCOMB_FIXTURE_PORT").unwrap_or_else(|_| "18080".into());
    let db = silicon_honeycomb_server::database("sqlite::memory:").await?;
    for (id, name, description, visibility) in [
        (
            "briefcase",
            "Briefcase",
            "Files, thoughtfully organized. A shared home for your team's documents, assets, and everyday work.",
            "public",
        ),
        (
            "waveform",
            "Waveform",
            "A voice for your silicons. Turn audio into text and give your applications natural speech.",
            "public",
        ),
        (
            "commit",
            "Commit",
            "Keep the next step in sight. Tasks and projects for teams of Carbons and Silicons.",
            "public",
        ),
        (
            "internal-tools",
            "Internal tools",
            "A private set of tools for the Team of Silicons organization. Available only to current members.",
            "private",
        ),
    ] {
        let app_id = format!("tos>{id}");
        let config = json!({"org_id":"tos","local_app_id":id,"name":name,"description":description,"website_url":"https://teamofsilicons.com","docs_url":format!("https://docs.{id}.teamofsilicons.com"),"base_url":format!("https://backend.{id}.teamofsilicons.com"),"app_scope":{"iam":["self.identity.read"],"external":[]},"obo_endpoints":[]});
        sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,visibility,state,revision,iam_revision,config,effective_config,webhook_secret,created_at,updated_at) VALUES('production',?,'tos',?,?,?,'active',1,1,?,?,?,1,1)").bind(app_id).bind(name).bind(description).bind(visibility).bind(config.to_string()).bind(config.to_string()).bind("fixture-no-secret").execute(&db).await?;
    }
    let state = State {
        db,
        identity: Arc::new(FixtureIam::default()),
        management: Arc::new(FixtureManagement::default()),
        storage: Arc::new(FixtureStorage::default()),
        app_id: "tos>honeycomb".into(),
        iam_login_url: format!("http://127.0.0.1:{port}"),
        encryption_key: [7; 32],
        webhook_secret: "fixture-webhook-secret-0000000000000".into(),
    };
    sqlx::query("UPDATE applications SET effective_revision=revision,webhook_secret=?")
        .bind(
            state
                .encrypt("fixture-webhook-secret-0000000000000")
                .map_err(|e| anyhow::anyhow!(e.1.message))?,
        )
        .execute(&state.db)
        .await?;
    for project in ["desktop", "mobile"] {
        let app_id = format!("fixture>review-{project}");
        let request_id = format!("fixture-review-{project}");
        let config = json!({"org_id":"fixture","local_app_id":format!("review-{project}"),"name":format!("Review candidate {project}"),"description":"An isolated application requesting access to Briefcase files.","app_scope":{"iam":[],"external":[{"app_id":"tos>briefcase","endpoint_id":"files.read"}]}});
        sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,state,revision,iam_revision,effective_revision,config,effective_config,webhook_secret,created_at,updated_at) VALUES('production',?,'fixture',?,'Isolated review candidate','active',1,1,1,?,?,'',1,1)")
            .bind(&app_id).bind(format!("Review candidate {project}")).bind(config.to_string()).bind(config.to_string()).execute(&state.db).await?;
        sqlx::query("INSERT INTO publication_requests(id,plane,app_id,revision,state,requested_by,created_at,config_snapshot,plan_id) VALUES(?,'production',?,1,'awaiting_scope_review','fixture-requester',1,?,'fixture-plan')")
            .bind(&request_id).bind(&app_id).bind(config.to_string()).execute(&state.db).await?;
        for (provider, scopes) in [
            ("tos>briefcase", json!(["files.read"])),
            ("honeycomb", json!([])),
        ] {
            sqlx::query("INSERT INTO review_gates(request_id,provider,scopes) VALUES(?,?,?)")
                .bind(&request_id)
                .bind(provider)
                .bind(scopes.to_string())
                .execute(&state.db)
                .await?;
        }

        sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,state,created_at,last_activity) VALUES(?,'tos','fixture-org_owner',?,'Isolated import journey',?,?,'ready',1,1)")
            .bind(format!("fixture-import-{project}")).bind(format!("Import sandbox {project}"))
            .bind(state.encrypt("fixture-import-root-key-0000000000").map_err(|e|anyhow::anyhow!(e.1.message))?).bind(format!("fixture-import-unused-hash-{project}")).execute(&state.db).await?;
    }
    let app = silicon_honeycomb_server::api::router(state).route("/login", get(fixture_login));
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    println!(
        "ISOLATED TEST FIXTURE http://127.0.0.1:{port}; identity and integrations are test doubles"
    );
    axum::serve(listener, app).await?;
    Ok(())
}
