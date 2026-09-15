use crate::{
    State,
    auth::Identity,
    error::{Error, Result},
    now,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State as ExtractState},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use honeycomb_core::{App, AppInput, Page, Release};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Row, sqlite::SqliteRow};

pub fn router(state: State) -> Router {
    let archive_limit = DefaultBodyLimit::max(honeycomb_core::package::MAX_ARCHIVE_BYTES as usize);
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/contract", get(contract))
        .route("/api/v1/iam", get(iam))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/status", get(status))
        .route("/api/v1/apps", get(search).post(create_app))
        .route("/api/v1/apps/{id}", get(get_app).put(update_app))
        .route(
            "/api/v1/apps/{id}/releases",
            get(releases).post(upload_release).layer(archive_limit),
        )
        .route("/api/v1/apps/{id}/download", get(download))
        .route("/api/v1/apps/{id}/reviews", get(reviews).put(review))
        .route("/api/v1/apps/{id}/star", put(star).delete(unstar))
        .route(
            "/api/v1/apps/{id}/publication",
            post(request_publication).get(publication),
        )
        .route("/api/v1/apps/{id}/publication/messages", post(message))
        .route("/api/v1/operations/{id}", get(operation))
        .route("/api/v1/operations/{id}/retry", post(retry_operation))
        .route(
            "/api/v1/packages/validate",
            post(validate_archive).layer(archive_limit),
        )
        .route(
            "/api/v1/organizations/{org}/drafts",
            get(super::control::drafts),
        )
        .route(
            "/api/v1/organizations/{org}/drafts/{id}",
            put(super::control::save_draft),
        )
        .route(
            "/api/v1/environments",
            get(super::control::environments).post(super::control::create_environment),
        )
        .route(
            "/api/v1/environments/{id}",
            get(super::control::environment),
        )
        .route(
            "/api/v1/environments/{id}/key",
            post(super::control::environment_key),
        )
        .route(
            "/api/v1/environments/{id}/actions/{action}",
            post(super::control::environment_action),
        )
        .route("/webhook/", post(super::control::webhook))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(axum::middleware::from_fn(response_headers))
        .with_state(state)
}
async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}
async fn contract() -> Json<Value> {
    Json(json!({"selected_version":"v1", "supported_versions":["v1"], "minimum_client":"0.1.0"}))
}
async fn response_headers(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        "honeycomb-api-version",
        header::HeaderValue::from_static("v1"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        header::HeaderValue::from_static("nosniff"),
    );
    response
}
pub(crate) fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>> {
    headers
        .get(name)
        .map(|h| {
            h.to_str()
                .map_err(|_| Error::bad(format!("Invalid {name} header")))
        })
        .transpose()
}
fn bearer(headers: &HeaderMap) -> Result<Option<&str>> {
    match header_value(headers, "authorization")? {
        Some(v) => v
            .strip_prefix("Bearer ")
            .filter(|v| !v.is_empty())
            .map(Some)
            .ok_or_else(Error::unauthorized),
        None => Ok(None),
    }
}
pub(crate) fn key(headers: &HeaderMap) -> Result<&str> {
    let k = header_value(headers, "idempotency-key")?
        .ok_or_else(|| Error::bad("Idempotency-Key is required for mutations"))?;
    if !(16..=255).contains(&k.len()) || !k.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(Error::bad(
            "Idempotency-Key must contain 16–255 visible ASCII characters",
        ));
    }
    Ok(k)
}
pub(crate) fn revision(headers: &HeaderMap) -> Result<i64> {
    header_value(headers, "if-match")?
        .and_then(|v| v.trim_matches('"').parse().ok())
        .filter(|n| *n >= 0)
        .ok_or_else(|| Error::bad("If-Match must contain the resource's current revision"))
}
pub(crate) struct Context {
    pub(crate) plane: String,
    pub(crate) environment: Option<String>,
    pub(crate) identity: Option<Identity>,
    pub(crate) token: Option<String>,
}
impl Context {
    pub(crate) fn identity(&self) -> Result<&Identity> {
        self.identity.as_ref().ok_or_else(Error::unauthorized)
    }
    pub(crate) fn admin(&self, org: &str) -> Result<&Identity> {
        let i = self.identity()?;
        if !i.admin(org) {
            return Err(Error::forbidden());
        }
        Ok(i)
    }
    pub(crate) fn member(&self, org: &str) -> bool {
        self.identity.as_ref().is_some_and(|i| i.member(org))
    }
}
pub(crate) async fn context(s: &State, h: &HeaderMap) -> Result<Context> {
    let environment = header_value(h, "x-testing-environment-key")?.map(str::to_owned);
    let plane = if let Some(env) = &environment {
        let hash = hex::encode(Sha256::digest(env));
        let row = sqlx::query("SELECT id,state FROM environments WHERE key_hash=?")
            .bind(hash)
            .fetch_optional(&s.db)
            .await?
            .ok_or_else(|| {
                Error::bad("Invalid testing environment key; no production fallback is permitted")
            })?;
        if row.get::<String, _>("state") != "ready" {
            return Err(Error::conflict("Testing environment is not ready"));
        }
        row.get("id")
    } else {
        "production".to_owned()
    };
    let token = bearer(h)?.map(str::to_owned);
    let identity = match &token {
        Some(t) => Some(s.identity.authenticate(t, environment.as_deref()).await?),
        None => None,
    };
    if let Some(identity) = &identity
        && ((plane == "production" && identity.testing_environment_id.is_some())
            || (plane != "production"
                && identity.testing_environment_id.as_deref() != Some(&plane)))
    {
        return Err(Error::unauthorized());
    }
    Ok(Context {
        plane,
        environment,
        identity,
        token,
    })
}
async fn iam(ExtractState(s): ExtractState<State>) -> Json<Value> {
    Json(json!({"app_id":s.app_id,"login_url":s.iam_login_url,"version":env!("CARGO_PKG_VERSION")}))
}
#[derive(Deserialize)]
struct Login {
    slt: String,
}
async fn login(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Json(body): Json<Login>,
) -> Result<Response> {
    if body.slt.is_empty() || body.slt.len() > 4096 {
        return Err(Error::bad(
            "slt is required and must be at most 4096 characters",
        ));
    }
    let env = header_value(&h, "x-testing-environment-key")?;
    if env.is_some() {
        context(&s, &h).await?;
    }
    let result = s.identity.login(&body.slt, key(&h)?, env).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}
#[derive(Deserialize)]
struct Refresh {
    refresh_token: String,
}
async fn refresh(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Json(body): Json<Refresh>,
) -> Result<Response> {
    let env = header_value(&h, "x-testing-environment-key")?;
    if env.is_some() {
        context(&s, &h).await?;
    }
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(
            s.identity
                .refresh(&body.refresh_token, key(&h)?, env)
                .await?,
        ),
    )
        .into_response())
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Logout {
    refresh_token: Option<String>,
}
async fn logout(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    body: Option<Json<Logout>>,
) -> Result<StatusCode> {
    // Revocation is authenticated by possession plus Honeycomb's IAM application credentials.
    // It must remain usable after the access token expires; do not introspect that token first.
    let mut environment_headers = h.clone();
    environment_headers.remove(header::AUTHORIZATION);
    let c = context(&s, &environment_headers).await?;
    let token = bearer(&h)?.ok_or_else(Error::unauthorized)?;
    let operation = key(&h)?;
    if let Some(refresh) = body.and_then(|b| b.0.refresh_token) {
        if refresh.is_empty() {
            return Err(Error::bad("refresh_token cannot be empty"));
        }
        let refresh_key = hex::encode(Sha256::digest(format!("{operation}:refresh")));
        s.identity
            .revoke(&refresh, &refresh_key, c.environment.as_deref())
            .await?;
    }
    s.identity
        .revoke(token, operation, c.environment.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn status(ExtractState(s): ExtractState<State>, h: HeaderMap) -> Result<Response> {
    let c = context(&s, &h).await?;
    let identity = c.identity()?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"authenticated":true,"identity":identity})),
    )
        .into_response())
}

fn app_from(row: &SqliteRow, admin: bool) -> Result<App> {
    let config: String = if admin {
        row.get("config")
    } else {
        row.get::<Option<String>, _>("effective_config")
            .unwrap_or_else(|| "{}".into())
    };
    let config: Value = serde_json::from_str(&config).map_err(|e| anyhow::anyhow!(e))?;
    let mut app = App {
        app_id: row.get("app_id"),
        org_id: row.get("org_id"),
        name: row.get("name"),
        description: row.get("description"),
        visibility: row.get("visibility"),
        state: row.get("state"),
        revision: row.get("revision"),
        iam_revision: row.get("iam_revision"),
        config,
        latest_version: None,
        rating: row.get::<Option<f64>, _>("rating").unwrap_or(0.0),
        reviews: row.get("reviews"),
        stars: row.get("stars"),
        installs: row.get("installs"),
    };
    if !admin {
        if let Some(name) = app.config["name"].as_str() {
            app.name = name.into();
        }
        if let Some(d) = app.config["description"].as_str() {
            app.description = d.into();
        }
    }
    Ok(app)
}
async fn find_app(s: &State, c: &Context, id: &str, manage: bool) -> Result<App> {
    let row = sqlx::query(concat!("SELECT a.*, (SELECT AVG(rating) FROM reviews r WHERE r.plane=a.plane AND r.app_id=a.app_id) AS rating, (SELECT COUNT(*) FROM reviews r WHERE r.plane=a.plane AND r.app_id=a.app_id) AS reviews, (SELECT COUNT(*) FROM stars r WHERE r.plane=a.plane AND r.app_id=a.app_id) AS stars, (SELECT COUNT(*) FROM downloads r WHERE r.plane=a.plane AND r.app_id=a.app_id) AS installs FROM applications a", " WHERE a.plane=? AND a.app_id=?"))
        .bind(&c.plane)
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)?;
    let org: String = row.get("org_id");
    let admin = c.identity.as_ref().is_some_and(|i| i.admin(&org));
    if manage {
        c.admin(&org)?;
    } else if !(row.get::<String, _>("visibility") == "public" || c.member(&org)) {
        return Err(Error::missing());
    }
    if !admin && row.get::<i64, _>("iam_revision") == 0 {
        return Err(Error::missing());
    }
    let mut app = app_from(&row, admin)?;
    app.latest_version = latest_version(s, c, id).await?;
    Ok(app)
}
async fn latest_version(s: &State, c: &Context, id: &str) -> Result<Option<String>> {
    let versions: Vec<String> =
        sqlx::query_scalar("SELECT version FROM releases WHERE plane=? AND app_id=?")
            .bind(&c.plane)
            .bind(id)
            .fetch_all(&s.db)
            .await?;
    Ok(versions
        .into_iter()
        .filter_map(|v| semver::Version::parse(&v).ok())
        .max()
        .map(|v| v.to_string()))
}
#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    q: String,
    #[serde(default = "one")]
    page: u32,
    #[serde(default)]
    managed: bool,
}
fn one() -> u32 {
    1
}
fn trigrams(s: &str) -> std::collections::BTreeSet<String> {
    let s = format!("  {} ", s.to_lowercase());
    let c: Vec<char> = s.chars().collect();
    c.windows(3).map(|w| w.iter().collect()).collect()
}
fn relevance(app: &App, q: &str, full_text: Option<f64>) -> i64 {
    if q.is_empty() {
        return 1;
    }
    let q = q.to_lowercase();
    let name = app.name.to_lowercase();
    let id = app.app_id.to_lowercase();
    if id == q {
        return 100_000;
    }
    if name == q {
        return 90_000;
    }
    if name.starts_with(&q) {
        return 80_000;
    }
    if name.contains(&q) || id.contains(&q) {
        return 70_000;
    }
    let weighted = full_text.map(|rank| (-rank * 1000.0).clamp(1.0, 9999.0) as i64);
    let words: Vec<&str> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    if let Some(rank) = weighted
        && words
            .iter()
            .any(|word| name.split_whitespace().any(|part| part.starts_with(word)))
    {
        return 60_000 + rank;
    }
    let a = trigrams(&name);
    let b = trigrams(&q);
    let similarity = 2.0 * a.intersection(&b).count() as f64 / (a.len() + b.len()).max(1) as f64;
    if similarity >= 0.25 {
        return 40_000 + (similarity * 1000.0) as i64;
    }
    weighted.map(|rank| 10_000 + rank).unwrap_or(0)
}
async fn search(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Query(q): Query<Search>,
) -> Result<Json<Page<App>>> {
    if q.page == 0 || q.q.len() > 200 {
        return Err(Error::bad(
            "page starts at 1; search query is limited to 200 characters",
        ));
    }
    let c = context(&s, &h).await?;
    if q.managed {
        c.identity()?;
    }
    let rows = sqlx::query(concat!("SELECT a.*, (SELECT AVG(rating) FROM reviews r WHERE r.plane=a.plane AND r.app_id=a.app_id) AS rating, (SELECT COUNT(*) FROM reviews r WHERE r.plane=a.plane AND r.app_id=a.app_id) AS reviews, (SELECT COUNT(*) FROM stars r WHERE r.plane=a.plane AND r.app_id=a.app_id) AS stars, (SELECT COUNT(*) FROM downloads r WHERE r.plane=a.plane AND r.app_id=a.app_id) AS installs FROM applications a", " WHERE a.plane=?"))
        .bind(&c.plane)
        .fetch_all(&s.db)
        .await?;
    // FTS weights identity/name above description. Literal token construction prevents
    // user query punctuation from becoming FTS operators or malformed syntax.
    let query =
        q.q.split(|ch: char| !ch.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(|word| format!("\"{word}\"*"))
            .collect::<Vec<_>>()
            .join(" OR ");
    let mut ranks = std::collections::HashMap::new();
    if !query.is_empty() {
        let ranked = sqlx::query("SELECT app_id,variant,bm25(catalog_search,0.0,12.0,0.0,8.0,1.0) AS rank FROM catalog_search WHERE catalog_search MATCH ? AND plane=?")
            .bind(query).bind(&c.plane).fetch_all(&s.db).await?;
        for row in ranked {
            ranks.insert(
                (
                    row.get::<String, _>("app_id"),
                    row.get::<String, _>("variant"),
                ),
                row.get::<f64, _>("rank"),
            );
        }
    }
    let mut matches = vec![];
    for row in rows {
        let org: String = row.get("org_id");
        let admin = c.identity.as_ref().is_some_and(|i| i.admin(&org));
        if q.managed && !admin {
            continue;
        }
        if row.get::<String, _>("visibility") != "public" && !c.member(&org) {
            continue;
        }
        if row.get::<i64, _>("iam_revision") == 0 && !admin {
            continue;
        }
        let app = app_from(&row, admin)?;
        let score = relevance(
            &app,
            q.q.trim(),
            ranks
                .get(&(
                    app.app_id.clone(),
                    if admin { "desired" } else { "effective" }.into(),
                ))
                .copied(),
        );
        if score > 0 {
            matches.push((score, app));
        }
    }
    matches.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.rating.total_cmp(&a.1.rating))
            .then_with(|| a.1.app_id.cmp(&b.1.app_id))
    });
    let total = matches.len();
    let mut items: Vec<App> = matches
        .into_iter()
        .skip((q.page as usize - 1).saturating_mul(20))
        .take(20)
        .map(|(_, a)| a)
        .collect();
    for a in &mut items {
        a.latest_version = latest_version(&s, &c, &a.app_id).await?;
    }
    Ok(Json(Page {
        items,
        page: q.page,
        per_page: 20,
        total,
    }))
}
async fn get_app(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response> {
    let c = context(&s, &h).await?;
    let app = find_app(&s, &c, &id, false).await?;
    Ok((
        [
            (header::CACHE_CONTROL, "private, no-store"),
            (header::ETAG, &format!("\"{}\"", app.revision)),
        ],
        Json(app),
    )
        .into_response())
}
fn validate_input(input: &AppInput) -> Result<()> {
    let errors = input.validate();
    if errors.is_empty() {
        Ok(())
    } else {
        let mut error = Error::bad("Application validation failed");
        error.1.details = errors;
        Err(error)
    }
}
fn hash(v: &Value) -> String {
    hex::encode(Sha256::digest(v.to_string()))
}
async fn existing_operation(
    s: &State,
    c: &Context,
    key: &str,
    request_hash: &str,
) -> Result<Option<Value>> {
    let actor = &c.identity()?.principal_id;
    let row =
        sqlx::query("SELECT * FROM operations WHERE plane=? AND actor=? AND idempotency_key=?")
            .bind(&c.plane)
            .bind(actor)
            .bind(key)
            .fetch_optional(&s.db)
            .await?;
    if let Some(row) = row {
        if row.get::<String, _>("request_hash") != request_hash {
            return Err(Error::conflict(
                "Idempotency key was already used for a different request",
            ));
        }
        return Ok(Some(operation_value(&row)));
    }
    Ok(None)
}
fn operation_value(r: &SqliteRow) -> Value {
    json!({"id":r.get::<String,_>("id"),"kind":r.get::<String,_>("kind"),"resource":r.get::<String,_>("resource"),"revision":r.get::<i64,_>("revision"),"state":r.get::<String,_>("state"),"error":r.get::<Option<String>,_>("error")})
}
async fn create_app(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Json(input): Json<AppInput>,
) -> Result<(StatusCode, Json<Value>)> {
    save_app(&s, &h, input, None, None).await
}
async fn update_app(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(mut input): Json<AppInput>,
) -> Result<(StatusCode, Json<Value>)> {
    if id != input.app_id() {
        return Err(Error::bad(
            "Application ID and owning organization are immutable",
        ));
    }
    let expected = revision(&h)?;
    let digest = hash(&json!({"kind":"configure","expected":expected,"input":input}));
    if input.webhook_secret.is_empty() {
        let c = context(&s, &h).await?;
        find_app(&s, &c, &id, true).await?;
        let encrypted: String = sqlx::query_scalar(
            "SELECT webhook_secret FROM applications WHERE plane=? AND app_id=?",
        )
        .bind(&c.plane)
        .bind(&id)
        .fetch_one(&s.db)
        .await?;
        input.webhook_secret = s.decrypt(&encrypted)?;
    }
    save_app(&s, &h, input, Some(expected), Some(digest)).await
}
async fn save_app(
    s: &State,
    h: &HeaderMap,
    input: AppInput,
    expected: Option<i64>,
    request_digest: Option<String>,
) -> Result<(StatusCode, Json<Value>)> {
    validate_input(&input)?;
    let c = context(s, h).await?;
    let actor = c.admin(&input.org_id)?;
    let app_id = input.app_id();
    let k = key(h)?;
    let digest = request_digest
        .unwrap_or_else(|| hash(&json!({"kind":"configure","expected":expected,"input":input})));
    if let Some(o) = existing_operation(s, &c, k, &digest).await? {
        return Ok((StatusCode::ACCEPTED, Json(o)));
    }
    let current = if expected.is_some() {
        Some(find_app(s, &c, &app_id, true).await?)
    } else {
        None
    };
    if let Some(a) = &current {
        if Some(a.revision) != expected {
            return Err(Error::conflict("Application revision has changed"));
        }
        let old = a.config["obo_endpoints"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for endpoint in &input.obo_endpoints {
            if old
                .iter()
                .any(|e| e["endpoint_id"] == endpoint.endpoint_id && e["path"] != endpoint.path)
            {
                return Err(Error::bad(
                    "An existing OBO endpoint_id cannot move to another path; create a new endpoint_id",
                ));
            }
        }
    }
    let revision = expected.unwrap_or(0) + 1;
    let op = uuid::Uuid::new_v4().to_string();
    let config = input.public_config();
    let secret = s.encrypt(&input.webhook_secret)?;
    let mut tx = s.db.begin().await?;
    if let Some(expected) = expected {
        let updated=sqlx::query("UPDATE applications SET name=?,description=?,config=?,webhook_secret=?,revision=?,updated_at=? WHERE plane=? AND app_id=? AND revision=?").bind(&input.name).bind(&input.description).bind(config.to_string()).bind(secret).bind(revision).bind(now()).bind(&c.plane).bind(&app_id).bind(expected).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(Error::conflict("Application revision has changed"));
        }
    } else {
        if c.plane != "production" {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM applications WHERE plane='production' AND app_id=?)",
            )
            .bind(&app_id)
            .fetch_one(&mut *tx)
            .await?;
            if exists {
                return Err(Error::conflict(
                    "A test-only app cannot claim a production app ID; import the existing application",
                ));
            }
        }
        sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,config,webhook_secret,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(&c.plane).bind(&app_id).bind(&input.org_id).bind(&input.name).bind(&input.description).bind(config.to_string()).bind(secret).bind(now()).bind(now()).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(&op).bind(&c.plane).bind(&actor.principal_id).bind(k).bind("configure").bind(&app_id).bind(digest).bind(revision).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    let result = apply_config(s, &c, &op).await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}
async fn apply_config(s: &State, c: &Context, id: &str) -> Result<Value> {
    let op = sqlx::query("SELECT * FROM operations WHERE id=? AND plane=?")
        .bind(id)
        .bind(&c.plane)
        .fetch_one(&s.db)
        .await?;
    let app_id: String = op.get("resource");
    let app = find_app(s, c, &app_id, true).await?;
    if app.revision != op.get::<i64, _>("revision") {
        return Err(Error::conflict(
            "A newer configuration supersedes this operation",
        ));
    }
    let encrypted: String =
        sqlx::query_scalar("SELECT webhook_secret FROM applications WHERE plane=? AND app_id=?")
            .bind(&c.plane)
            .bind(&app_id)
            .fetch_one(&s.db)
            .await?;
    let request = json!({"operation_id":id,"configuration_revision":app.revision,"expected_iam_revision":app.iam_revision,"app_id":app_id,"configuration":app.config,"webhook_secret":s.decrypt(&encrypted)?,"visibility":app.visibility});
    let result = s
        .management
        .configure(
            &request,
            c.token.as_deref().ok_or_else(Error::unauthorized)?,
            c.environment.as_deref(),
        )
        .await;
    match result {
        Ok(response)
            if response["state"] == "accepted"
                && response["configuration_revision"] == app.revision =>
        {
            let accepted = response["iam_revision"]
                .as_i64()
                .filter(|r| *r > app.iam_revision)
                .ok_or_else(|| Error::unavailable("IAM returned an invalid accepted revision"))?;
            let effective = response
                .get("effective_configuration")
                .filter(|value| value.is_object())
                .ok_or_else(|| Error::unavailable("IAM omitted its accepted configuration"))?;
            if effective["org_id"] != app.config["org_id"]
                || effective["local_app_id"] != app.config["local_app_id"]
            {
                return Err(Error::unavailable(
                    "IAM returned a configuration for another application",
                ));
            }
            // Restrict the stored public snapshot to known configuration fields, excluding secrets.
            let mut effective = effective.as_object().unwrap().clone();
            effective.retain(|field, _| {
                app.config.get(field).is_some()
                    && field != "webhook_secret"
                    && field != "app_secret"
            });
            let mut tx = s.db.begin().await?;
            let updated=sqlx::query("UPDATE applications SET effective_config=?,iam_revision=?,state='active' WHERE plane=? AND app_id=? AND revision=? AND iam_revision=?").bind(Value::Object(effective).to_string()).bind(accepted).bind(&c.plane).bind(&app_id).bind(app.revision).bind(app.iam_revision).execute(&mut *tx).await?;
            if updated.rows_affected() != 1 {
                return Err(Error::conflict(
                    "Newer configuration arrived during IAM acceptance; reconcile before retrying",
                ));
            }
            sqlx::query("UPDATE operations SET state='accepted',error=NULL WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            // App secrets are deliberately not persisted in operations or catalog records.
            Ok(
                json!({"id":id,"state":"accepted","resource":app_id,"revision":app.revision,"iam_revision":accepted,"app_secret":response.get("app_secret")}),
            )
        }
        other => {
            let message = match other {
                Err(e) => e.1.message,
                Ok(_) => "IAM has not accepted this configuration".into(),
            };
            sqlx::query("UPDATE operations SET error=? WHERE id=?")
                .bind(&message)
                .bind(id)
                .execute(&s.db)
                .await?;
            Ok(
                json!({"id":id,"state":"pending","resource":app_id,"revision":app.revision,"error":message}),
            )
        }
    }
}
async fn operation(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    let row = sqlx::query("SELECT * FROM operations WHERE id=? AND plane=? AND actor=?")
        .bind(id)
        .bind(&c.plane)
        .bind(&c.identity()?.principal_id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)?;
    Ok(Json(operation_value(&row)))
}
async fn retry_operation(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    let row = sqlx::query("SELECT * FROM operations WHERE id=? AND plane=? AND actor=?")
        .bind(&id)
        .bind(&c.plane)
        .bind(&c.identity()?.principal_id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)?;
    if row.get::<String, _>("state") == "accepted" {
        return Ok(Json(operation_value(&row)));
    }
    if row.get::<String, _>("kind") != "configure" {
        return Err(Error::bad(
            "Retry this operation through its original command using the original idempotency key",
        ));
    }
    Ok(Json(apply_config(&s, &c, &id).await?))
}
async fn validate_archive(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<honeycomb_core::package::Validation>> {
    context(&s, &h).await?.identity()?;
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let d = tempfile::tempdir()?;
        let path = d.path().join("archive.tar.gz");
        std::fs::write(&path, body)?;
        Ok(honeycomb_core::package::validate(&path))
    })
    .await
    .map_err(|e| anyhow::anyhow!(e))??;
    Ok(Json(result))
}
async fn upload_release(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<Value>)> {
    let c = context(&s, &h).await?;
    let app = find_app(&s, &c, &id, true).await?;
    let expected = revision(&h)?;
    if app.revision != expected {
        return Err(Error::conflict("Application revision changed"));
    }
    let k = key(&h)?;
    let (temp, path, manifest, sha) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("release.tar.gz");
        std::fs::write(&path, body)?;
        let v = honeycomb_core::package::validate(&path);
        if !v.valid {
            anyhow::bail!("{}", v.errors.join("\n"));
        }
        let sha = honeycomb_core::package::sha256(&path)?;
        Ok((temp, path, v.manifest.unwrap(), sha))
    })
    .await
    .map_err(|e| anyhow::anyhow!(e))?
    .map_err(|e| Error::bad(e.to_string()))?;
    if manifest.app_id != id {
        return Err(Error::bad(
            "Archive app_id differs from the application being released",
        ));
    }
    let version = manifest.version;
    let digest = hash(&json!({"kind":"release","app_id":id,"version":version,"sha256":sha}));
    let existing = existing_operation(&s, &c, k, &digest).await?;
    if existing.as_ref().is_some_and(|o| o["state"] == "accepted") {
        return Ok((StatusCode::OK, Json(existing.unwrap())));
    }
    let op = if let Some(o) = existing {
        o["id"].as_str().unwrap().to_owned()
    } else {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM releases WHERE plane=? AND app_id=? AND version=?)",
        )
        .bind(&c.plane)
        .bind(&id)
        .bind(&version)
        .fetch_one(&s.db)
        .await?;
        if exists {
            return Err(Error::conflict(
                "Release versions are immutable; increment honeycomb.yaml version and repack",
            ));
        }
        let op = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(&op).bind(&c.plane).bind(&c.identity()?.principal_id).bind(k).bind("release").bind(&id).bind(digest).bind(app.revision).bind(now()).execute(&s.db).await?;
        op
    };
    let mut reference = s
        .storage
        .put(
            &id,
            &version,
            &path,
            c.token.as_deref().unwrap(),
            c.environment.as_deref(),
            &op,
        )
        .await?;
    if app.visibility == "public" {
        reference = s
            .storage
            .publish(
                &reference,
                c.token.as_deref().unwrap(),
                c.environment.as_deref(),
                &op,
            )
            .await?;
    }
    let size = std::fs::metadata(&path)
        .map_err(|e| anyhow::anyhow!(e))?
        .len() as i64;
    let mut tx = s.db.begin().await?;
    sqlx::query("INSERT INTO releases(plane,app_id,version,sha256,size,storage_ref,created_at) VALUES(?,?,?,?,?,?,?)").bind(&c.plane).bind(&id).bind(&version).bind(&sha).bind(size).bind(reference).bind(now()).execute(&mut *tx).await?;
    sqlx::query("UPDATE operations SET state='accepted',error=NULL WHERE id=?")
        .bind(&op)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO outbox(id,plane,event_key,kind,payload,created_at) VALUES(?,?,?,?,?,?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&c.plane)
    .bind(format!("release:{op}"))
    .bind("release.created")
    .bind(json!({"app_id":id,"org_id":app.org_id,"version":version}).to_string())
    .bind(now())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    drop(temp);
    Ok((
        StatusCode::CREATED,
        Json(
            json!({"id":op,"state":"accepted","app_id":id,"version":version,"sha256":sha,"size":size}),
        ),
    ))
}
fn release_from(r: &SqliteRow) -> Release {
    Release {
        app_id: r.get("app_id"),
        version: r.get("version"),
        sha256: r.get("sha256"),
        size: r.get("size"),
        created_at: r.get("created_at"),
    }
}
async fn releases(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    find_app(&s, &c, &id, false).await?;
    let mut items: Vec<Release> = sqlx::query("SELECT * FROM releases WHERE plane=? AND app_id=?")
        .bind(&c.plane)
        .bind(id)
        .fetch_all(&s.db)
        .await?
        .iter()
        .map(release_from)
        .collect();
    items.sort_by(|a, b| {
        semver::Version::parse(&b.version)
            .ok()
            .cmp(&semver::Version::parse(&a.version).ok())
    });
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
struct Download {
    version: Option<String>,
}
async fn download(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<Download>,
) -> Result<Response> {
    let c = context(&s, &h).await?;
    let app = find_app(&s, &c, &id, false).await?;
    if app.iam_revision == 0 {
        return Err(Error::conflict("Application is awaiting IAM activation"));
    }
    let version = q
        .version
        .or(app.latest_version)
        .ok_or_else(Error::missing)?;
    let row = sqlx::query("SELECT * FROM releases WHERE plane=? AND app_id=? AND version=?")
        .bind(&c.plane)
        .bind(&id)
        .bind(&version)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(Error::missing)?;
    let bytes = s
        .storage
        .read(
            &row.get::<String, _>("storage_ref"),
            c.token.as_deref(),
            c.environment.as_deref(),
        )
        .await?;
    let sha = hex::encode(Sha256::digest(&bytes));
    if sha != row.get::<String, _>("sha256") {
        return Err(Error::unavailable(
            "Stored release checksum does not match its immutable manifest",
        ));
    }
    let receipt = header_value(&h, "x-download-receipt")?
        .filter(|s| uuid::Uuid::parse_str(s).is_ok())
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    sqlx::query("INSERT OR IGNORE INTO downloads(plane,app_id,receipt,created_at) VALUES(?,?,?,?)")
        .bind(&c.plane)
        .bind(&id)
        .bind(receipt)
        .bind(now())
        .execute(&s.db)
        .await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/gzip"),
            (header::CACHE_CONTROL, "private, no-store"),
            (
                header::HeaderName::from_static("x-checksum-sha256"),
                sha.as_str(),
            ),
        ],
        bytes,
    )
        .into_response())
}
#[derive(Deserialize)]
struct Review {
    rating: f64,
    review: String,
}
async fn review(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Review>,
) -> Result<Json<Value>> {
    key(&h)?;
    if !input.rating.is_finite()
        || !(0.0..=5.0).contains(&input.rating)
        || input.review.trim().is_empty()
        || input.review.len() > 10000
    {
        return Err(Error::bad(
            "rating must be 0–5 and review must contain 1–10000 characters",
        ));
    }
    let c = context(&s, &h).await?;
    let identity = c.identity()?;
    find_app(&s, &c, &id, false).await?;
    sqlx::query("INSERT INTO reviews(plane,app_id,actor,rating,review,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(plane,app_id,actor) DO UPDATE SET rating=excluded.rating,review=excluded.review,updated_at=excluded.updated_at").bind(&c.plane).bind(id).bind(&identity.principal_id).bind(input.rating).bind(input.review).bind(now()).execute(&s.db).await?;
    Ok(Json(json!({"saved":true})))
}
async fn reviews(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    find_app(&s, &c, &id, false).await?;
    let rows=sqlx::query("SELECT actor,rating,review,updated_at FROM reviews WHERE plane=? AND app_id=? ORDER BY updated_at DESC,actor LIMIT 100").bind(&c.plane).bind(id).fetch_all(&s.db).await?;
    Ok(Json(
        json!({"items":rows.iter().map(|r|json!({"actor":r.get::<String,_>("actor"),"rating":r.get::<f64,_>("rating"),"review":r.get::<String,_>("review"),"updated_at":r.get::<i64,_>("updated_at")})).collect::<Vec<_>>()}),
    ))
}
async fn star(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    set_star(s, h, id, true).await
}
async fn unstar(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    set_star(s, h, id, false).await
}
async fn set_star(s: State, h: HeaderMap, id: String, enabled: bool) -> Result<Json<Value>> {
    key(&h)?;
    let c = context(&s, &h).await?;
    let identity = c.identity()?;
    find_app(&s, &c, &id, false).await?;
    let q = if enabled {
        "INSERT OR IGNORE INTO stars(plane,app_id,actor) VALUES(?,?,?)"
    } else {
        "DELETE FROM stars WHERE plane=? AND app_id=? AND actor=?"
    };
    sqlx::query(q)
        .bind(&c.plane)
        .bind(id)
        .bind(&identity.principal_id)
        .execute(&s.db)
        .await?;
    Ok(Json(json!({"starred":enabled})))
}
#[derive(Deserialize)]
struct Message {
    message: String,
    #[serde(default)]
    provider: Option<String>,
}
async fn request_publication(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Message>,
) -> Result<(StatusCode, Json<Value>)> {
    key(&h)?;
    let c = context(&s, &h).await?;
    let app = find_app(&s, &c, &id, true).await?;
    if revision(&h)? != app.revision {
        return Err(Error::conflict("Application revision changed"));
    }
    if body.message.trim().is_empty() || body.message.len() > 10000 {
        return Err(Error::bad(
            "A publication justification of 1–10000 characters is required",
        ));
    }
    if app.latest_version.is_none() || app.iam_revision == 0 {
        return Err(Error::conflict(
            "Upload a valid CLI release and wait for IAM private activation before requesting publication",
        ));
    }
    let existing = sqlx::query(
        "SELECT id,state FROM publication_requests WHERE plane=? AND app_id=? AND revision=?",
    )
    .bind(&c.plane)
    .bind(&id)
    .bind(app.revision)
    .fetch_optional(&s.db)
    .await?;
    if let Some(r) = existing {
        return Ok((
            StatusCode::OK,
            Json(json!({"id":r.get::<String,_>("id"),"state":r.get::<String,_>("state")})),
        ));
    }
    let request = uuid::Uuid::new_v4().to_string();
    let mut tx = s.db.begin().await?;
    sqlx::query("INSERT INTO publication_requests(id,plane,app_id,revision,state,requested_by,created_at) VALUES(?,?,?,?,'awaiting_scope_review',?,?)").bind(&request).bind(&c.plane).bind(&id).bind(app.revision).bind(&c.identity()?.principal_id).bind(now()).execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO discussions(id,request_id,actor,message,created_at) VALUES(?,?,?,?,?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&request)
    .bind(&c.identity()?.principal_id)
    .bind(body.message)
    .bind(now())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"id":request,"state":"awaiting_scope_review","visibility":"private"})),
    ))
}
async fn publication(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    find_app(&s, &c, &id, true).await?;
    let requests = sqlx::query(
        "SELECT * FROM publication_requests WHERE plane=? AND app_id=? ORDER BY revision DESC",
    )
    .bind(&c.plane)
    .bind(id)
    .fetch_all(&s.db)
    .await?;
    let mut items = vec![];
    for r in requests {
        let request_id: String = r.get("id");
        let rows =
            sqlx::query("SELECT * FROM discussions WHERE request_id=? ORDER BY created_at,id")
                .bind(&request_id)
                .fetch_all(&s.db)
                .await?;
        let messages:Vec<Value>=rows.iter().map(|r|json!({"id":r.get::<String,_>("id"),"actor":r.get::<String,_>("actor"),"message":r.get::<String,_>("message"),"provider":r.get::<Option<String>,_>("provider")})).collect();
        items.push(json!({"id":request_id,"revision":r.get::<i64,_>("revision"),"state":r.get::<String,_>("state"),"messages":messages}));
    }
    Ok(Json(json!({"items":items})))
}
async fn message(
    ExtractState(s): ExtractState<State>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Message>,
) -> Result<Json<Value>> {
    key(&h)?;
    let c = context(&s, &h).await?;
    find_app(&s, &c, &id, true).await?;
    if body.message.trim().is_empty() || body.message.len() > 10000 {
        return Err(Error::bad("message must contain 1–10000 characters"));
    }
    let request:Option<String>=sqlx::query_scalar("SELECT id FROM publication_requests WHERE plane=? AND app_id=? ORDER BY revision DESC LIMIT 1").bind(&c.plane).bind(&id).fetch_optional(&s.db).await?;
    let request = request.ok_or_else(Error::missing)?;
    let message_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO discussions(id,request_id,actor,provider,message,created_at) VALUES(?,?,?,?,?,?)").bind(&message_id).bind(request).bind(&c.identity()?.principal_id).bind(body.provider).bind(body.message).bind(now()).execute(&s.db).await?;
    Ok(Json(json!({"id":message_id})))
}
