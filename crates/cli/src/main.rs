use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use fs2::FileExt;
use honeycomb_client::{
    AppInput, Client, Mutation,
    installer::{self, Installed},
    package,
};
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
enum Apps {
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
                let fingerprint = honeycomb_client::context_fingerprint(&settings.api, None);
                let path = format!(
                    "{}:{}",
                    root.join("system/bin").display(),
                    root.join("contexts")
                        .join(fingerprint)
                        .join("bin")
                        .display()
                );
                let quoted = path.replace('\'', "'\\''");
                println!("export PATH='{quoted}':\"$PATH\"");
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
        return self_update(&root, true).await;
    }
    if let Command::Daemon { once } = cli.command {
        loop {
            let config: Settings = read(&root.join("config.json"))?;
            if config.auto_update
                && let Err(e) = self_update(&root, false).await
            {
                eprintln!("Honeycomb update check: {e}");
            }
            if once {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    }
    let mut client = Client::new(cli.api.as_deref().unwrap_or(&settings.api))?;
    let origin = cli.api.as_deref().unwrap_or(&settings.api);
    let selected_key = if let Some(test) = &cli.test {
        let keys: BTreeMap<String, String> = read(&root.join("environments.json"))?;
        Some(keys.get(test).cloned().unwrap_or_else(|| test.clone()))
    } else {
        None
    };
    if let Some(key) = &selected_key {
        client = client.with_environment(key)?;
    }
    let plane = honeycomb_client::context_fingerprint(origin, selected_key.as_deref());
    let plane_dir = root.join("contexts").join(&plane);
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
        let tokens = client.refresh(refresh, &mutation(&cli, None)?).await?;
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
    let operation = mutation(&cli, None)?;
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
                            &mutation(&cli, Some(*revision))?,
                        )
                        .await?,
                )?;
            }
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
                        &mutation(&cli, Some(*revision))?,
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
                    .upload_release(app_id, archive, &mutation(&cli, Some(*revision))?)
                    .await?,
            )?,
        },
        Command::Publication { command } => match command {
            Publication::Get { app_id } => show(&client.publication(app_id).await?)?,
            Publication::Request {
                app_id,
                message,
                revision,
            } => show(
                &client
                    .request_publication(app_id, message, &mutation(&cli, Some(*revision))?)
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
                        &mutation(&cli, Some(*revision))?,
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
                        &mutation(&cli, Some(*revision))?,
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
                    .environment_action(id, action, &mutation(&cli, Some(*revision))?)
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
            )
            .await?
        }
        Command::Update { app_id } => {
            install(&client, &plane_dir, app_id, None, None, true).await?
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
        Command::Config { .. }
        | Command::Daemon { .. }
        | Command::SelfUpdate
        | Command::Service { .. } => unreachable!(),
    }
    // Maintenance failures must not change the user's successful command result.
    if settings.auto_update
        && now() - settings.last_update_check >= 3600
        && let Err(e) = self_update(&root, false).await
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
    lock.lock_exclusive()?;
    let mut records: BTreeMap<String, Installed> = read(&root.join("installed.json"))?;
    let previous = records.get(id);
    if updating && previous.is_none() {
        bail!("{id} is not installed; run honeycomb install '{id}' first");
    }
    let releases = client.releases(id).await?;
    let release = releases
        .iter()
        .find(|r| version.is_none_or(|v| v == r.version))
        .context("Requested release is unavailable")?;
    if previous.is_some_and(|p| p.version == release.version && p.sha256 == release.sha256) {
        show(&json!({"app_id":id,"version":release.version,"status":"already_up_to_date"}))?;
        return Ok(());
    }
    let aliases = aliases
        .or_else(|| previous.map(|p| p.aliases.clone()))
        .unwrap_or_default();
    let stage = root.join(format!("download-{}.tar.gz", uuid::Uuid::new_v4()));
    client.download(id, release, &stage).await?;
    let result = installer::install_archive(&stage, root, &aliases, previous);
    let _ = fs::remove_file(&stage);
    let record = result?;
    let first = record
        .commands
        .values()
        .next()
        .context("No commands installed")?
        .clone();
    records.insert(id.into(), record.clone());
    write_private(&root.join("installed.json"), &records)?;
    show(
        &json!({"app_id":id,"version":record.version,"status":"installed","bin_directory":root.join("bin"),"help_command":format!("{} --help",first.display())}),
    )?;
    Ok(())
}
async fn self_update(root: &Path, force: bool) -> Result<()> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("update.lock"))?;
    if lock.try_lock_exclusive().is_err() {
        return Ok(());
    }
    let mut settings: Settings = read(&root.join("config.json"))?;
    if !force && now() - settings.last_update_check < 3600 {
        return Ok(());
    }
    settings.last_update_check = now();
    write_private(&root.join("config.json"), &settings)?;
    let executable = std::env::current_exe()?;
    let updated =
        honeycomb_client::maintenance::update_cli(&root.join("system"), &executable).await?;
    if force {
        show(&json!({"updated":updated,"executable":executable}))?;
    }
    Ok(())
}
