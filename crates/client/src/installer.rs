//! Local package actions. The caller owns paths, installation records and locking.
use anyhow::{Context, Result, bail};
use honeycomb_core::{ReleaseChannel, package};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Installed {
    pub app_id: String,
    #[serde(default)]
    pub channel: ReleaseChannel,
    pub version: String,
    pub target: String,
    pub directory: PathBuf,
    pub commands: BTreeMap<String, PathBuf>,
    pub aliases: BTreeMap<String, String>,
    pub sha256: String,
    /// Commands another installation already provided at a version this package accepts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved: Vec<PathConflict>,
    /// Commands rewritten outside this bin directory, with the file kept for uninstall.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rewritten: BTreeMap<String, Rewritten>,
}

/// A command of the same name already reachable on PATH from another installation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PathConflict {
    pub command: String,
    pub original: String,
    pub existing: PathBuf,
    /// `None` when the existing command does not report a version we can read.
    pub existing_version: Option<String>,
    pub required_version: String,
    /// True when the version already present is new enough for this package.
    pub satisfied: bool,
}

/// A command replaced outside this installation's bin directory, and the file it displaced.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rewritten {
    pub path: PathBuf,
    pub backup: PathBuf,
}

/// Commands on PATH that are older than this package needs. The caller decides whether to
/// rewrite them, so this stays a distinct error rather than a plain collision failure.
#[derive(Debug)]
pub struct UnresolvedCommands(pub Vec<PathConflict>);

impl std::fmt::Display for UnresolvedCommands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, c) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            match &c.existing_version {
                Some(v) => write!(
                    f,
                    "You have {} {v} on your system at {}; this package needs {} {} to continue",
                    c.command,
                    c.existing.display(),
                    c.command,
                    c.required_version
                )?,
                None => write!(
                    f,
                    "You have {} on your system at {} and its version is unreadable; this package needs {} {} to continue",
                    c.command,
                    c.existing.display(),
                    c.command,
                    c.required_version
                )?,
            }
        }
        Ok(())
    }
}
impl std::error::Error for UnresolvedCommands {}
/// Validate, stage and activate all commands. Never overwrite a command belonging to someone else.
/// `previous` is supplied by the caller's installation registry when updating.
pub fn install_archive(
    archive: &Path,
    state_dir: &Path,
    aliases: &BTreeMap<String, String>,
    previous: Option<&Installed>,
) -> Result<Installed> {
    install_archive_with_identity(archive, state_dir, aliases, previous, None, false)
}

/// Install using the application selected by the caller's verified release record.
/// An optional manifest ID must match this identity; updates must retain ownership.
pub fn install_archive_for(
    app_id: &str,
    archive: &Path,
    state_dir: &Path,
    aliases: &BTreeMap<String, String>,
    previous: Option<&Installed>,
) -> Result<Installed> {
    install_archive_with_identity(
        archive,
        state_dir,
        aliases,
        previous,
        Some((app_id, ReleaseChannel::Prod)),
        false,
    )
}

/// Install after the caller has answered an [`UnresolvedCommands`] error. With `rewrite`,
/// commands older than this package needs are replaced where they sit and the displaced
/// files are kept so uninstall can put them back.
pub fn install_archive_resolving(
    app_id: &str,
    archive: &Path,
    state_dir: &Path,
    aliases: &BTreeMap<String, String>,
    previous: Option<&Installed>,
    rewrite: bool,
) -> Result<Installed> {
    install_archive_channel_resolving(
        app_id,
        ReleaseChannel::Prod,
        archive,
        state_dir,
        aliases,
        previous,
        rewrite,
    )
}

/// Install the selected release track while keeping the manifest's base app identity.
pub fn install_archive_channel_resolving(
    app_id: &str,
    channel: ReleaseChannel,
    archive: &Path,
    state_dir: &Path,
    aliases: &BTreeMap<String, String>,
    previous: Option<&Installed>,
    rewrite: bool,
) -> Result<Installed> {
    install_archive_with_identity(
        archive,
        state_dir,
        aliases,
        previous,
        Some((app_id, channel)),
        rewrite,
    )
}

fn install_archive_with_identity(
    archive: &Path,
    state_dir: &Path,
    aliases: &BTreeMap<String, String>,
    previous: Option<&Installed>,
    requested: Option<(&str, ReleaseChannel)>,
    rewrite: bool,
) -> Result<Installed> {
    let expected_sha = package::sha256(archive)?;
    fs::create_dir_all(state_dir)?;
    let state_dir = state_dir.canonicalize()?;
    let stage = tempfile::tempdir_in(&state_dir)?;
    let m = package::unpack(archive, stage.path())?;
    let channel = requested.map(|(_, channel)| channel).unwrap_or_default();
    let app_id = requested
        .map(|(id, _)| id)
        .or(m.app_id.as_deref())
        .or_else(|| previous.map(|p| p.app_id.as_str()))
        .context(
            "Archive omits app_id; use install_archive_for with the selected application ID",
        )?;
    if !honeycomb_core::valid_app_id(app_id) {
        bail!("Invalid installation application ID");
    }
    if m.app_id.as_deref().is_some_and(|id| id != app_id) {
        bail!("Archive app_id differs from the selected application");
    }
    let target = package::current_target()?;
    let payload = m
        .targets
        .get(target)
        .with_context(|| format!("{app_id} has no payload for {target}"))?;
    if let Some(p) = previous
        && p.app_id != app_id
    {
        bail!("The existing installation belongs to a different application");
    }
    for name in aliases.keys() {
        if !m.bin.contains_key(name) {
            bail!(
                "Unknown command {name} in --alias; available commands: {}",
                m.bin.keys().cloned().collect::<Vec<_>>().join(", ")
            );
        }
    }
    let bin = state_dir.join("bin");
    fs::create_dir_all(&bin)?;
    let mut commands = BTreeMap::new();
    let mut destinations = std::collections::BTreeSet::new();
    let mut conflicts: Vec<PathConflict> = vec![];
    let required = Version::parse(&m.version).ok();
    for (original, executable) in &m.bin {
        let command = aliases.get(original).unwrap_or(original);
        if !package::safe_command(command) || !destinations.insert(command.to_lowercase()) {
            bail!("Alias {command} is unsafe or collides with another command");
        }
        #[cfg(windows)]
        let command = format!("{command}.cmd");
        let destination = bin.join(command.as_str());
        let owned = previous
            .and_then(|p| p.commands.get(original))
            .is_some_and(|p| p == &destination && owns_command(previous.unwrap(), p));
        if destination.symlink_metadata().is_ok() && !owned {
            bail!(
                "Command collision: {} exists. Use --alias {original}=another-name",
                destination.display()
            );
        }
        // A command of this name elsewhere on PATH is a dependency question rather than an
        // ownership conflict: record which version is already there and decide after the loop.
        if !owned && let Some(existing) = first_on_path(command.as_str(), &destination) {
            let found = existing_version(&existing);
            conflicts.push(PathConflict {
                command: command.to_string(),
                original: original.clone(),
                existing,
                satisfied: match (&found, &required) {
                    (Some(have), Some(need)) => have >= need,
                    _ => false,
                },
                existing_version: found.map(|v| v.to_string()),
                required_version: m.version.clone(),
            });
        }
        commands.insert(
            original.clone(),
            (
                destination,
                payload
                    .executables
                    .get(executable)
                    .context("Validated executable mapping missing")?
                    .clone(),
            ),
        );
    }
    // Anything already new enough is reported as resolved; the rest is the caller's call.
    let unresolved: Vec<PathConflict> =
        conflicts.iter().filter(|c| !c.satisfied).cloned().collect();
    if !unresolved.is_empty() && !rewrite {
        return Err(UnresolvedCommands(unresolved).into());
    }
    let application_directory = state_dir.join("packages").join(app_id.replace('>', "/"));
    let application_directory = if channel == ReleaseChannel::Dev {
        application_directory.join("dev")
    } else {
        application_directory
    };
    let directory = application_directory.join(format!("{}-{}", m.version, &expected_sha[..12]));
    if directory.exists() {
        bail!(
            "This exact package is already staged at {}; uninstall it before reinstalling",
            directory.display()
        );
    }
    fs::create_dir_all(directory.parent().context("Missing package parent")?)?;
    fs::rename(stage.path().join(&payload.root), &directory)?;
    let mut activated = vec![];
    let mut backups = vec![];
    let mut restored = vec![];
    let mut rewritten: BTreeMap<String, Rewritten> = BTreeMap::new();
    let result = (|| -> Result<()> {
        for (original, (destination, relative)) in &commands {
            let source = directory.join(relative);
            if destination.symlink_metadata().is_ok() {
                let backup = bin.join(format!(".honeycomb-backup-{}", uuid::Uuid::new_v4()));
                fs::rename(destination, &backup)?;
                backups.push((destination.clone(), backup));
            }
            write_launcher(&source, destination)
                .with_context(|| format!("Cannot activate command {original}"))?;
            activated.push(destination.clone());
        }
        // Keep approved external rewrites pointed at the new payload across updates and
        // channel switches, retaining the original displaced file for eventual uninstall.
        if let Some(old) = previous {
            for (original, replacement) in &old.rewritten {
                if let Some((_, relative)) = commands.get(original)
                    && owns_command(old, &replacement.path)
                {
                    let backup = replacement
                        .path
                        .with_file_name(format!(".honeycomb-backup-{}", uuid::Uuid::new_v4()));
                    fs::rename(&replacement.path, &backup)?;
                    backups.push((replacement.path.clone(), backup));
                    write_launcher(&directory.join(relative), &replacement.path)?;
                    activated.push(replacement.path.clone());
                    rewritten.insert(original.clone(), replacement.clone());
                } else if !commands.contains_key(original) && owns_command(old, &replacement.path) {
                    let backup = replacement
                        .path
                        .with_file_name(format!(".honeycomb-backup-{}", uuid::Uuid::new_v4()));
                    fs::rename(&replacement.path, &backup)?;
                    backups.push((replacement.path.clone(), backup));
                    fs::rename(&replacement.backup, &replacement.path)?;
                    restored.push((replacement.path.clone(), replacement.backup.clone()));
                }
            }
        }
        // Answered rewrites replace the older command where it already sits, keeping the
        // file it displaced so uninstall can restore the system to its previous state.
        for conflict in conflicts.iter().filter(|c| !c.satisfied) {
            let Some((_, relative)) = commands.get(&conflict.original) else {
                continue;
            };
            let source = directory.join(relative);
            let backup = conflict
                .existing
                .with_file_name(format!(".honeycomb-replaced-{}", uuid::Uuid::new_v4()));
            fs::rename(&conflict.existing, &backup)
                .with_context(|| format!("Cannot set aside {}", conflict.existing.display()))?;
            rewritten.insert(
                conflict.original.clone(),
                Rewritten {
                    path: conflict.existing.clone(),
                    backup: backup.clone(),
                },
            );
            write_launcher(&source, &conflict.existing)
                .with_context(|| format!("Cannot rewrite {}", conflict.existing.display()))?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        for (path, original) in restored {
            let _ = fs::rename(path, original);
        }
        for p in activated {
            let _ = fs::remove_file(p);
        }
        for (p, b) in backups {
            let _ = fs::rename(b, p);
        }
        for r in rewritten.values().filter(|r| {
            previous.is_none_or(|old| !old.rewritten.values().any(|prior| prior.backup == r.backup))
        }) {
            let _ = fs::remove_file(&r.path);
            let _ = fs::rename(&r.backup, &r.path);
        }
        let _ = fs::remove_dir_all(&directory);
        return Err(e);
    }
    for (_, backup) in backups {
        fs::remove_file(backup)?;
    }
    if let Some(old) = previous {
        for (original, path) in &old.commands {
            if commands.get(original).is_none_or(|(new, _)| new != path) && owns_command(old, path)
            {
                fs::remove_file(path)?;
            }
        }
        if old.directory != directory && old.directory.starts_with(state_dir.join("packages")) {
            fs::remove_dir_all(&old.directory)?;
        }
    }
    Ok(Installed {
        app_id: app_id.to_owned(),
        channel,
        version: m.version,
        target: target.into(),
        directory,
        commands: commands.into_iter().map(|(k, (p, _))| (k, p)).collect(),
        aliases: aliases.clone(),
        sha256: expected_sha,
        resolved: conflicts.into_iter().filter(|c| c.satisfied).collect(),
        rewritten,
    })
}

/// Point a command name at a package executable, replacing it in one rename.
fn write_launcher(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("Command destination has no directory")?;
    let temp = parent.join(format!(".honeycomb-{}", uuid::Uuid::new_v4()));
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, &temp)?;
    #[cfg(windows)]
    {
        use std::io::Write;
        let mut f = fs::File::create(&temp)?;
        // canonicalize() returns verbatim paths on Windows. cmd.exe
        // cannot launch batch files through the \\?\ namespace.
        let source = batch_path(&source.to_string_lossy());
        writeln!(
            f,
            "@echo off\r\nsetlocal DisableDelayedExpansion\r\n\"{source}\" %*"
        )?;
    }
    fs::rename(&temp, destination)?;
    Ok(())
}

/// The first command of this name on PATH, ignoring the launcher we are about to write.
fn first_on_path(command: &str, destination: &Path) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|p| p.join(command))
        .find(|candidate| candidate.exists() && candidate != destination)
}

/// Read the version already serving a command. A Honeycomb launcher answers from its own
/// package path; anything else is asked directly, which is the only way an unmanaged
/// binary can report itself.
fn existing_version(path: &Path) -> Option<Version> {
    launcher_version(path).or_else(|| probe_version(path))
}

fn launcher_version(path: &Path) -> Option<Version> {
    let target = fs::canonicalize(path).ok()?;
    let parts: Vec<String> = target
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    // <state>/packages/<org>/<app>/<version>-<sha12>/<executable>
    let packages = parts.iter().rposition(|p| p == "packages")?;
    let offset = if parts.get(packages + 3)? == "dev" {
        4
    } else {
        3
    };
    let (version, _) = parts.get(packages + offset)?.rsplit_once('-')?;
    Version::parse(version).ok()
}

fn probe_version(path: &Path) -> Option<Version> {
    let mut child = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // An unmanaged command is not required to be well behaved; never wait on it forever.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .find_map(|token| Version::parse(token.trim_start_matches('v')).ok())
}
fn owns_command(record: &Installed, path: &Path) -> bool {
    #[cfg(unix)]
    {
        fs::read_link(path).is_ok_and(|p| p.starts_with(&record.directory))
    }
    #[cfg(windows)]
    {
        fs::read_to_string(path)
            .is_ok_and(|s| s.contains(&batch_path(&record.directory.to_string_lossy())))
    }
}

#[cfg(any(windows, test))]
fn batch_path(path: &str) -> String {
    let ordinary = if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_owned()
    };
    // Percent expansion still runs inside double quotes in a batch file.
    ordinary.replace('%', "%%")
}

/// Remove only launchers still pointing to this package; preserve commands replaced by the user.
pub fn uninstall(record: &Installed, state_dir: &Path) -> Result<()> {
    let root = state_dir.canonicalize()?;
    if !record.directory.starts_with(root.join("packages")) {
        bail!("Installation registry points outside the Honeycomb package directory");
    }
    for path in record.commands.values() {
        if path.parent() != Some(root.join("bin").as_path()) {
            bail!("Installation command points outside Honeycomb's bin directory");
        }
        if owns_command(record, path) {
            fs::remove_file(path)?;
        }
    }
    // Put back whatever a rewrite displaced, but never over a command the user has since
    // replaced themselves.
    for r in record.rewritten.values() {
        if owns_command(record, &r.path) {
            fs::remove_file(&r.path)?;
        }
        if r.path.symlink_metadata().is_err() && r.backup.exists() {
            fs::rename(&r.backup, &r.path)?;
        }
    }
    if record.directory.exists() {
        fs::remove_dir_all(&record.directory)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn batch_paths_support_canonical_drive_unc_and_literal_percent() {
        assert_eq!(
            super::batch_path(r"\\?\C:\my packages\100%\hello.cmd"),
            r"C:\my packages\100%%\hello.cmd"
        );
        assert_eq!(
            super::batch_path(r"\\?\UNC\server\share\hello.cmd"),
            r"\\server\share\hello.cmd"
        );
        assert_eq!(
            super::batch_path(r"C:\packages\hello.exe"),
            r"C:\packages\hello.exe"
        );
    }
}
