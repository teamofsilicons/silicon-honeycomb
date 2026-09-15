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

struct FixtureIam;
#[async_trait]
impl IdentityProvider for FixtureIam {
    async fn login(&self, slt: &str, _: &str, _: Option<&str>) -> Result<Value> {
        let role = match slt {
            "fixture-owner" => "owner",
            "fixture-member" => "member",
            "fixture-outsider" => "outsider",
            _ => return Err(Error::unauthorized()),
        };
        Ok(
            json!({"access_token":format!("fixture-{role}-token"),"refresh_token":format!("fixture-{role}-refresh"),"expires_in":3600,"token_type":"Bearer"}),
        )
    }
    async fn refresh(&self, token: &str, _: &str, _: Option<&str>) -> Result<Value> {
        let role = token
            .strip_prefix("fixture-")
            .and_then(|t| t.strip_suffix("-refresh"))
            .ok_or_else(Error::unauthorized)?;
        self.login(&format!("fixture-{role}"), "", None).await
    }
    async fn revoke(&self, _: &str, _: &str, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    async fn authenticate(&self, token: &str, _: Option<&str>) -> Result<Identity> {
        let role = match token {
            "fixture-owner-token" => "org_owner",
            "fixture-member-token" => "org_member",
            "fixture-outsider-token" => "outsider",
            _ => return Err(Error::unauthorized()),
        };
        Ok(Identity {
            principal_id: token.into(),
            actor_type: Some("carbon".into()),
            organizations: BTreeMap::from([(
                if role == "outsider" { "other" } else { "tos" }.into(),
                Some(role.into()),
            )]),
            testing_environment_id: None,
            validator: false,
        })
    }
}
struct FixtureManagement;
#[async_trait]
impl Management for FixtureManagement {
    async fn configure(&self, body: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        Ok(
            json!({"state":"accepted","configuration_revision":body["configuration_revision"],"iam_revision":body["expected_iam_revision"].as_i64().unwrap()+1,"effective_configuration":body["configuration"]}),
        )
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
    async fn publish(&self, _: &str, _: &str, _: Option<&str>) -> Result<()> {
        Ok(())
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
        identity: Arc::new(FixtureIam),
        management: Arc::new(FixtureManagement),
        storage: Arc::new(FixtureStorage::default()),
        app_id: "tos>honeycomb".into(),
        iam_login_url: format!("http://127.0.0.1:{port}"),
        encryption_key: [7; 32],
        webhook_secret: "fixture-webhook-secret-0000000000000".into(),
    };
    let app = silicon_honeycomb_server::api::router(state).route("/login", get(fixture_login));
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    println!(
        "ISOLATED TEST FIXTURE http://127.0.0.1:{port}; identity and integrations are test doubles"
    );
    axum::serve(listener, app).await?;
    Ok(())
}
