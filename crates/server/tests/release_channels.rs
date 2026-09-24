use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use honeycomb_client::{Client, Mutation, ReleaseChannel};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
use tower::ServiceExt;

struct IdentityService;
#[async_trait]
impl IdentityProvider for IdentityService {
    async fn authenticate(&self, token: &str, environment: Option<&str>) -> Result<Identity> {
        if !["admin", "member", "outsider"].contains(&token) {
            return Err(Error::unauthorized());
        }
        Ok(Identity {
            principal_id: token.into(),
            actor_type: Some("carbon".into()),
            organizations: BTreeMap::from([(
                if token == "outsider" { "other" } else { "tos" }.into(),
                Some(
                    if token == "admin" {
                        "org_admin"
                    } else {
                        "org_member"
                    }
                    .into(),
                ),
            )]),
            testing_environment_id: environment.map(|_| "release-test".into()),
            validator: token == "admin",
        })
    }
    async fn login(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn refresh(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn revoke(&self, _: &str, _: &str, _: Option<&str>) -> Result<()> {
        unreachable!()
    }
}
#[derive(Default)]
struct Manager(Mutex<Option<Value>>);
#[async_trait]
impl Management for Manager {
    async fn configure(&self, _: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        unreachable!()
    }
    async fn publication_plan(&self, r: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        Ok(
            json!({"state":"accepted","request_id":r["request_id"],"app_id":r["app_id"],"configuration_revision":r["configuration_revision"],"plan_id":format!("plan-{}",r["request_id"].as_str().unwrap()),"gates":[{"provider":"iam","scopes":["directory.carbons.read"]},{"provider":"honeycomb","scopes":[]}]}),
        )
    }
    async fn review_eligibility(
        &self,
        _: &str,
        _: &str,
        token: &str,
        _: Option<&str>,
    ) -> Result<bool> {
        Ok(token == "admin")
    }
    async fn review_decision(&self, o: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        let mut result = o.clone();
        result["state"] = json!("accepted");
        Ok(result)
    }
    async fn activate_publication(&self, o: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        let response = json!({"state":"accepted","operation_id":o["operation_id"],"request_id":o["request_id"],"publication_request_id":o["request_id"],"app_id":o["app_id"],"configuration_revision":o["configuration_revision"],"iam_revision":o["expected_iam_revision"].as_i64().unwrap()+1,"visibility":"public","effective_configuration":o["configuration"]});
        *self.0.lock().unwrap() = Some(response.clone());
        Ok(response)
    }
    async fn application_state(&self, _: &str, _: &str, _: Option<&str>) -> Result<Value> {
        Ok(self.0.lock().unwrap().clone().unwrap())
    }
}
#[derive(Default)]
struct Storage(
    Mutex<BTreeMap<String, Vec<u8>>>,
    Mutex<Vec<String>>,
    std::sync::atomic::AtomicBool,
);
#[async_trait]
impl ArchiveStorage for Storage {
    async fn put(
        &self,
        _org: &str,
        id: &str,
        version: &str,
        path: &std::path::Path,
        _: &str,
        env: Option<&str>,
        _: &str,
    ) -> Result<String> {
        let key = format!("{}:{id}:{version}", env.unwrap_or("production"));
        self.0
            .lock()
            .unwrap()
            .insert(key.clone(), std::fs::read(path).unwrap());
        Ok(key)
    }
    async fn read(
        &self,
        reference: &str,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Vec<u8>> {
        self.0
            .lock()
            .unwrap()
            .get(reference)
            .cloned()
            .ok_or_else(Error::missing)
    }
    async fn publish(
        &self,
        reference: &str,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: &str,
    ) -> Result<String> {
        if self.2.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::unavailable("Publication storage unavailable"));
        }
        self.1.lock().unwrap().push(reference.into());
        Ok(reference.into())
    }
}
async fn setup() -> (State, Client, tokio::task::JoinHandle<()>) {
    let (s, client, server, _) = setup_with_storage().await;
    (s, client, server)
}
async fn setup_with_storage() -> (State, Client, tokio::task::JoinHandle<()>, Arc<Storage>) {
    let storage = Arc::new(Storage::default());
    let s = State {
        telemetry: Default::default(),
        db: silicon_honeycomb_server::database("sqlite::memory:")
            .await
            .unwrap(),
        identity: Arc::new(IdentityService),
        management: Arc::new(Manager::default()),
        storage: storage.clone(),
        app_id: "honeycomb".into(),
        iam_app_id: "iam".into(),
        iam_login_url: "https://iam.example.com".into(),
        encryption_key: [7; 32],
        webhook_secret: "test-secret".into(),
    };
    for plane in ["production", "release-test"] {
        sqlx::query("INSERT INTO applications(plane,app_id,org_id,name,description,visibility,state,revision,iam_revision,effective_revision,config,effective_config,webhook_secret,created_at,updated_at) VALUES(?,'tracks','tos','Tracks','Track fixture','public','active',1,1,1,'{}','{}','',1,1)")
            .bind(plane).execute(&s.db).await.unwrap();
    }
    let root = "ReleaseRootKey000000000000000000";
    sqlx::query("INSERT INTO environments(id,org_id,creator,name,description,encrypted_key,key_hash,state,created_at,last_activity) VALUES('release-test','tos','admin','Releases','',?,?,'ready',1,1)")
        .bind(s.encrypt(root).unwrap()).bind(hex::encode(Sha256::digest(root))).execute(&s.db).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = Client::new(&format!("http://{}", listener.local_addr().unwrap()))
        .unwrap()
        .with_token("admin")
        .with_telemetry(false);
    let router = silicon_honeycomb_server::api::router(s.clone());
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (s, client, handle, storage)
}
fn archive(version: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let mut targets = serde_json::Map::new();
    for target in honeycomb_core::package::REQUIRED_TARGETS {
        let root = format!("targets/{target}");
        std::fs::create_dir_all(d.path().join(&root)).unwrap();
        let path = d.path().join(&root).join("tracks");
        std::fs::write(&path, b"#!/bin/sh\necho payload\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        targets.insert(
            target.into(),
            json!({"root":root,"executables":{"app":"tracks"}}),
        );
    }
    std::fs::write(d.path().join("honeycomb.yaml"), json!({"format_version":1,"app_id":"tracks","version":version,"bin":{"tracks":"app"},"targets":targets}).to_string()).unwrap();
    let path = honeycomb_core::package::pack(d.path(), None).unwrap();
    (d, path)
}

#[tokio::test]
async fn tracks_have_independent_histories_and_promotion_creates_a_valid_new_archive() {
    let (s, client, server) = setup().await;
    let (_old, old) = archive("1.0.0");
    let (_new, new) = archive("9.0.0");
    client
        .upload_release(
            "tracks",
            &new,
            ReleaseChannel::Dev,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap();
    assert!(client.releases("tracks").await.unwrap().is_empty());
    assert_eq!(client.app("tracks").await.unwrap().latest_version, None);
    client
        .upload_release(
            "tracks",
            &old,
            ReleaseChannel::Prod,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap();
    client
        .upload_release(
            "tracks",
            &old,
            ReleaseChannel::Dev,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap();
    let prod = client.releases("tracks").await.unwrap();
    let dev = client
        .releases_channel("tracks", ReleaseChannel::Dev)
        .await
        .unwrap();
    assert_eq!(prod.len(), 1);
    assert_eq!(prod[0].version, "1.0.0");
    assert_eq!(
        dev.iter().map(|r| r.version.as_str()).collect::<Vec<_>>(),
        ["9.0.0", "1.0.0"]
    );
    assert_eq!(
        client
            .app("tracks")
            .await
            .unwrap()
            .latest_version
            .as_deref(),
        Some("1.0.0")
    );
    assert!(
        client
            .upload_release(
                "tracks",
                &old,
                ReleaseChannel::Dev,
                &Mutation::at_revision(1)
            )
            .await
            .is_err()
    );
    assert!(
        client
            .clone()
            .with_token("member")
            .promote_release("tracks", "9.0.0", "2.0.0", &Mutation::at_revision(1))
            .await
            .is_err()
    );
    assert!(
        client
            .promote_release("tracks", "9.0.0", "2.0.0", &Mutation::at_revision(99))
            .await
            .is_err()
    );
    let mutation = Mutation::at_revision(1);
    let promoted = client
        .promote_release("tracks", "9.0.0", "2.0.0", &mutation)
        .await
        .unwrap();
    assert_eq!(promoted["channel"], "prod");
    assert_eq!(promoted["promoted_from"], "9.0.0");
    let replay = client
        .promote_release("tracks", "9.0.0", "2.0.0", &mutation)
        .await
        .unwrap();
    assert_eq!(replay["id"], promoted["id"]);
    assert!(
        client
            .promote_release("tracks", "9.0.0", "2.0.0", &Mutation::at_revision(1))
            .await
            .is_err()
    );
    assert!(
        client
            .promote_release("tracks", "2.0.0", "3.0.0", &Mutation::at_revision(1))
            .await
            .is_err()
    );
    let prod = client.releases("tracks").await.unwrap();
    assert_eq!(prod[0].version, "2.0.0");
    let download = tempfile::tempdir().unwrap();
    client
        .download("tracks", &prod[0], &download.path().join("prod.tar.gz"))
        .await
        .unwrap();
    client
        .download("tracks", &dev[0], &download.path().join("dev.tar.gz"))
        .await
        .unwrap();
    assert_eq!(
        honeycomb_core::package::validate(&download.path().join("prod.tar.gz"))
            .manifest
            .unwrap()
            .version,
        "2.0.0"
    );
    assert_eq!(
        honeycomb_core::package::validate(&download.path().join("dev.tar.gz"))
            .manifest
            .unwrap()
            .version,
        "9.0.0"
    );
    assert_eq!(
        client
            .app("tracks")
            .await
            .unwrap()
            .latest_version
            .as_deref(),
        Some("2.0.0")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM downloads")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        1
    );
    // Test-environment release tracks remain independent of the production environment.
    let test_client = client
        .clone()
        .with_environment("ReleaseRootKey000000000000000000")
        .unwrap();
    assert!(test_client.releases("tracks").await.unwrap().is_empty());
    assert!(
        test_client
            .releases_channel("tracks", ReleaseChannel::Dev)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        test_client
            .promote_release("tracks", "9.0.0", "4.0.0", &Mutation::at_revision(1))
            .await
            .is_err()
    );
    test_client
        .upload_release(
            "tracks",
            &new,
            ReleaseChannel::Dev,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap();
    test_client
        .promote_release("tracks", "9.0.0", "4.0.0", &Mutation::at_revision(1))
        .await
        .unwrap();
    assert_eq!(
        test_client.releases("tracks").await.unwrap()[0].version,
        "4.0.0"
    );
    assert_eq!(client.releases("tracks").await.unwrap()[0].version, "2.0.0");
    assert!(
        client
            .with_environment("InvalidRootKey000000000000000000")
            .unwrap()
            .releases("tracks")
            .await
            .is_err()
    );
    server.abort();
}

#[tokio::test]
async fn uploads_require_an_explicit_track_and_download_defaults_to_production() {
    let (s, client, server) = setup().await;
    let (_dir, path) = archive("1.0.0");
    let router = silicon_honeycomb_server::api::router(s);
    for query in ["", "?channel=unknown"] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v2/apps/tracks/releases{query}"))
                    .header("authorization", "Bearer admin")
                    .header("if-match", "1")
                    .header("idempotency-key", "missing-channel-test")
                    .body(Body::from(std::fs::read(&path).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    client
        .upload_release(
            "tracks",
            &path,
            ReleaseChannel::Prod,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap();
    let (_dev, path) = archive("8.0.0");
    client
        .upload_release(
            "tracks",
            &path,
            ReleaseChannel::Dev,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap();
    for (query, version) in [("", "1.0.0"), ("?channel=dev", "8.0.0")] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v2/apps/tracks/download{query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let directory = tempfile::tempdir().unwrap();
        let download = directory.path().join("download.tar.gz");
        std::fs::write(&download, bytes).unwrap();
        assert_eq!(
            honeycomb_core::package::validate(&download)
                .manifest
                .unwrap()
                .version,
            version
        );
    }
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v2/apps/tracks/download?version=8.0.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    server.abort();
}

#[tokio::test]
async fn migration_preserves_existing_release_and_publication_records_as_production() {
    use sqlx::Row;
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0001_initial.sql"))
        .execute(&db)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/0008_publication_activation.sql"
    ))
    .execute(&db)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0020_release_validation.sql"))
        .execute(&db)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO applications(plane,app_id,org_id,name,description,config,webhook_secret,created_at,updated_at) VALUES('production','old','tos','Old','','{}','',1,1); INSERT INTO releases VALUES('production','old','1.0.0','checksum',42,'storage',1); INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES('op','production','admin','migration-key','release','old','hash',1,1); INSERT INTO publication_archives VALUES('op','1.0.0','storage',NULL,'pending',NULL); INSERT INTO release_validation_failures VALUES('production','old',1,'[]',1);").execute(&db).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0024_release_channels.sql"))
        .execute(&db)
        .await
        .unwrap();
    for query in [
        "SELECT * FROM releases",
        "SELECT * FROM publication_archives",
        "SELECT * FROM release_validation_failures",
    ] {
        let row = sqlx::query(query).fetch_one(&db).await.unwrap();
        assert_eq!(row.get::<String, _>("channel"), "prod");
    }
    sqlx::query("INSERT INTO releases VALUES('production','old','dev','1.0.0','dev-checksum',43,'dev-storage',2)").execute(&db).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM releases")
            .fetch_one(&db)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn a_pending_upload_reserves_only_its_own_channel_version() {
    let (s, client, server) = setup().await;
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES('in-flight','production','admin','in-flight-upload-key','release','tracks','hash',1,1)").execute(&s.db).await.unwrap();
    sqlx::query(
        "INSERT INTO release_reservations VALUES('production','tracks','prod','1.0.0','in-flight')",
    )
    .execute(&s.db)
    .await
    .unwrap();
    let (_directory, path) = archive("1.0.0");
    let error = client
        .upload_release(
            "tracks",
            &path,
            ReleaseChannel::Prod,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("already being released"));
    client
        .upload_release(
            "tracks",
            &path,
            ReleaseChannel::Dev,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap();
    assert!(client.releases("tracks").await.unwrap().is_empty());
    assert_eq!(
        client
            .releases_channel("tracks", ReleaseChannel::Dev)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operations WHERE kind='release'")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        2
    );
    server.abort();
}

#[tokio::test]
async fn legacy_upload_keys_replay_only_the_same_production_archive() {
    let (s, client, server) = setup().await;
    let (_directory, path) = archive("1.0.0");
    let sha = honeycomb_core::package::sha256(&path).unwrap();
    let old_hash = hex::encode(Sha256::digest(
        json!({"kind":"release","app_id":"tracks","version":"1.0.0","sha256":sha}).to_string(),
    ));
    let mutation = Mutation {
        idempotency_key: "legacy-upload-idempotency".into(),
        revision: Some(1),
    };
    sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES('legacy-upload','production','admin',?,'release','tracks',?,1,1)").bind(&mutation.idempotency_key).bind(&old_hash).execute(&s.db).await.unwrap();
    let uploaded = client
        .upload_release("tracks", &path, ReleaseChannel::Prod, &mutation)
        .await
        .unwrap();
    assert_eq!(uploaded["id"], "legacy-upload");
    assert_eq!(uploaded["channel"], "prod");
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT storage_ref FROM releases WHERE plane='production' AND app_id='tracks' AND channel='prod'").fetch_one(&s.db).await.unwrap(), "production:tracks:1.0.0");
    let replay = client
        .upload_release("tracks", &path, ReleaseChannel::Prod, &mutation)
        .await
        .unwrap();
    assert_eq!(replay["id"], "legacy-upload");
    assert_eq!(replay["sha256"], sha);
    assert!(
        client
            .upload_release("tracks", &path, ReleaseChannel::Dev, &mutation)
            .await
            .is_err()
    );
    let (_changed, changed) = archive("2.0.0");
    assert!(
        client
            .upload_release("tracks", &changed, ReleaseChannel::Prod, &mutation)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM releases")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        1
    );
    server.abort();
}

#[tokio::test]
async fn v1_upload_and_archive_semver_contract_survive_v2_channel_introduction() {
    let (s, client, server) = setup().await;
    let (_dir, path) = archive("1.2.3-preview.1+original");
    let bytes = std::fs::read(&path).unwrap();
    let router = silicon_honeycomb_server::api::router(s.clone());
    let request = |route: &str, key: &str| {
        Request::builder()
            .method("POST")
            .uri(route)
            .header("authorization", "Bearer admin")
            .header("if-match", "1")
            .header("idempotency-key", key)
            .header("content-type", "application/gzip")
            .body(Body::from(bytes.clone()))
            .unwrap()
    };
    // A pre-channel caller supplies neither channel nor negotiation headers.
    for expected in [StatusCode::CREATED, StatusCode::OK] {
        let response = router
            .clone()
            .oneshot(request(
                "/api/v1/apps/tracks/releases",
                "legacy-raw-upload-key",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(response.headers()["honeycomb-api-version"], "v1");
        let result: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(result["version"], "1.2.3-preview.1+original");
        assert_eq!(result["channel"], "prod");
    }
    let sha = honeycomb_core::package::sha256(&path).unwrap();
    let expected_hash = hex::encode(Sha256::digest(json!({"kind":"release","app_id":"tracks","version":"1.2.3-preview.1+original","sha256":sha}).to_string()));
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT request_hash FROM operations WHERE idempotency_key='legacy-raw-upload-key'"
        )
        .fetch_one(&s.db)
        .await
        .unwrap(),
        expected_hash
    );
    let response = router
        .clone()
        .oneshot(request(
            "/api/v2/apps/tracks/releases?channel=dev",
            "v2-strict-upload-key",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let result: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(result["error"]["details"].to_string().contains("x.y.z"));
    // V2 reads keep the original immutable metadata and archives available.
    let release = client.releases("tracks").await.unwrap().remove(0);
    assert_eq!(release.version, "1.2.3-preview.1+original");
    let downloaded = tempfile::tempdir().unwrap();
    client
        .download(
            "tracks",
            &release,
            &downloaded.path().join("historical.tar.gz"),
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(downloaded.path().join("historical.tar.gz")).unwrap(),
        bytes
    );
    let (_dev, dev) = archive("9.0.0");
    client
        .upload_release(
            "tracks",
            &dev,
            ReleaseChannel::Dev,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap();
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/apps/tracks/releases?channel=dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let result: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert_eq!(result["items"][0]["channel"], "prod");
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/v1/apps/tracks/download?channel=dev")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .as_ref(),
        bytes
    );
    server.abort();
}

async fn request_bytes(s: &State, path: &str, token: Option<&str>) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().uri(path);
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    (
        response.status(),
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
}
async fn request_scope_expansion(s: &State, revision: i64) {
    let config = json!({"app_id":"tracks","org_id":"tos","visibility":"public","app_scope":{"iam":["directory.carbons.read"],"external":[]}});
    sqlx::query(
        "UPDATE applications SET revision=?,config=? WHERE plane='production' AND app_id='tracks'",
    )
    .bind(revision)
    .bind(config.to_string())
    .execute(&s.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn public_updates_wait_for_permission_approval_across_lists_downloads_and_channels() {
    let (s, client, server, storage) = setup_with_storage().await;
    let (_old, old) = archive("1.0.0");
    for channel in [ReleaseChannel::Prod, ReleaseChannel::Dev] {
        client
            .upload_release("tracks", &old, channel, &Mutation::at_revision(1))
            .await
            .unwrap();
    }
    request_scope_expansion(&s, 2).await;
    let (_new, new) = archive("2.0.0");
    let mutation = Mutation::at_revision(2);
    let receipt = client
        .upload_release("tracks", &new, ReleaseChannel::Prod, &mutation)
        .await
        .unwrap();
    assert_eq!(receipt["release"]["visibility"], "private");
    assert_eq!(receipt["release"]["permission_approval_required"], true);
    assert_eq!(receipt["release"]["configuration_revision"], 2);
    assert_eq!(receipt["publication"]["state"], "awaiting_scope_review");
    let id = receipt["publication"]["id"].as_str().unwrap();
    let replay = client
        .upload_release("tracks", &new, ReleaseChannel::Prod, &mutation)
        .await
        .unwrap();
    assert_eq!(receipt["release"], replay["release"]);
    client
        .upload_release(
            "tracks",
            &new,
            ReleaseChannel::Dev,
            &Mutation::at_revision(2),
        )
        .await
        .unwrap();
    let promoted = client
        .promote_release("tracks", "2.0.0", "3.0.0", &Mutation::at_revision(2))
        .await
        .unwrap();
    assert_eq!(promoted["release"]["visibility"], "private");
    assert_eq!(
        storage.1.lock().unwrap().len(),
        2,
        "Only the previously public archives may be shared"
    );
    let history = client
        .release_history("tracks", ReleaseChannel::Prod)
        .await
        .unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(
        history[0].approval_status.as_deref(),
        Some("awaiting_scope_review")
    );
    assert!(history[0].permission_approval_required);
    for token in [None, Some("outsider"), Some("member"), Some("admin")] {
        for channel in ["prod", "dev"] {
            let (status, body) = request_bytes(
                &s,
                &format!("/api/v2/apps/tracks/releases?channel={channel}"),
                token,
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            let list: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(list["items"].as_array().unwrap().len(), 1);
            assert_eq!(list["items"][0]["version"], "1.0.0");
            let (status, body) = request_bytes(
                &s,
                &format!("/api/v2/apps/tracks/download?channel={channel}"),
                token,
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body, std::fs::read(&old).unwrap());
            assert_eq!(
                request_bytes(
                    &s,
                    &format!("/api/v2/apps/tracks/download?channel={channel}&version=2.0.0"),
                    token
                )
                .await
                .0,
                StatusCode::NOT_FOUND
            );
        }
        let (status, body) = request_bytes(&s, "/api/v1/apps/tracks/releases", token).await;
        assert_eq!(status, StatusCode::OK);
        let list: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(list["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            request_bytes(&s, "/api/v1/apps/tracks/download?version=2.0.0", token)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        let (_, body) = request_bytes(&s, "/api/v1/apps/tracks", token).await;
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["latest_version"],
            "1.0.0"
        );
    }
    assert!(
        client
            .clone()
            .with_token("member")
            .release_history("tracks", ReleaseChannel::Prod)
            .await
            .is_err()
    );
    // A notification's temporary private fence must not let members update early.
    sqlx::query(
        "UPDATE applications SET visibility='private' WHERE plane='production' AND app_id='tracks'",
    )
    .execute(&s.db)
    .await
    .unwrap();
    assert_eq!(client.releases("tracks").await.unwrap()[0].version, "1.0.0");
    assert_eq!(
        request_bytes(
            &s,
            "/api/v2/apps/tracks/download?version=2.0.0",
            Some("admin")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    sqlx::query(
        "UPDATE applications SET visibility='public' WHERE plane='production' AND app_id='tracks'",
    )
    .execute(&s.db)
    .await
    .unwrap();
    // Scope-provider approval alone is not publication approval.
    client
        .decide_review(id, "iam", "approve", "", &Mutation::at_revision(2))
        .await
        .unwrap();
    assert_eq!(client.releases("tracks").await.unwrap()[0].version, "1.0.0");
    storage.2.store(true, std::sync::atomic::Ordering::SeqCst);
    client
        .decide_review(id, "honeycomb", "approve", "", &Mutation::at_revision(2))
        .await
        .unwrap();
    assert_eq!(
        client.releases("tracks").await.unwrap()[0].version,
        "1.0.0",
        "Accepted permissions alone cannot bypass failed archive publication"
    );
    storage.2.store(false, std::sync::atomic::Ordering::SeqCst);
    sqlx::query("UPDATE publication_authorizations SET next_attempt_at=0 WHERE request_id=?")
        .bind(id)
        .execute(&s.db)
        .await
        .unwrap();
    silicon_honeycomb_server::publication_worker::tick(&s)
        .await
        .unwrap();
    assert_eq!(
        storage.1.lock().unwrap().len(),
        5,
        "Only the three new archives are shared after approval"
    );
    assert_eq!(client.releases("tracks").await.unwrap()[0].version, "3.0.0");
    assert_eq!(
        client
            .releases_channel("tracks", ReleaseChannel::Dev)
            .await
            .unwrap()[0]
            .version,
        "2.0.0"
    );
    let (_, body) = request_bytes(&s, "/api/v2/apps/tracks/download?version=2.0.0", None).await;
    assert_eq!(body, std::fs::read(new).unwrap());
    assert_eq!(client.app("tracks").await.unwrap().visibility, "public");
    let history = client
        .release_history("tracks", ReleaseChannel::Prod)
        .await
        .unwrap();
    assert!(
        history
            .iter()
            .all(|r| r.visibility == "public" && !r.permission_approval_required)
    );
    // Later releases on the approved configuration need no additional review.
    let (_later, later) = archive("4.0.0");
    let receipt = client
        .upload_release(
            "tracks",
            &later,
            ReleaseChannel::Prod,
            &Mutation::at_revision(2),
        )
        .await
        .unwrap();
    assert_eq!(receipt["release"]["visibility"], "public");
    server.abort();
}

#[tokio::test]
async fn denied_and_superseded_releases_never_publish_with_a_later_approval() {
    let (s, client, server) = setup().await;
    let (_old, old) = archive("1.0.0");
    client
        .upload_release(
            "tracks",
            &old,
            ReleaseChannel::Prod,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap();
    request_scope_expansion(&s, 2).await;
    let (_new, new) = archive("2.0.0");
    let receipt = client
        .upload_release(
            "tracks",
            &new,
            ReleaseChannel::Prod,
            &Mutation::at_revision(2),
        )
        .await
        .unwrap();
    let id = receipt["publication"]["id"].as_str().unwrap();
    client
        .decide_review(
            id,
            "iam",
            "deny",
            "Narrow the requested access.",
            &Mutation::at_revision(2),
        )
        .await
        .unwrap();
    assert_eq!(
        client
            .release_history("tracks", ReleaseChannel::Prod)
            .await
            .unwrap()[0]
            .approval_status
            .as_deref(),
        Some("denied")
    );
    assert_eq!(client.releases("tracks").await.unwrap()[0].version, "1.0.0");
    request_scope_expansion(&s, 3).await;
    let (_later, later) = archive("3.0.0");
    let receipt = client
        .upload_release(
            "tracks",
            &later,
            ReleaseChannel::Prod,
            &Mutation::at_revision(3),
        )
        .await
        .unwrap();
    let id = receipt["publication"]["id"].as_str().unwrap();
    for provider in ["iam", "honeycomb"] {
        client
            .decide_review(id, provider, "approve", "", &Mutation::at_revision(3))
            .await
            .unwrap();
    }
    let list = client.releases("tracks").await.unwrap();
    assert_eq!(
        list.iter().map(|r| r.version.as_str()).collect::<Vec<_>>(),
        ["3.0.0", "1.0.0"]
    );
    let history = client
        .release_history("tracks", ReleaseChannel::Prod)
        .await
        .unwrap();
    assert_eq!(history[1].visibility, "private");
    assert_eq!(history[1].approval_status.as_deref(), Some("superseded"));
    server.abort();
}

#[tokio::test]
async fn visibility_migration_preserves_existing_distribution_and_new_rows_default_private() {
    use sqlx::Row;
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE applications(plane TEXT,app_id TEXT,visibility TEXT,revision INTEGER,effective_revision INTEGER); CREATE TABLE releases(plane TEXT,app_id TEXT,channel TEXT,version TEXT); INSERT INTO applications VALUES('production','live','public',3,2),('production','internal','private',4,4); INSERT INTO releases VALUES('production','live','prod','1.0.0'),('production','live','dev','8.0.0'),('production','internal','prod','2.0.0');").execute(&db).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0027_release_visibility.sql"))
        .execute(&db)
        .await
        .unwrap();
    let rows =
        sqlx::query("SELECT visibility,configuration_revision FROM releases WHERE app_id='live'")
            .fetch_all(&db)
            .await
            .unwrap();
    assert!(
        rows.iter()
            .all(|r| r.get::<String, _>("visibility") == "public"
                && r.get::<i64, _>("configuration_revision") == 2)
    );
    let row = sqlx::query(
        "SELECT visibility,configuration_revision FROM releases WHERE app_id='internal'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("visibility"), "private");
    assert_eq!(row.get::<i64, _>("configuration_revision"), 4);
    sqlx::query("INSERT INTO releases(plane,app_id,channel,version) VALUES('production','live','prod','2.0.0')").execute(&db).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT visibility FROM releases WHERE app_id='live' AND version='2.0.0'"
        )
        .fetch_one(&db)
        .await
        .unwrap(),
        "private"
    );
}

#[tokio::test]
async fn private_apps_retain_organization_only_release_distribution() {
    let (s, client, server, storage) = setup_with_storage().await;
    sqlx::query("UPDATE applications SET visibility='private' WHERE app_id='tracks'")
        .execute(&s.db)
        .await
        .unwrap();
    let (_dir, path) = archive("1.0.0");
    let receipt = client
        .upload_release(
            "tracks",
            &path,
            ReleaseChannel::Prod,
            &Mutation::at_revision(1),
        )
        .await
        .unwrap();
    assert_eq!(receipt["release"]["visibility"], "private");
    assert!(storage.1.lock().unwrap().is_empty());
    let member = client.clone().with_token("member");
    assert_eq!(
        member.releases("tracks").await.unwrap()[0].visibility,
        "private"
    );
    let (status, body) = request_bytes(&s, "/api/v2/apps/tracks/download", Some("member")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, std::fs::read(path).unwrap());
    for token in [None, Some("outsider")] {
        assert_eq!(
            request_bytes(&s, "/api/v2/apps/tracks/releases", token)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request_bytes(&s, "/api/v2/apps/tracks/download", token)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }
    server.abort();
}
