use silicon_honeycomb_server::{
    State, auth::Iam, integration::AwaitingIamIntegration, storage::Briefcase,
};
use std::sync::Arc;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .json()
        .init();
    let required = |name: &str| {
        std::env::var(name).map_err(|_| anyhow::anyhow!("{name} is required; see .env.example"))
    };
    let app_id = required("HONEYCOMB_APP_ID")?;
    let iam_app_id = required("IAM_APPLICATION_ID")?;
    let storage_app_id = required("BRIEFCASE_APP_ID")?;
    for (key, value) in [
        ("HONEYCOMB_APP_ID", &app_id),
        ("IAM_APPLICATION_ID", &iam_app_id),
        ("BRIEFCASE_APP_ID", &storage_app_id),
    ] {
        anyhow::ensure!(
            honeycomb_core::model::valid_app_id(value),
            "{key} must name a registered application"
        );
    }
    anyhow::ensure!(
        app_id != iam_app_id && app_id != storage_app_id && iam_app_id != storage_app_id,
        "Identity, storage and coordinator roles require distinct registered applications"
    );
    let iam_url = std::env::var("IAM_BASE_URL")
        .unwrap_or_else(|_| "https://backend.iam.teamofsilicons.com".into());
    let secret = required("HONEYCOMB_APP_SECRET")?;
    let encryption_key: [u8; 32] = hex::decode(required("HONEYCOMB_ENCRYPTION_KEY")?)?
        .try_into()
        .map_err(|_| {
            anyhow::anyhow!("HONEYCOMB_ENCRYPTION_KEY must be 64 hexadecimal characters")
        })?;
    let identity = Iam::new(&iam_url, &app_id, &secret)?;
    let storage = Briefcase {
        iam: identity.client.clone(),
        http: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(300))
            .build()?,
        base_url: std::env::var("BRIEFCASE_BASE_URL")
            .unwrap_or_else(|_| "https://backend.briefcase.teamofsilicons.com".into()),
        app_id: app_id.clone(),
        audience: storage_app_id,
    };
    let mut state = State {
        telemetry: silicon_honeycomb_server::telemetry::Recorder::from_env(),
        db: silicon_honeycomb_server::database(
            &std::env::var("HONEYCOMB_DATABASE").unwrap_or_else(|_| "data/honeycomb.db".into()),
        )
        .await?,
        identity: Arc::new(identity),
        management: Arc::new(AwaitingIamIntegration),
        storage: Arc::new(storage),
        app_id,
        iam_app_id,
        iam_login_url: std::env::var("IAM_LOGIN_URL")
            .unwrap_or_else(|_| "https://iam.teamofsilicons.com".into()),
        encryption_key,
        webhook_secret: required("HONEYCOMB_WEBHOOK_SECRET")?,
    };
    if let Some(credential) = std::env::var("IAM_HONEYCOMB_SERVICE_CREDENTIAL")
        .ok()
        .filter(|v| !v.is_empty())
    {
        let participants =
            silicon_honeycomb_server::participant_management::ParticipantRegistry::from_json(
                &std::env::var("HONEYCOMB_LIFECYCLE_PARTICIPANTS").unwrap_or_else(|_| "[]".into()),
                |name| std::env::var(name).ok(),
            )?;
        anyhow::ensure!(
            !participants.contains(&state.iam_app_id) && !participants.contains(&state.app_id),
            "Identity and coordinator roles cannot also be external lifecycle participants"
        );
        let management = silicon_honeycomb_server::iam_management::IamManagement::new(
            &iam_url,
            credential,
            state.db.clone(),
            encryption_key,
        )?
        .with_participants(participants);
        state.management = Arc::new(management);
    }
    let notifications = std::env::var("IAM_HONEYCOMB_NOTIFICATION_SIGNING_KEY")
        .ok()
        .filter(|v| !v.is_empty())
        .map(|key| {
            silicon_honeycomb_server::iam_management::notification_router(state.clone(), key.into())
        })
        .unwrap_or_default();
    let bind = std::env::var("HONEYCOMB_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let origins = std::env::var("HONEYCOMB_WEB_ORIGINS")
        .unwrap_or_else(|_| {
            "https://honeycomb.teamofsilicons.com,https://console.honeycomb.teamofsilicons.com"
                .into()
        })
        .split(',')
        .map(|s| s.parse())
        .collect::<std::result::Result<Vec<axum::http::HeaderValue>, _>>()?;
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
        ])
        .allow_headers(
            [
                "authorization",
                "content-type",
                "idempotency-key",
                "if-match",
                "x-testing-environment-key",
                "x-download-receipt",
                "x-honeycomb-telemetry",
                "honeycomb-api-version",
                "honeycomb-client-version",
            ]
            .map(axum::http::HeaderName::from_static),
        );
    let mailer = std::env::var("POSTMARK_SERVER_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
        .map(silicon_honeycomb_server::notifications::Postmark::new)
        .transpose()?;
    tokio::spawn(silicon_honeycomb_server::notifications::run(
        state.clone(),
        mailer,
    ));
    tokio::spawn(silicon_honeycomb_server::reconciliation::run(state.clone()));
    tokio::spawn(silicon_honeycomb_server::contracts::run(state.clone()));
    tokio::spawn(silicon_honeycomb_server::retention_worker::run(
        state.clone(),
    ));
    let app = silicon_honeycomb_server::api::router(state)
        .merge(notifications)
        .layer(cors);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(address=%bind,"Honeycomb listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
