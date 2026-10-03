//! Feature-triggered storage consent and encrypted durable credential rotation.
use crate::{
    State,
    api::{context, key},
    auth::{Identity, IdentityProvider},
    error::{Error, Result},
    now,
};
use axum::{
    Json, Router,
    extract::{Path, State as S},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use silicon_iam_client::models::OboTokenPair;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;

pub const ENDPOINTS: [&str; 4] = [
    "briefcase.uploads.reserve",
    "briefcase.uploads.commit",
    "briefcase.link_access.update",
    "briefcase.files.read",
];
pub fn required(org: &str) -> Error {
    let mut e = Error::new(
        StatusCode::FORBIDDEN,
        "storage_authorization_required",
        "Authorize Briefcase storage to continue, then retry the same operation.",
    );
    e.1.details.push(format!("org_id={org}"));
    e
}
pub struct Broker {
    pub db: SqlitePool,
    pub identity: Arc<dyn IdentityProvider>,
    pub encryption_key: [u8; 32],
    pub audience: String,
    pub allowed_origins: Vec<String>,
    pub iam_origin: String,
}
impl Broker {
    async fn caller(
        &self,
        token: &str,
        environment: Option<&str>,
        org: &str,
    ) -> Result<(Identity, String, i64)> {
        let identity = self.identity.authenticate(token, environment).await?;
        if !identity.member(org) {
            return Err(Error::forbidden());
        }
        if let Some(environment) = environment {
            let row: Option<(String, i64)> = sqlx::query_as(
                "SELECT id,generation FROM environments WHERE key_hash=? AND state='ready'",
            )
            .bind(hex::encode(Sha256::digest(environment)))
            .fetch_optional(&self.db)
            .await?;
            let (plane, generation) = row.ok_or_else(Error::unauthorized)?;
            if identity.testing_environment_id.as_deref() != Some(plane.as_str()) {
                return Err(Error::unauthorized());
            }
            Ok((identity, plane, generation))
        } else {
            if identity.testing_environment_id.is_some() {
                return Err(Error::unauthorized());
            }
            Ok((identity, "production".into(), 0))
        }
    }
    fn callback(&self, uri: &str) -> Result<()> {
        let url = url::Url::parse(uri).map_err(|_| Error::bad("Invalid storage callback URL"))?;
        let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if uri.len() > 2048
            || url.as_str() != uri
            || url.path() != "/storage-authorization"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || !(url.scheme() == "https" || url.scheme() == "http" && loopback)
            || !self
                .allowed_origins
                .contains(&url.origin().ascii_serialization())
        {
            return Err(Error::bad(
                "Storage callback must use an allowed Honeycomb origin and /storage-authorization",
            ));
        }
        Ok(())
    }
    pub async fn start(
        &self,
        org: &str,
        return_url: Option<&str>,
        token: &str,
        environment: Option<&str>,
        idem: &str,
    ) -> Result<Value> {
        self.start_with_redirect(org, return_url, None, token, environment, idem)
            .await
    }
    pub async fn start_with_redirect(
        &self,
        org: &str,
        return_url: Option<&str>,
        redirect_url: Option<&str>,
        token: &str,
        environment: Option<&str>,
        idem: &str,
    ) -> Result<Value> {
        if let Some(uri) = return_url {
            self.callback(uri)?;
        }
        if let Some(destination) = redirect_url {
            let callback = return_url
                .and_then(|s| url::Url::parse(s).ok())
                .ok_or_else(|| Error::bad("redirect_url requires a Honeycomb return_url"))?;
            let target =
                url::Url::parse(destination).map_err(|_| Error::bad("Invalid redirect_url"))?;
            if destination.len() > 2048
                || target.as_str() != destination
                || target.origin() != callback.origin()
                || !target.username().is_empty()
                || target.password().is_some()
                || target.fragment().is_some()
                || target.path() == "/storage-authorization"
                || target.path() == "/login-complete"
                || target.path() == "/auth"
                || target.path().starts_with("/auth/")
            {
                return Err(Error::bad(
                    "redirect_url must use the same Honeycomb origin and a normal application page",
                ));
            }
        }
        let return_url = return_url.unwrap_or("");
        let (identity, plane, generation) = self.caller(token, environment, org).await?;
        let hash = hex::encode(Sha256::digest(match redirect_url {
            Some(url) => format!("{org}:{return_url}:redirect:{url}"),
            None => format!("{org}:{return_url}"),
        }));
        let id = uuid::Uuid::new_v4().to_string();
        let state = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let mut body = json!({"subject_token":token,"org_id":org,"endpoints":ENDPOINTS.iter().map(|endpoint|json!({"audience":self.audience,"endpoint_id":endpoint})).collect::<Vec<_>>(),"redirect_uri":return_url,"state":state});
        if return_url.is_empty() {
            body.as_object_mut().unwrap().remove("redirect_uri");
            body.as_object_mut().unwrap().remove("state");
        }
        sqlx::query("INSERT OR IGNORE INTO storage_authorizations(id,plane,generation,actor,org_id,idempotency_key,request_hash,callback_state,redirect_uri,expires_at,exchange_key,created_at,encrypted_request,post_redirect_url) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
   .bind(&id).bind(&plane).bind(generation).bind(&identity.principal_id).bind(org).bind(idem).bind(&hash).bind(&state).bind(return_url).bind(now()+600).bind(uuid::Uuid::new_v4().to_string()).bind(now()).bind(crate::encrypt(&self.encryption_key,&body.to_string())?).bind(redirect_url).execute(&self.db).await?;
        let row=sqlx::query("SELECT * FROM storage_authorizations WHERE plane=? AND generation=? AND actor=? AND idempotency_key=?").bind(&plane).bind(generation).bind(&identity.principal_id).bind(idem).fetch_one(&self.db).await?;
        if row.get::<String, _>("request_hash") != hash {
            return Err(Error::conflict(
                "Idempotency key belongs to another storage request",
            ));
        }
        if row.get::<i64, _>("expires_at") <= now() {
            return Err(Error::conflict(
                "Storage authorization expired; start a new request",
            ));
        }
        let id: String = row.get("id");
        if row.get::<Option<String>, _>("iam_request_id").is_none() {
            let body: Value = serde_json::from_str(&crate::decrypt(
                &self.encryption_key,
                &row.get::<String, _>("encrypted_request"),
            )?)
            .map_err(|e| anyhow::anyhow!(e))?;
            let result = self
                .identity
                .obo_operation("authorize", body, &id, environment)
                .await?;
            let request = result["id"]
                .as_str()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .ok_or_else(|| Error::unavailable("IAM omitted storage authorization ID"))?;
            let consent = result["authorization_url"]
                .as_str()
                .and_then(|s| url::Url::parse(s).ok())
                .ok_or_else(|| Error::unavailable("IAM omitted storage consent URL"))?;
            if consent.origin().ascii_serialization() != self.iam_origin
                || consent.path() != "/obo/consent"
                || consent
                    .query_pairs()
                    .find(|(k, _)| k == "request")
                    .map(|(_, v)| v.to_string())
                    != Some(request.to_string())
            {
                return Err(Error::unavailable(
                    "IAM returned an unexpected storage consent URL",
                ));
            }
            sqlx::query("UPDATE storage_authorizations SET iam_request_id=?,consent_url=?,encrypted_request=NULL WHERE id=?").bind(request.to_string()).bind(consent.as_str()).bind(&id).execute(&self.db).await?;
        }
        let row=sqlx::query("SELECT iam_request_id,consent_url,callback_state,status FROM storage_authorizations WHERE id=?").bind(id).fetch_one(&self.db).await?;
        Ok(
            json!({"authorization_id":row.get::<String,_>("iam_request_id"),"consent_url":row.get::<String,_>("consent_url"),"state":row.get::<String,_>("callback_state"),"status":row.get::<String,_>("status")}),
        )
    }
    pub async fn complete(
        &self,
        id: uuid::Uuid,
        code: &str,
        state: &str,
        token: &str,
        environment: Option<&str>,
    ) -> Result<Value> {
        // Lookup provides no authority: the current authenticated identity and plane are rechecked before exchange.
        let row = sqlx::query("SELECT * FROM storage_authorizations WHERE iam_request_id=?")
            .bind(id.to_string())
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(Error::missing)?;
        let org: String = row.get("org_id");
        let (identity, plane, generation) = self.caller(token, environment, &org).await?;
        if row.get::<String, _>("actor") != identity.principal_id
            || row.get::<String, _>("plane") != plane
            || row.get::<i64, _>("generation") != generation
            || row.get::<String, _>("callback_state") != state
        {
            return Err(Error::unauthorized());
        }
        if row.get::<String, _>("status") == "ready" {
            return Ok(
                json!({"authorization_id":id,"status":"ready","redirect_url":row.get::<Option<String>,_>("post_redirect_url")}),
            );
        }
        if row.get::<i64, _>("expires_at") <= now() {
            return Err(Error::bad("Storage authorization expired"));
        }
        let result = self
            .identity
            .obo_operation(
                "exchange",
                json!({"authorization_id":id,"code":code}),
                &row.get::<String, _>("exchange_key"),
                environment,
            )
            .await?;
        let response: silicon_iam_client::models::OboTokenResponse = serde_json::from_value(result)
            .map_err(|_| Error::unavailable("Invalid IAM storage credentials"))?;
        let mut selected = std::collections::BTreeSet::new();
        for pair in &response.items {
            if pair.audience != self.audience
                || !ENDPOINTS.contains(&pair.endpoint_id.as_str())
                || !selected.insert(pair.endpoint_id.clone())
                || environment.is_some() != pair.testing_context.is_some()
            {
                return Err(Error::unavailable("IAM returned a different storage grant"));
            }
        }
        if selected.len() != ENDPOINTS.len() {
            return Err(Error::unavailable(
                "IAM did not approve all storage operations",
            ));
        }
        let mut tx = self.db.begin().await?;
        for pair in response.items {
            let encrypted = crate::encrypt(
                &self.encryption_key,
                &serde_json::to_string(&pair).map_err(|e| anyhow::anyhow!(e))?,
            )?;
            sqlx::query("INSERT INTO storage_grants(plane,generation,actor,org_id,endpoint_id,encrypted_pair,expires_at,updated_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(plane,generation,actor,org_id,endpoint_id) DO UPDATE SET encrypted_pair=excluded.encrypted_pair,expires_at=excluded.expires_at,refresh_key=NULL,lease_token=NULL,lease_until=NULL,updated_at=excluded.updated_at")
    .bind(&plane).bind(generation).bind(&identity.principal_id).bind(&org).bind(&pair.endpoint_id).bind(encrypted).bind(pair.expires_at.unix_timestamp()).bind(now()).execute(&mut *tx).await?;
        }
        sqlx::query(
            "UPDATE storage_authorizations SET status='ready',encrypted_request=NULL WHERE id=?",
        )
        .bind(row.get::<String, _>("id"))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(
            json!({"authorization_id":id,"status":"ready","redirect_url":row.get::<Option<String>,_>("post_redirect_url")}),
        )
    }
    pub async fn token(
        &self,
        endpoint: &str,
        token: &str,
        environment: Option<&str>,
        org: &str,
    ) -> Result<OboTokenPair> {
        let (identity, plane, generation) = self.caller(token, environment, org).await?;
        let mut tx = self.db.begin().await?;
        let row=sqlx::query("SELECT * FROM storage_grants WHERE plane=? AND generation=? AND actor=? AND org_id=? AND endpoint_id=?")
   .bind(&plane).bind(generation).bind(&identity.principal_id).bind(org).bind(endpoint).fetch_optional(&mut *tx).await?.ok_or_else(||required(org))?;
        let pair: OboTokenPair = serde_json::from_str(&crate::decrypt(
            &self.encryption_key,
            &row.get::<String, _>("encrypted_pair"),
        )?)
        .map_err(|_| Error::unavailable("Stored storage grant is invalid"))?;
        if row.get::<i64, _>("expires_at") > now() + 15 {
            tx.commit().await?;
            return Ok(pair);
        }
        if row
            .get::<Option<i64>, _>("lease_until")
            .is_some_and(|t| t > now())
        {
            return Err(Error::conflict(
                "Storage authorization is refreshing; retry the same operation shortly",
            ));
        }
        let key = row
            .get::<Option<String>, _>("refresh_key")
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let lease = uuid::Uuid::new_v4().to_string();
        sqlx::query("UPDATE storage_grants SET refresh_key=?,lease_token=?,lease_until=? WHERE plane=? AND generation=? AND actor=? AND org_id=? AND endpoint_id=?")
   .bind(&key).bind(&lease).bind(now()+90).bind(&plane).bind(generation).bind(&identity.principal_id).bind(org).bind(endpoint).execute(&mut *tx).await?;
        tx.commit().await?;
        let refreshed = self
            .identity
            .obo_operation(
                "refresh",
                json!({"refresh_token":pair.refresh_token}),
                &key,
                environment,
            )
            .await;
        let result:Result<OboTokenPair>=async{
   let response:silicon_iam_client::models::OboTokenResponse=serde_json::from_value(refreshed?).map_err(|_|Error::unavailable("Invalid refreshed storage grant"))?;
   let [updated]:[OboTokenPair;1]=response.items.try_into().map_err(|_|Error::unavailable("IAM returned an unexpected storage grant count"))?;
   if updated.grant_id!=pair.grant_id || updated.audience!=self.audience || updated.endpoint_id!=endpoint || environment.is_some()!=updated.testing_context.is_some(){return Err(Error::unavailable("IAM returned a different storage grant"));}
   let encrypted=crate::encrypt(&self.encryption_key,&serde_json::to_string(&updated).map_err(|e|anyhow::anyhow!(e))?)?;
   let saved=sqlx::query("UPDATE storage_grants SET encrypted_pair=?,expires_at=?,refresh_key=NULL,lease_token=NULL,lease_until=NULL,updated_at=? WHERE plane=? AND generation=? AND actor=? AND org_id=? AND endpoint_id=? AND lease_token=?")
    .bind(encrypted).bind(updated.expires_at.unix_timestamp()).bind(now()).bind(&plane).bind(generation).bind(&identity.principal_id).bind(org).bind(endpoint).bind(&lease).execute(&self.db).await?;
   if saved.rows_affected()!=1{return Err(Error::conflict("Storage grant changed; retry the same operation"));}Ok(updated)
  }.await;
        if result.is_err() {
            sqlx::query("UPDATE storage_grants SET lease_until=NULL,lease_token=NULL WHERE plane=? AND generation=? AND actor=? AND org_id=? AND endpoint_id=? AND lease_token=?")
   .bind(&plane).bind(generation).bind(&identity.principal_id).bind(org).bind(endpoint).bind(&lease).execute(&self.db).await?;
        }
        result
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    org_id: String,
    return_url: Option<String>,
    redirect_url: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Complete {
    code: String,
    state: String,
}
pub fn router() -> Router<State> {
    Router::new()
        .route("/api/v1/storage-authorizations", post(start))
        .route("/api/v1/storage-authorizations/{id}", get(status))
        .route(
            "/api/v1/storage-authorizations/{id}/complete",
            post(complete),
        )
}
async fn start(S(s): S<State>, h: HeaderMap, Json(input): Json<Start>) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    let token = c.token.as_deref().ok_or_else(Error::unauthorized)?;
    let broker = s
        .storage
        .authorization_broker()
        .ok_or_else(|| required(&input.org_id))?;
    Ok(Json(
        broker
            .start_with_redirect(
                &input.org_id,
                input.return_url.as_deref(),
                input.redirect_url.as_deref(),
                token,
                c.environment.as_deref(),
                key(&h)?,
            )
            .await?,
    ))
}
async fn complete(
    S(s): S<State>,
    Path(id): Path<uuid::Uuid>,
    h: HeaderMap,
    Json(input): Json<Complete>,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    let token = c.token.as_deref().ok_or_else(Error::unauthorized)?;
    let _ = key(&h)?;
    let broker = s
        .storage
        .authorization_broker()
        .ok_or_else(|| Error::unavailable("Storage authorization is not configured"))?;
    Ok(Json(
        broker
            .complete(
                id,
                &input.code,
                &input.state,
                token,
                c.environment.as_deref(),
            )
            .await?,
    ))
}

async fn status(S(s): S<State>, Path(id): Path<uuid::Uuid>, h: HeaderMap) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    let token = c.token.as_deref().ok_or_else(Error::unauthorized)?;
    let broker = s
        .storage
        .authorization_broker()
        .ok_or_else(|| Error::unavailable("Storage authorization is not configured"))?;
    let row = sqlx::query("SELECT * FROM storage_authorizations WHERE iam_request_id=?")
        .bind(id.to_string())
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)?;
    let (identity, plane, generation) = broker
        .caller(
            token,
            c.environment.as_deref(),
            &row.get::<String, _>("org_id"),
        )
        .await?;
    if row.get::<String, _>("actor") != identity.principal_id
        || row.get::<String, _>("plane") != plane
        || row.get::<i64, _>("generation") != generation
    {
        return Err(Error::missing());
    }
    Ok(Json(
        json!({"authorization_id":id,"consent_url":row.get::<String,_>("consent_url"),"state":row.get::<String,_>("callback_state"),"status":row.get::<String,_>("status"),"expires_at":row.get::<i64,_>("expires_at")}),
    ))
}
