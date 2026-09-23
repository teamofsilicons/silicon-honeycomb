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
            validator: false,
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
struct Manager;
#[async_trait]
impl Management for Manager {
    async fn configure(&self, _: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        unreachable!()
    }
}
#[derive(Default)]
struct Storage(Mutex<BTreeMap<String, Vec<u8>>>);
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
        Ok(reference.into())
    }
}
async fn setup() -> (State, Client, tokio::task::JoinHandle<()>) {
    let s = State {
        telemetry: Default::default(),
        db: silicon_honeycomb_server::database("sqlite::memory:")
            .await
            .unwrap(),
        identity: Arc::new(IdentityService),
        management: Arc::new(Manager),
        storage: Arc::new(Storage::default()),
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
    (s, client, handle)
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
