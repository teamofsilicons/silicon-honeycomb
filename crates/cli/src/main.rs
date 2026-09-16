mod progress;
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use fs2::FileExt;
use honeycomb_client::{
    AppInput, Client, Mutation,
    installer::{self, Installed},
    package,
};
use progress::Progress;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "honeycomb",
    version,
    about = "Discover, publish and install Silicon applications.",
    long_about = "Honeycomb is the application library for Carbons and Silicons.\n\nStart: honeycomb search briefcase\nAuthenticate: honeycomb login <slt>\nPublish: honeycomb validate → honeycomb pack → honeycomb apps create → honeycomb releases upload\n\nRepository: https://github.com/teamofsilicons/silicon-honeycomb\nLibrary: https://honeycomb.teamofsilicons.com\nRust client: https://crates.io/crates/silicon-honeycomb-client\n\nEvery command supports --help. Quote app identifiers: 'tos>briefcase'."
)]
struct Cli {
    /// Honeycomb backend origin. HTTPS required outside localhost.
    #[arg(long, global = true, env = "HONEYCOMB_API_URL")]
    api: Option<String>,
    /// Use a saved testing environment ID or its 32-character root key.
    #[arg(long = "test", global = true)]
    test: Option<String>,
    /// Emit structured JSON. Secrets appear only in explicit credential-returning commands.
    #[arg(long, global = true)]
    json: bool,
    /// Reuse this key to safely retry a mutation after an uncertain response.
    #[arg(long, global = true)]
    idempotency_key: Option<String>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Print Honeycomb's MIT license notice without contacting a service.
    License,
    /// Discover the configured IAM app identity and login URL.
    Iam,
    /// Exchange an IAM short-lived token, or use `login status` to check live authentication.
    Login { slt_or_status: String },
    /// Revoke the IAM session and refresh family, then clear the local session.
    Logout,
    /// Search public apps and private apps visible to your current memberships.
    Search {
        #[arg(default_value = "")]
        query: String,
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
    /// Validate honeycomb.yaml and all six required targets, or a .tar.gz archive.
    Validate {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Validate and build an immutable .tar.gz. Creates a template if honeycomb.yaml is absent.
    Pack {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Install the latest or selected app release for this platform. Collision errors suggest --alias.
    Install {
        app_id: String,
        #[arg(long)]
        version: Option<String>,
        #[arg(long,value_parser=parse_alias)]
        alias: Vec<(String, String)>,
    },
    /// Update an installed app to its latest release, preserving aliases.
    Update { app_id: String },
    /// Uninstall only files and launchers owned by this package.
    Uninstall { app_id: String },
    /// Show packages installed in the selected home and testing environment.
    Installed,
    /// Save a rating and review for an application you can access.
    Review {
        app_id: String,
        #[arg(long)]
        rating: f64,
        #[arg(long)]
        review: String,
    },
    /// Report a reproducible bug; optionally link a pull request with its fix.
    Report {
        message: String,
        #[arg(long)]
        pr: Option<String>,
    },
    /// Star an application; repeat safely. Use --remove to remove the star.
    Star {
        app_id: String,
        #[arg(long)]
        remove: bool,
    },
    /// Create, inspect and configure organization-owned applications.
    Apps {
        #[command(subcommand)]
        command: Apps,
    },
    /// Upload and inspect immutable CLI releases. Run pack before upload.
    Releases {
        #[command(subcommand)]
        command: Releases,
    },
    /// Request publication and participate in application review discussions.
    Publication {
        #[command(subcommand)]
        command: Publication,
    },
    /// Save shared drafts with revision checks, allowing another admin to continue.
    Drafts {
        #[command(subcommand)]
        command: Drafts,
    },
    /// Inspect pending operations or retry after repairing an integration.
    Operations {
        #[command(subcommand)]
        command: Operations,
    },
    /// Create and manage isolated ecosystem testing environments.
    Environments {
        #[command(subcommand)]
        command: Environments,
    },
    /// Configure home, backend and update/telemetry preferences.
    Config {
        #[command(subcommand)]
        command: Config,
    },
    /// Run the hourly update worker, including while no interactive commands run.
    Daemon {
        /// Run one scheduled check and exit, honoring auto_update.
        #[arg(long)]
        once: bool,
    },
    /// Check for and install the latest verified Honeycomb CLI binary.
    SelfUpdate,
    /// Register the per-user hourly update worker.
    Service {
        #[arg(value_parser=["install"])]
        action: String,
    },
}
#[derive(Subcommand)]
enum Webhook {
    /// Read the current pending endpoint ID and IAM revision before making a change.
    Status { app_id: String },
    /// Approve the exact pending destination. Obtain application.webhook.approve step-up from IAM.
    Approve {
        app_id: String,
        #[arg(long)]
        endpoint: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        step_up_file: Option<PathBuf>,
    },
    /// Install a signing secret from a protected file. Update your receiver to verify it.
    RotateSecret {
        app_id: String,
        #[arg(long)]
        secret_file: PathBuf,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        step_up_file: Option<PathBuf>,
    },
    /// Retry a saved change by operation ID; omit the secret and original endpoint.
    Retry {
        operation_id: String,
        #[arg(long)]
        step_up_file: Option<PathBuf>,
    },
}
#[derive(Subcommand)]
enum Apps {
    /// Upload a publicly viewable logo to Briefcase. Set the returned logo_url in application.json.
    UploadLogo {
        org_id: String,
        file: PathBuf,
    },
    /// Manage pending webhook destinations and signing credentials through IAM.
    Webhook {
        #[command(subcommand)]
        command: Webhook,
    },
    /// Refresh accepted IAM state and retry notification reconciliation.
    Reconcile {
        app_id: String,
    },
    /// Replace an application's secret through IAM. Save the one-time result securely.
    RotateSecret {
        app_id: String,
        #[arg(long)]
        revision: i64,
        /// Read fresh IAM step-up evidence from a file, avoiding shell-history exposure.
        #[arg(long)]
        step_up_file: Option<PathBuf>,
    },
    List {
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
    Get {
        app_id: String,
    },
    /// Submit application.json containing org_id, local_app_id, details, webhook and scopes.
    Create {
        file: PathBuf,
    },
    /// Update using a complete application.json and the current revision from apps get.
    Update {
        app_id: String,
        file: PathBuf,
        #[arg(long)]
        revision: i64,
    },
}
#[derive(Subcommand)]
enum Releases {
    List {
        app_id: String,
    },
    Upload {
        app_id: String,
        archive: PathBuf,
        #[arg(long)]
        revision: i64,
    },
}
#[derive(Subcommand)]
enum Publication {
    /// Activate an approved public revision and reconcile archive access.
    Activate {
        request_id: String,
        #[arg(long)]
        revision: i64,
    },
    /// Requests you can review as a provider administrator or authorized validator.
    Inbox,
    Review {
        request_id: String,
        provider: String,
    },
    RetryPlan {
        request_id: String,
        #[arg(long)]
        revision: i64,
    },
    Decide {
        request_id: String,
        provider: String,
        #[arg(value_parser=["approve","deny"])]
        decision: String,
        #[arg(long, default_value = "")]
        reason: String,
        #[arg(long)]
        revision: i64,
    },
    ReviewReply {
        request_id: String,
        provider: String,
        #[arg(long)]
        message: String,
    },
    Get {
        app_id: String,
    },
    Request {
        app_id: String,
        #[arg(long)]
        message: String,
        #[arg(long)]
        revision: i64,
    },
    Reply {
        app_id: String,
        #[arg(long)]
        message: String,
        #[arg(long)]
        provider: Option<String>,
    },
}
#[derive(Subcommand)]
enum Drafts {
    List {
        org_id: String,
    },
    Save {
        org_id: String,
        id: String,
        file: PathBuf,
        #[arg(long, default_value_t = 0)]
        revision: i64,
    },
}
#[derive(Subcommand)]
enum Operations {
    /// Recover a creation/rotation secret within IAM's short replay window.
    RecoverSecret {
        id: String,
    },
    Get {
        id: String,
    },
    Retry {
        id: String,
    },
}
#[derive(Subcommand)]
enum Environments {
    /// Inspect activity and retention deadlines without extending the idle timer.
    Retention {
        id: String,
    },
    /// Set the shared environment idle period. App-specific longer retention still applies.
    SetRetention {
        id: String,
        #[arg(long)]
        days: i64,
        #[arg(long)]
        revision: i64,
    },
    /// Report actual use of an application in a ready environment; supports --test root authority.
    Activity {
        id: String,
        app_id: String,
        #[arg(long)]
        generation: i64,
        #[arg(long)]
        key_version: i64,
    },
    List,
    Get {
        id: String,
    },
    Create {
        org_id: String,
        name: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Import an application's external-scope dependencies and pin accepted configurations.
    Import {
        id: String,
        app_id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        release: Option<String>,
        /// Explicitly refresh the imported dependency graph from accepted production configurations.
        #[arg(long)]
        refresh: bool,
    },
    /// Retrieve and save an environment root key. This action is audited by the backend.
    Key {
        id: String,
    },
    /// Coordinate rotation, clean, delete or restore with every participating service.
    Action {
        id: String,
        #[arg(value_parser=["rotate-key","clean","delete","restore","retry","purge"])]
        action: String,
        #[arg(long)]
        revision: i64,
    },
}
#[derive(Subcommand)]
enum Config {
    /// Use an existing directory as home. Data lives below <home>/.honeycomb/dir.
    Home {
        location: PathBuf,
    },
    Show,
    /// Print shell setup for this home and backend. Use eval "$(honeycomb config env)".
    Env,
    /// Set api, auto_update or telemetry. Boolean values are true/false.
    Set {
        #[arg(value_parser=["api","auto_update","telemetry"])]
        name: String,
        value: String,
    },
}
#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    api: String,
    auto_update: bool,
    telemetry: bool,
    #[serde(default)]
    last_update_check: i64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            api: "https://backend.honeycomb.teamofsilicons.com".into(),
            auto_update: true,
            telemetry: true,
            last_update_check: 0,
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
struct Session {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_at: i64,
}
/// Private because the selected environment key grants test root authority.
#[derive(Serialize, Deserialize)]
struct InstallContext {
    api: String,
    testing_key: Option<String>,
}
impl InstallContext {
    fn client(&self, telemetry: bool) -> Result<Client> {
        let client = Client::new(&self.api)?.with_telemetry(telemetry);
        match &self.testing_key {
            Some(key) => client.with_environment(key),
            None => Ok(client),
        }
    }
    fn directory(&self, root: &Path) -> PathBuf {
        root.join("contexts")
            .join(honeycomb_client::context_fingerprint(
                &self.api,
                self.testing_key.as_deref(),
            ))
    }
}
fn selected_context(cli: &Cli, settings: &Settings, root: &Path) -> Result<InstallContext> {
    let testing_key = if let Some(test) = &cli.test {
        let keys: BTreeMap<String, String> = read(&root.join("environments.json"))?;
        Some(keys.get(test).cloned().unwrap_or_else(|| test.clone()))
    } else {
        None
    };
    let context = InstallContext {
        api: cli.api.as_deref().unwrap_or(&settings.api).to_owned(),
        testing_key,
    };
    context.client(false)?;
    Ok(context)
}
fn shell_environment(root: &Path, context: &InstallContext) -> String {
    let path = format!(
        "{}:{}",
        root.join("system/bin").display(),
        context.directory(root).join("bin").display()
    );
    let quoted = path.replace('\'', "'\\''");
    format!("export PATH='{quoted}':\"$PATH\"")
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn base_home() -> Result<PathBuf> {
    std::env::var_os("SILICON_HOME")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .context("Cannot resolve home directory; set SILICON_HOME")
}
fn root() -> Result<PathBuf> {
    let base = base_home()?;
    let pointer = base.join(".honeycomb/home.json");
    let home: PathBuf = if pointer.exists() {
        serde_json::from_slice(&fs::read(pointer)?)?
    } else {
        base
    };
    Ok(home.join(".honeycomb/dir"))
}
fn read<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match fs::read(path) {
        Ok(b) => serde_json::from_slice(&b).with_context(|| {
            format!(
                "{} is invalid JSON; repair it before continuing",
                path.display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}
fn write_private(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("Missing file parent")?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let temp = parent.join(format!(".write-{}", uuid::Uuid::new_v4()));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&temp)?;
    f.write_all(&serde_json::to_vec_pretty(value)?)?;
    f.sync_all()?;
    fs::rename(temp, path)?;
    Ok(())
}
fn parse_alias(s: &str) -> std::result::Result<(String, String), String> {
    s.split_once('=')
        .filter(|(a, b)| package::safe_command(a) && package::safe_command(b))
        .map(|(a, b)| (a.into(), b.into()))
        .ok_or_else(|| "Use --alias original=new-command".into())
}
fn input<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("{} is not valid input JSON", path.display()))
}
fn show(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
fn mutation(cli: &Cli, revision: Option<i64>) -> Result<Mutation> {
    let m = Mutation {
        idempotency_key: cli
            .idempotency_key
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        revision,
    };
    if !(16..=255).contains(&m.idempotency_key.len())
        || !m.idempotency_key.bytes().all(|b| b.is_ascii_graphic())
    {
        bail!("--idempotency-key requires 16–255 visible ASCII characters");
    }
    Ok(m)
}
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        let as_json = std::env::args().any(|a| a == "--json");
        if as_json {
            eprintln!("{}", json!({"error":{"message":error.to_string()}}));
        } else {
            eprintln!("honeycomb: {error:#}");
        }
        std::process::exit(1);
    }
}
async fn run() -> Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Command::License) {
        if cli.json {
            show(&json!({"license":"MIT","text":honeycomb_client::LICENSE_TEXT}))?;
        } else {
            print!("{}", honeycomb_client::LICENSE_TEXT);
        }
        return Ok(());
    }
    let started = std::time::Instant::now();
    let message = match &cli.command {
        Command::Install { app_id, .. } => Some(format!("Starting installation of {app_id}")),
        Command::Update { app_id } => Some(format!("Starting update of {app_id}")),
        Command::SelfUpdate => Some("Starting Honeycomb update".to_owned()),
        _ => None,
    };
    let progress = Progress::new(
        !cli.json && message.is_some(),
        message.as_deref().unwrap_or(""),
    );
    let result = execute(&cli, &progress).await;
    progress.finish(result.is_ok());
    record_command(
        &cli,
        "cli",
        "command_completed",
        started.elapsed().as_millis() as u64,
        result.is_ok(),
    )
    .await;
    result
}
async fn execute(cli: &Cli, progress: &Progress) -> Result<()> {
    let root = root()?;
    fs::create_dir_all(&root)?;
    let mut settings: Settings = read(&root.join("config.json"))?;
    if let Command::Config { command } = &cli.command {
        match command {
            Config::Home { location } => {
                if !location.is_dir() {
                    bail!("{} is not a directory", location.display());
                }
                let home = location.canonicalize()?;
                write_private(&base_home()?.join(".honeycomb/home.json"), &home)?;
                show(&json!({"home":home,"data_directory":home.join(".honeycomb/dir")}))?;
            }
            Config::Env => {
                let context = selected_context(cli, &settings, &root)?;
                println!("{}", shell_environment(&root, &context));
            }
            Config::Show => show(
                &json!({"home":root.parent().and_then(Path::parent),"data_directory":root,"settings":settings}),
            )?,
            Config::Set { name, value } => {
                match name.as_str() {
                    "api" => {
                        Client::new(value)?;
                        settings.api = value.clone();
                    }
                    "auto_update" => {
                        settings.auto_update =
                            value.parse().context("auto_update must be true or false")?
                    }
                    "telemetry" => {
                        settings.telemetry =
                            value.parse().context("telemetry must be true or false")?
                    }
                    _ => unreachable!(),
                }
                write_private(&root.join("config.json"), &settings)?;
                show(&json!({"saved":true,name:value}))?;
            }
        }
        return Ok(());
    }
    if matches!(cli.command, Command::Service { .. }) {
        honeycomb_client::maintenance::install_service(&base_home()?, &std::env::current_exe()?)?;
        show(&json!({"service":"installed"}))?;
        return Ok(());
    }
    if matches!(cli.command, Command::SelfUpdate) {
        return self_update(&root, true, Some(progress)).await;
    }
    if let Command::Daemon { once } = cli.command {
        loop {
            let config: Settings = read(&root.join("config.json"))?;
            if config.auto_update {
                let started = std::time::Instant::now();
                // Check packages even if the Honeycomb binary update server is unavailable.
                let packages = update_packages(&root, cli, false).await;
                let result = self_update(&root, false, None).await.and(packages);
                record_command(
                    cli,
                    "daemon",
                    "update_completed",
                    started.elapsed().as_millis() as u64,
                    result.is_ok(),
                )
                .await;
                if result.is_err() {
                    eprintln!("Honeycomb maintenance deferred; run honeycomb update for details");
                }
            }
            if once {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    }
    let context = selected_context(cli, &settings, &root)?;
    let mut client = context.client(telemetry_enabled(settings.telemetry))?;
    let plane_dir = context.directory(&root);
    fs::create_dir_all(&plane_dir)?;
    let session_path = plane_dir.join("session.json");
    let mut session: Session = read(&session_path)?;
    if !matches!(
        cli.command,
        Command::Login { .. }
            | Command::Logout
            | Command::Validate { .. }
            | Command::Pack { .. }
            | Command::Installed
            | Command::Uninstall { .. }
    ) && session.access_token.is_some()
        && session.expires_at <= now() + 30
        && let Some(refresh) = &session.refresh_token
    {
        let tokens = client.refresh(refresh, &mutation(cli, None)?).await?;
        session = Session {
            access_token: tokens["access_token"].as_str().map(str::to_owned),
            refresh_token: tokens["refresh_token"].as_str().map(str::to_owned),
            expires_at: now() + tokens["expires_in"].as_i64().unwrap_or(0),
        };
        write_private(&session_path, &session)?;
    }
    if let Some(token) = &session.access_token {
        client = client.with_token(token);
    }
    let operation = mutation(cli, None)?;
    match &cli.command {
        Command::Iam => show(&client.iam().await?)?,
        Command::Login { slt_or_status } if slt_or_status == "status" => {
            if session.access_token.is_none() {
                show(&json!({"authenticated":false}))?;
            } else {
                show(&client.login_status().await?)?;
            }
        }
        Command::Login { slt_or_status } => {
            let tokens = client.login(slt_or_status, &operation).await?;
            let access = tokens["access_token"]
                .as_str()
                .context("IAM login returned no access token")?;
            let session = Session {
                access_token: Some(access.into()),
                refresh_token: tokens["refresh_token"].as_str().map(str::to_owned),
                expires_at: now() + tokens["expires_in"].as_i64().unwrap_or(0),
            };
            write_private(&session_path, &session)?;
            show(&client.with_token(access).login_status().await?)?;
        }
        Command::Logout => {
            if session.access_token.is_some() {
                client
                    .logout_session(session.refresh_token.as_deref(), &operation)
                    .await?;
            }
            if session_path.exists() {
                fs::remove_file(&session_path)?;
            }
            show(&json!({"authenticated":false}))?;
        }
        Command::Report { message, pr } => {
            show(&client.report(message, pr.as_deref(), &operation).await?)?
        }
        Command::Search { query, page } => show(&client.search(query, *page, false).await?)?,
        Command::Validate { path } => {
            let v = package::validate(path);
            show(&v)?;
            if !v.valid {
                bail!(
                    "Validation failed. Fix every reported path or field, then run honeycomb validate again"
                );
            }
            if !cli.json {
                eprintln!("Validation passed. Run honeycomb pack to create the archive.");
            }
        }
        Command::Pack { path, output } => {
            let archive = package::pack(path, output.as_deref())?;
            show(&json!({"archive":archive,"sha256":package::sha256(&archive)?}))?;
        }
        Command::Apps { command } => match command {
            Apps::Webhook { command } => match command {
                Webhook::Status { app_id } => show(&client.webhook_state(app_id).await?)?,
                Webhook::Approve {
                    app_id,
                    endpoint,
                    revision,
                    step_up_file,
                } => {
                    let proof = step_up_file.as_ref().map(fs::read_to_string).transpose()?;
                    show(
                        &client
                            .approve_webhook(
                                app_id,
                                endpoint,
                                proof.as_deref().map(str::trim),
                                &mutation(cli, Some(*revision))?,
                            )
                            .await?,
                    )?;
                }
                Webhook::RotateSecret {
                    app_id,
                    secret_file,
                    revision,
                    step_up_file,
                } => {
                    let secret = fs::read_to_string(secret_file)?;
                    let proof = step_up_file.as_ref().map(fs::read_to_string).transpose()?;
                    show(
                        &client
                            .rotate_webhook_secret(
                                app_id,
                                secret.trim(),
                                proof.as_deref().map(str::trim),
                                &mutation(cli, Some(*revision))?,
                            )
                            .await?,
                    )?;
                }
                Webhook::Retry {
                    operation_id,
                    step_up_file,
                } => {
                    let proof = step_up_file.as_ref().map(fs::read_to_string).transpose()?;
                    show(
                        &client
                            .retry_webhook_operation(
                                operation_id,
                                proof.as_deref().map(str::trim),
                                &mutation(cli, None)?,
                            )
                            .await?,
                    )?;
                }
            },
            Apps::RotateSecret {
                app_id,
                revision,
                step_up_file,
            } => {
                let step_up = step_up_file.as_ref().map(fs::read_to_string).transpose()?;
                show(
                    &client
                        .rotate_app_secret(
                            app_id,
                            step_up.as_deref().map(str::trim),
                            &mutation(cli, Some(*revision))?,
                        )
                        .await?,
                )?;
            }
            Apps::Reconcile { app_id } => {
                show(&client.reconcile_app(app_id, &mutation(cli, None)?).await?)?
            }
            Apps::UploadLogo { org_id, file } => show(
                &client
                    .upload_logo(org_id, file, &mutation(cli, None)?)
                    .await?,
            )?,
            Apps::List { page } => show(&client.search("", *page, true).await?)?,
            Apps::Get { app_id } => show(&client.app(app_id).await?)?,
            Apps::Create { file } => show(
                &client
                    .create_app(&input::<AppInput>(file)?, &operation)
                    .await?,
            )?,
            Apps::Update {
                app_id,
                file,
                revision,
            } => show(
                &client
                    .update_app(
                        app_id,
                        &input::<AppInput>(file)?,
                        &mutation(cli, Some(*revision))?,
                    )
                    .await?,
            )?,
        },
        Command::Releases { command } => match command {
            Releases::List { app_id } => show(&client.releases(app_id).await?)?,
            Releases::Upload {
                app_id,
                archive,
                revision,
            } => show(
                &client
                    .upload_release(app_id, archive, &mutation(cli, Some(*revision))?)
                    .await?,
            )?,
        },
        Command::Publication { command } => match command {
            Publication::Activate {
                request_id,
                revision,
            } => show(
                &client
                    .activate_publication(request_id, &mutation(cli, Some(*revision))?)
                    .await?,
            )?,
            Publication::Inbox => show(&client.review_inbox().await?)?,
            Publication::Review {
                request_id,
                provider,
            } => show(&client.review_request(request_id, provider).await?)?,
            Publication::RetryPlan {
                request_id,
                revision,
            } => show(
                &client
                    .retry_review_plan(request_id, &mutation(cli, Some(*revision))?)
                    .await?,
            )?,
            Publication::Decide {
                request_id,
                provider,
                decision,
                reason,
                revision,
            } => show(
                &client
                    .decide_review(
                        request_id,
                        provider,
                        decision,
                        reason,
                        &mutation(cli, Some(*revision))?,
                    )
                    .await?,
            )?,
            Publication::ReviewReply {
                request_id,
                provider,
                message,
            } => show(
                &client
                    .reply_review(request_id, provider, message, &operation)
                    .await?,
            )?,
            Publication::Get { app_id } => show(&client.publication(app_id).await?)?,
            Publication::Request {
                app_id,
                message,
                revision,
            } => show(
                &client
                    .request_publication(app_id, message, &mutation(cli, Some(*revision))?)
                    .await?,
            )?,
            Publication::Reply {
                app_id,
                message,
                provider,
            } => show(
                &client
                    .publication_message(app_id, message, provider.as_deref(), &operation)
                    .await?,
            )?,
        },
        Command::Drafts { command } => match command {
            Drafts::List { org_id } => show(&client.drafts(org_id).await?)?,
            Drafts::Save {
                org_id,
                id,
                file,
                revision,
            } => show(
                &client
                    .save_draft(
                        org_id,
                        id,
                        &input::<Value>(file)?,
                        &mutation(cli, Some(*revision))?,
                    )
                    .await?,
            )?,
        },
        Command::Operations { command } => match command {
            Operations::RecoverSecret { id } => {
                show(&client.recover_operation_secret(id, &operation).await?)?
            }
            Operations::Get { id } => show(&client.operation(id).await?)?,
            Operations::Retry { id } => show(&client.retry_operation(id, &operation).await?)?,
        },
        Command::Environments { command } => match command {
            Environments::Retention { id } => show(&client.environment_retention(id).await?)?,
            Environments::SetRetention { id, days, revision } => show(
                &client
                    .set_environment_retention(id, *days, &mutation(cli, Some(*revision))?)
                    .await?,
            )?,
            Environments::Activity {
                id,
                app_id,
                generation,
                key_version,
            } => show(
                &client
                    .report_environment_activity(id, app_id, *generation, *key_version, &operation)
                    .await?,
            )?,
            Environments::List => show(&client.environments().await?)?,
            Environments::Get { id } => show(&client.environment(id).await?)?,
            Environments::Create {
                org_id,
                name,
                description,
            } => show(
                &client
                    .create_environment(org_id, name, description, &operation)
                    .await?,
            )?,
            Environments::Import {
                id,
                app_id,
                revision,
                release,
                refresh,
            } => show(
                &client
                    .import_application(
                        id,
                        app_id,
                        release.as_deref(),
                        *refresh,
                        &mutation(cli, Some(*revision))?,
                    )
                    .await?,
            )?,
            Environments::Key { id } => {
                let value = client.environment_key(id, &operation).await?;
                let mut keys: BTreeMap<String, String> = read(&root.join("environments.json"))?;
                keys.insert(
                    id.clone(),
                    value["testing_key"]
                        .as_str()
                        .context("No environment key returned")?
                        .into(),
                );
                write_private(&root.join("environments.json"), &keys)?;
                show(&value)?;
            }
            Environments::Action {
                id,
                action,
                revision,
            } => {
                let result = client
                    .environment_action(id, action, &mutation(cli, Some(*revision))?)
                    .await?;
                if let Some(key) = result["testing_key"].as_str() {
                    let mut keys: BTreeMap<String, String> = read(&root.join("environments.json"))?;
                    keys.insert(id.clone(), key.into());
                    write_private(&root.join("environments.json"), &keys)?;
                }
                show(&result)?;
            }
        },
        Command::Review {
            app_id,
            rating,
            review,
        } => show(&client.review(app_id, *rating, review, &operation).await?)?,
        Command::Star { app_id, remove } => show(&client.star(app_id, !remove, &operation).await?)?,
        Command::Installed => {
            let records: BTreeMap<String, Installed> = read(&plane_dir.join("installed.json"))?;
            show(&records)?;
        }
        Command::Install {
            app_id,
            version,
            alias,
        } => {
            install(
                &client,
                &plane_dir,
                app_id,
                version.as_deref(),
                Some(alias.iter().cloned().collect()),
                false,
                Some(progress),
            )
            .await?;
            write_private(&plane_dir.join("context.json"), &context)?;
        }
        Command::Update { app_id } => {
            install(
                &client,
                &plane_dir,
                app_id,
                None,
                None,
                true,
                Some(progress),
            )
            .await?;
            write_private(&plane_dir.join("context.json"), &context)?;
        }
        Command::Uninstall { app_id } => {
            let lock = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(plane_dir.join("install.lock"))?;
            lock.lock_exclusive()?;
            let mut records: BTreeMap<String, Installed> = read(&plane_dir.join("installed.json"))?;
            let record = records
                .get(app_id)
                .context("Application is not installed in this context")?;
            installer::uninstall(record, &plane_dir)?;
            records.remove(app_id);
            write_private(&plane_dir.join("installed.json"), &records)?;
            show(
                &json!({"uninstalled":app_id,"review_command":format!("honeycomb review '{app_id}' --rating 4.7 --review 'Your review'")}),
            )?;
        }
        Command::License
        | Command::Config { .. }
        | Command::Daemon { .. }
        | Command::SelfUpdate
        | Command::Service { .. } => unreachable!(),
    }
    if settings.auto_update {
        progress.stage("Checking scheduled updates");
    }
    if settings.auto_update && update_packages(&root, cli, true).await.is_err() {
        eprintln!("Package update check deferred; run honeycomb update for details");
    }
    // Maintenance failures must not change the user's successful command result.
    if settings.auto_update
        && now() - settings.last_update_check >= 3600
        && let Err(e) = self_update(&root, false, None).await
    {
        eprintln!("Update check deferred: {e}");
    }
    Ok(())
}
async fn install(
    client: &Client,
    root: &Path,
    id: &str,
    version: Option<&str>,
    aliases: Option<BTreeMap<String, String>>,
    updating: bool,
    progress: Option<&Progress>,
) -> Result<()> {
    if !honeycomb_client::valid_app_id(id) {
        bail!("Expected an org>app identifier; quote it in your shell");
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("install.lock"))?;
    if let Some(progress) = progress {
        progress.stage("Waiting for the installation lock");
    }
    lock.lock_exclusive()?;
    let mut records: BTreeMap<String, Installed> = read(&root.join("installed.json"))?;
    let previous = records.get(id);
    if updating && previous.is_none() {
        bail!("{id} is not installed; run honeycomb install '{id}' first");
    }
    if let Some(progress) = progress {
        progress.stage("Finding the requested release");
    }
    let releases = client.releases(id).await?;
    let release = releases
        .iter()
        .find(|r| version.is_none_or(|v| v == r.version))
        .context("Requested release is unavailable")?;
    if previous.is_some_and(|p| p.version == release.version && p.sha256 == release.sha256) {
        if let Some(progress) = progress {
            progress.pause();
            show(&json!({"app_id":id,"version":release.version,"status":"already_up_to_date"}))?;
        }
        return Ok(());
    }
    let aliases = aliases
        .or_else(|| previous.map(|p| p.aliases.clone()))
        .unwrap_or_default();
    let stage = root.join(format!("download-{}.tar.gz", uuid::Uuid::new_v4()));
    let result = async {
        client
            .download_with_progress(id, release, &stage, &|event| {
                if let Some(progress) = progress {
                    progress.event(event);
                }
            })
            .await?;
        if let Some(progress) = progress {
            progress.stage("Unpacking and activating commands");
        }
        installer::install_archive_for(id, &stage, root, &aliases, previous)
    }
    .await;
    let _ = fs::remove_file(&stage);
    let record = result?;
    let first = record
        .commands
        .values()
        .next()
        .context("No commands installed")?
        .clone();
    if let Some(progress) = progress {
        progress.stage("Saving installation record");
    }
    records.insert(id.into(), record.clone());
    write_private(&root.join("installed.json"), &records)?;
    if let Some(progress) = progress {
        progress.pause();
        show(
            &json!({"app_id":id,"version":record.version,"status":"installed","bin_directory":root.join("bin"),"help_command":format!("{} --help",first.display())}),
        )?;
    }
    Ok(())
}
/// Resolve each package against its original origin and testing context. Never guess a
/// missing test context or borrow credentials from production.
async fn update_context(root: &Path, context: &InstallContext, telemetry: bool) -> Result<()> {
    let directory = context.directory(root);
    let records: BTreeMap<String, Installed> = read(&directory.join("installed.json"))?;
    if records.is_empty() {
        return Ok(());
    }
    let mut client = context.client(telemetry)?;
    let session_path = directory.join("session.json");
    let mut session: Session = read(&session_path)?;
    if session.access_token.is_some() && session.expires_at <= now() + 30 {
        let refresh = session
            .refresh_token
            .as_ref()
            .context("Package update requires renewed login")?;
        let tokens = client.refresh(refresh, &Mutation::new()).await?;
        session = Session {
            access_token: tokens["access_token"].as_str().map(str::to_owned),
            refresh_token: tokens["refresh_token"].as_str().map(str::to_owned),
            expires_at: now() + tokens["expires_in"].as_i64().unwrap_or(0),
        };
        write_private(&session_path, &session)?;
    }
    if let Some(token) = session.access_token {
        client = client.with_token(token);
    }
    let mut failed = false;
    for id in records.keys() {
        // Failure for one package must not prevent other installed packages updating.
        failed |= install(&client, &directory, id, None, None, true, None)
            .await
            .is_err();
    }
    if failed {
        bail!("One or more package updates failed; existing installations were retained");
    }
    Ok(())
}
async fn update_packages(root: &Path, cli: &Cli, selected_only: bool) -> Result<()> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("package-update.lock"))?;
    if lock.try_lock_exclusive().is_err() {
        return Ok(());
    }
    let settings: Settings = read(&root.join("config.json"))?;
    if !settings.auto_update {
        return Ok(());
    }
    let selected = selected_context(cli, &settings, root)?;
    let mut contexts = BTreeMap::new();
    contexts.insert(selected.directory(root), selected);
    if !selected_only && cli.api.is_none() && cli.test.is_none() {
        let entries = match fs::read_dir(root.join("contexts")) {
            Ok(entries) => Some(entries),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        for entry in entries.into_iter().flatten() {
            let path = entry?.path();
            let metadata = path.join("context.json");
            if !metadata.exists() {
                continue;
            }
            let context: InstallContext = serde_json::from_slice(&fs::read(metadata)?)?;
            if context.directory(root) != path {
                bail!("Installed package context does not match its directory");
            }
            contexts.insert(path, context);
        }
    }
    let mut failed = false;
    for (directory, context) in contexts {
        let checked: i64 = read(&directory.join("last-package-update.json"))?;
        if now() - checked < 3600 {
            continue;
        }
        let records: BTreeMap<String, Installed> = read(&directory.join("installed.json"))?;
        if records.is_empty() {
            continue;
        }
        write_private(&directory.join("last-package-update.json"), &now())?;
        failed |= update_context(root, &context, telemetry_enabled(settings.telemetry))
            .await
            .is_err();
    }

    if failed {
        bail!("Package maintenance needs attention");
    }
    Ok(())
}
async fn self_update(root: &Path, force: bool, progress: Option<&Progress>) -> Result<()> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("update.lock"))?;
    if lock.try_lock_exclusive().is_err() {
        if let Some(progress) = progress {
            progress.stage("Another Honeycomb update is already running");
        }
        return Ok(());
    }
    let mut settings: Settings = read(&root.join("config.json"))?;
    if !force && now() - settings.last_update_check < 3600 {
        return Ok(());
    }
    settings.last_update_check = now();
    write_private(&root.join("config.json"), &settings)?;
    let executable = std::env::current_exe()?;
    let updated = honeycomb_client::maintenance::update_cli_with_progress(
        &root.join("system"),
        &executable,
        &|event| {
            if let Some(progress) = progress {
                progress.event(event);
            }
        },
    )
    .await?;
    if let Some(progress) = progress {
        progress.stage(if updated {
            "Honeycomb updated"
        } else {
            "Honeycomb is already up to date"
        });
    }
    if force {
        if let Some(progress) = progress {
            progress.pause();
        }
        show(&json!({"updated":updated,"executable":executable}))?;
    }
    Ok(())
}

fn telemetry_enabled(preference: bool) -> bool {
    preference
        && !std::env::var("HONEYCOMB_TELEMETRY").is_ok_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "no"
            )
        })
}
async fn record_command(cli: &Cli, source: &str, event: &str, duration: u64, success: bool) {
    let _ = async {
        let root = root()?;
        let settings: Settings = read(&root.join("config.json"))?;
        if !telemetry_enabled(settings.telemetry) {
            return Ok::<(), anyhow::Error>(());
        }
        let origin = cli.api.as_deref().unwrap_or(&settings.api);
        let mut client = Client::new(origin)?;
        let selected = if let Some(test) = &cli.test {
            let keys: BTreeMap<String, String> = read(&root.join("environments.json"))?;
            Some(keys.get(test).cloned().unwrap_or_else(|| test.clone()))
        } else {
            None
        };
        if let Some(key) = &selected {
            client = client.with_environment(key)?;
        }
        let context = honeycomb_client::context_fingerprint(origin, selected.as_deref());
        let session: Session = read(&root.join("contexts").join(context).join("session.json"))?;
        let Some(token) = session.access_token else {
            return Ok(());
        };
        let action = match &cli.command {
            Command::License => return Ok(()),
            Command::Iam => "iam",
            Command::Search { .. } => "search",
            Command::Login { .. } => "login",
            Command::Logout => "logout",
            Command::Apps { .. } => "apps",
            Command::Releases { .. } => "releases",
            Command::Publication { .. } => "publication",
            Command::Drafts { .. } => "drafts",
            Command::Environments { .. } => "environments",
            Command::Operations { .. } => "operations",
            Command::Validate { .. } => "validate",
            Command::Pack { .. } => "pack",
            Command::Install { .. } => "install",
            Command::Update { .. } => "update",
            Command::Uninstall { .. } => "uninstall",
            Command::Installed => "installed",
            Command::Review { .. } => "review",
            Command::Report { .. } => "report",
            Command::Star { .. } => "star",
            Command::Config { .. } => "config",
            Command::SelfUpdate => "self_update",
            Command::Daemon { .. } => "daemon",
            Command::Service { .. } => "service",
        };
        client
            .with_token(token)
            .diagnostic(source, event, action, duration, success)
            .await;
        Ok(())
    }
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("honeycomb-cli-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture(root: &Path, version: &str) -> PathBuf {
        let mut targets = serde_json::Map::new();
        for target in package::REQUIRED_TARGETS {
            let dir = root.join(format!("targets/{target}/bin"));
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join("app");
            fs::write(&path, format!("payload {version}")).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
            }
            targets.insert(
                target.into(),
                json!({"root":format!("targets/{target}"),"executables":{"app":"bin/app"}}),
            );
        }
        fs::write(root.join("honeycomb.yaml"), json!({"format_version":1,"app_id":"fixture>package","version":version,"bin":{"honeycomb-fixture-command":"app"},"targets":targets}).to_string()).unwrap();
        package::pack(root, None).unwrap()
    }
    async fn release_server(
        archive: &Path,
        corrupt: bool,
        key: Option<&str>,
        token: &str,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let sha = package::sha256(archive).unwrap();
        let bytes = fs::read(archive).unwrap();
        let release = json!({"items":[{"app_id":"fixture>package","version":"1.1.0","sha256":sha,"size":bytes.len(),"created_at":1}]}).to_string().into_bytes();
        let key = key.map(str::to_owned);
        let token = token.to_owned();
        let task = tokio::spawn(async move {
            for step in 0..2 {
                let (mut stream, _) =
                    tokio::time::timeout(std::time::Duration::from_secs(10), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let count = stream.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&chunk[..count]);
                    if request.windows(4).any(|x| x == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(request).unwrap().to_lowercase();
                assert!(
                    request.contains(&format!("authorization: bearer {}", token.to_lowercase()))
                );
                if let Some(key) = &key {
                    assert!(request.contains(&format!(
                        "x-testing-environment-key: {}",
                        key.to_lowercase()
                    )));
                } else {
                    assert!(!request.contains("x-testing-environment-key:"));
                }
                assert!(request.contains(if step == 0 {
                    "/releases "
                } else {
                    "/download?version=1.1.0 "
                }));
                let body = if step == 0 {
                    release.clone()
                } else if corrupt {
                    b"corrupted archive".to_vec()
                } else {
                    bytes.clone()
                };
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n", body.len()).as_bytes()).await.unwrap();
                stream.write_all(&body).await.unwrap();
            }
        });
        (origin, task)
    }
    fn seed(root: &Path, context: &InstallContext, archive: &Path, token: &str) -> Installed {
        let directory = context.directory(root);
        fs::create_dir_all(&directory).unwrap();
        let aliases = BTreeMap::from([(
            "honeycomb-fixture-command".into(),
            "honeycomb-fixture-alias".into(),
        )]);
        let record = installer::install_archive(archive, &directory, &aliases, None).unwrap();
        write_private(
            &directory.join("installed.json"),
            &BTreeMap::from([(record.app_id.clone(), record.clone())]),
        )
        .unwrap();
        write_private(&directory.join("context.json"), context).unwrap();
        write_private(
            &directory.join("session.json"),
            &Session {
                access_token: Some(token.into()),
                refresh_token: None,
                expires_at: now() + 3600,
            },
        )
        .unwrap();
        record
    }
    #[test]
    fn shell_path_resolves_api_and_saved_environment_without_exposing_key() {
        let root = Scratch::new();
        let key = "T".repeat(32);
        write_private(
            &root.0.join("environments.json"),
            &BTreeMap::from([("test-env", &key)]),
        )
        .unwrap();
        let cli = Cli::try_parse_from([
            "honeycomb",
            "--api",
            "http://localhost:8777",
            "--test",
            "test-env",
            "config",
            "env",
        ])
        .unwrap();
        let context = selected_context(&cli, &Settings::default(), &root.0).unwrap();
        assert_eq!(context.testing_key.as_deref(), Some(key.as_str()));
        assert_eq!(context.api, "http://localhost:8777");
        let output = shell_environment(&root.0, &context);
        assert!(
            output.contains(
                &context
                    .directory(&root.0)
                    .join("bin")
                    .to_string_lossy()
                    .to_string()
            )
        );
        assert!(!output.contains(&key));
        let production = InstallContext {
            api: context.api.clone(),
            testing_key: None,
        };
        assert_ne!(context.directory(&root.0), production.directory(&root.0));
        let invalid =
            Cli::try_parse_from(["honeycomb", "--test", "invalid", "config", "env"]).unwrap();
        assert!(selected_context(&invalid, &Settings::default(), &root.0).is_err());
    }
    #[tokio::test]
    async fn daemon_updates_all_saved_contexts_and_preserves_failed_installation() {
        let root = Scratch::new();
        let source = Scratch::new();
        let old = fixture(&source.0, "1.0.0");
        let new = fixture(&source.0, "1.1.0");
        let key = "T".repeat(32);
        let (production_origin, production_server) =
            release_server(&new, false, None, "production-token").await;
        let (testing_origin, testing_server) =
            release_server(&new, true, Some(&key), "testing-token").await;
        let production = InstallContext {
            api: production_origin,
            testing_key: None,
        };
        let testing = InstallContext {
            api: testing_origin,
            testing_key: Some(key.clone()),
        };
        let before_prod = seed(&root.0, &production, &old, "production-token");
        let before_test = seed(&root.0, &testing, &old, "testing-token");
        let registry_path = testing.directory(&root.0).join("installed.json");
        let registry_before = fs::read(&registry_path).unwrap();
        let command_before = fs::read(&before_test.commands["honeycomb-fixture-command"]).unwrap();
        let cli = Cli {
            api: None,
            test: None,
            json: false,
            idempotency_key: None,
            command: Command::Daemon { once: true },
        };
        let error = update_packages(&root.0, &cli, false)
            .await
            .unwrap_err()
            .to_string();
        assert!(!error.contains(&key));
        assert!(!error.contains("token"));
        production_server.await.unwrap();
        testing_server.await.unwrap();
        let after: BTreeMap<String, Installed> =
            read(&production.directory(&root.0).join("installed.json")).unwrap();
        assert_eq!(after["fixture>package"].version, "1.1.0");
        assert_eq!(after["fixture>package"].aliases, before_prod.aliases);
        assert_eq!(fs::read(&registry_path).unwrap(), registry_before);
        assert_eq!(
            fs::read(&before_test.commands["honeycomb-fixture-command"]).unwrap(),
            command_before
        );
        assert!(fs::read_dir(testing.directory(&root.0)).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("download-")
        }));
        // No second request is made within the hourly interval, including for failures.
        update_packages(&root.0, &cli, false).await.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(testing.directory(&root.0).join("context.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[tokio::test]
    async fn disabled_updater_makes_no_requests() {
        let root = Scratch::new();
        write_private(
            &root.0.join("config.json"),
            &Settings {
                auto_update: false,
                ..Settings::default()
            },
        )
        .unwrap();
        let cli = Cli {
            api: Some("http://127.0.0.1:1".into()),
            test: None,
            json: false,
            idempotency_key: None,
            command: Command::Daemon { once: true },
        };
        update_packages(&root.0, &cli, false).await.unwrap();
    }
}
