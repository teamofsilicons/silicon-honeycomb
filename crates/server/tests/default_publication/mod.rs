use super::*;

fn archive() -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let mut targets = serde_json::Map::new();
    for target in honeycomb_core::package::REQUIRED_TARGETS {
        let root = format!("targets/{target}");
        std::fs::create_dir_all(dir.path().join(&root)).unwrap();
        std::fs::write(dir.path().join(&root).join("app"), b"fixture executable").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                dir.path().join(&root).join("app"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        targets.insert(
            target.to_string(),
            json!({"root":root,"executables":{"app":"app"}}),
        );
    }
    std::fs::write(
        dir.path().join("honeycomb.yaml"),
        json!({"format_version":1,"version":"1.0.0","bin":{"fixture":"app"},"targets":targets})
            .to_string(),
    )
    .unwrap();
    let path = dir.path().join("fixture.tar.gz");
    honeycomb_core::package::pack(dir.path(), Some(&path)).unwrap();
    std::fs::read(path).unwrap()
}
async fn upload(s: &State, id: &str, bytes: &[u8], actor: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/apps/{id}/releases?channel=prod"))
        .header("authorization", format!("Bearer {actor}"))
        .header("if-match", "1")
        .header("idempotency-key", "default-publication-upload")
        .header("content-type", "application/gzip")
        .body(Body::from(bytes.to_vec()))
        .unwrap();
    let response = silicon_honeycomb_server::api::router(s.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let body =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (status, body)
}
async fn count(s: &State) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM publication_requests")
        .fetch_one(&s.db)
        .await
        .unwrap()
}
#[tokio::test]
async fn default_requests_review_on_valid_upload_and_replays_without_duplicates() {
    let (s, _) = setup(true).await;
    let (status, created) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("auto"),
        "default-publication-create",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{created}");
    assert_eq!(created["publication"]["state"], "awaiting_release");
    assert_eq!(count(&s).await, 0);
    let bytes = archive();
    let (status, _) = upload(&s, "auto", &bytes, "member").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(count(&s).await, 0);
    let (status, rejected) = upload(&s, "auto", b"invalid archive", "admin").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    assert_eq!(count(&s).await, 0);
    let (status, released) = upload(&s, "auto", &bytes, "admin").await;
    assert_eq!(status, StatusCode::CREATED, "{released}");
    assert_eq!(released["publication"]["state"], "awaiting_review_plan");
    assert_eq!(count(&s).await, 1);
    let (status, replay) = upload(&s, "auto", &bytes, "admin").await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["publication"]["id"], released["publication"]["id"]);
    assert_eq!(count(&s).await, 1);
    let (_, app) = call(
        &s,
        "GET",
        "/api/v1/apps/auto",
        Some("admin"),
        Value::Null,
        "",
        None,
    )
    .await;
    assert_eq!(app["config"]["visibility"], "public");
    assert_eq!(app["visibility"], "private");
    let authorizations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM publication_authorizations")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(authorizations, 1);
}
#[tokio::test]
async fn explicit_private_and_legacy_choices_survive_updates_and_can_opt_in() {
    for legacy in [false, true] {
        let (s, _) = setup(true).await;
        let mut config = input("private");
        config["visibility"] = json!("private");
        call(
            &s,
            "POST",
            "/api/v1/apps",
            Some("admin"),
            config,
            "private-create-0001",
            None,
        )
        .await;
        if legacy {
            sqlx::query("UPDATE applications SET config=json_remove(config,'$.visibility')")
                .execute(&s.db)
                .await
                .unwrap();
        }
        let (_, result) = upload(&s, "private", &archive(), "admin").await;
        assert_eq!(result["publication"]["state"], "manual", "{result}");
        assert_eq!(count(&s).await, 0);
        let (status, updated) = call(
            &s,
            "PUT",
            "/api/v1/apps/private",
            Some("admin"),
            input("private"),
            "private-update-0001",
            Some(1),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED, "{updated}");
        assert_eq!(updated["publication"]["state"], "manual");
        assert_eq!(count(&s).await, 0);
        let mut config = input("private");
        config["visibility"] = json!("public");
        let (_, updated) = call(
            &s,
            "PUT",
            "/api/v1/apps/private",
            Some("admin"),
            config,
            "public-opt-in-0001",
            Some(2),
        )
        .await;
        assert_eq!(
            updated["publication"]["state"], "awaiting_review_plan",
            "{updated}"
        );
        assert_eq!(count(&s).await, 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT revision FROM publication_requests")
                .fetch_one(&s.db)
                .await
                .unwrap(),
            3
        );
    }
}
#[tokio::test]
async fn pending_configuration_waits_then_retries_publication_after_acceptance() {
    let (mut s, _) = setup(false).await;
    let (_, created) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("pending"),
        "pending-create-0001",
        None,
    )
    .await;
    let (_, uploaded) = upload(&s, "pending", &archive(), "admin").await;
    assert_eq!(
        uploaded["publication"]["state"], "awaiting_configuration",
        "{uploaded}"
    );
    assert_eq!(count(&s).await, 0);
    s.management = Arc::new(Manager { accept: true });
    let path = format!(
        "/api/v1/operations/{}/retry",
        created["id"].as_str().unwrap()
    );
    let (status, retried) = call(
        &s,
        "POST",
        &path,
        Some("admin"),
        json!({}),
        "pending-retry-0001",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{retried}");
    assert_eq!(retried["publication"]["state"], "awaiting_review_plan");
    assert_eq!(count(&s).await, 1);
    call(
        &s,
        "POST",
        &path,
        Some("admin"),
        json!({}),
        "pending-retry-0001",
        None,
    )
    .await;
    assert_eq!(count(&s).await, 1);
}
#[tokio::test]
async fn invalid_visibility_never_creates_an_application() {
    let (s, _) = setup(true).await;
    let mut config = input("invalid");
    config["visibility"] = json!("anything");
    let (status, _) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        config,
        "invalid-visibility",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(count(&s).await, 0);
}

#[tokio::test]
async fn deferred_request_resumes_after_reconciliation_without_another_upload() {
    let (s, _) = setup(false).await;
    let (status, created) = call(
        &s,
        "POST",
        "/api/v1/apps",
        Some("admin"),
        input("deferred"),
        "deferred-create-0001",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{created}");
    let (_, uploaded) = upload(&s, "deferred", &archive(), "admin").await;
    assert_eq!(uploaded["publication"]["state"], "awaiting_configuration");
    let encrypted: String = sqlx::query_scalar("SELECT encrypted_token FROM publication_intents")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_ne!(encrypted, "admin");
    assert_eq!(s.decrypt(&encrypted).unwrap(), "admin");
    // Model authoritative reconciliation accepting the saved configuration.
    sqlx::query(
        "UPDATE applications SET iam_revision=1,effective_revision=revision,state='active'",
    )
    .execute(&s.db)
    .await
    .unwrap();
    sqlx::query("UPDATE publication_intents SET next_attempt_at=0")
        .execute(&s.db)
        .await
        .unwrap();
    silicon_honeycomb_server::publication_worker::tick(&s)
        .await
        .unwrap();
    assert_eq!(count(&s).await, 1);
    sqlx::query("UPDATE publication_intents SET next_attempt_at=0")
        .execute(&s.db)
        .await
        .unwrap();
    silicon_honeycomb_server::publication_worker::tick(&s)
        .await
        .unwrap();
    assert_eq!(count(&s).await, 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT visibility FROM applications")
            .fetch_one(&s.db)
            .await
            .unwrap(),
        "private"
    );
}
#[tokio::test]
async fn deferred_requests_recheck_authority_and_discard_superseded_or_expired_tokens() {
    for condition in ["revoked", "superseded", "expired"] {
        let (s, revoked) = setup(false).await;
        let (status, created) = call(
            &s,
            "POST",
            "/api/v1/apps",
            Some("admin"),
            input("deferred"),
            "deferred-create-0001",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED, "{created}");
        upload(&s, "deferred", &archive(), "admin").await;
        sqlx::query(
            "UPDATE applications SET iam_revision=1,effective_revision=revision,state='active'",
        )
        .execute(&s.db)
        .await
        .unwrap();
        sqlx::query("UPDATE publication_intents SET next_attempt_at=0")
            .execute(&s.db)
            .await
            .unwrap();
        match condition {
            "revoked" => revoked.store(true, Ordering::SeqCst),
            "superseded" => {
                sqlx::query("UPDATE applications SET revision=2,config=json_set(config,'$.visibility','private')").execute(&s.db).await.unwrap();
            }
            "expired" => {
                sqlx::query("UPDATE publication_intents SET updated_at=1")
                    .execute(&s.db)
                    .await
                    .unwrap();
            }
            _ => unreachable!(),
        }
        silicon_honeycomb_server::publication_worker::tick(&s)
            .await
            .unwrap();
        assert_eq!(count(&s).await, 0, "{condition}");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM publication_intents")
                .fetch_one(&s.db)
                .await
                .unwrap(),
            0,
            "{condition}"
        );
    }
}
