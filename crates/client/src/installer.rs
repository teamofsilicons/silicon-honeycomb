//! Local package actions. The caller owns paths, installation records and locking.
use anyhow::{Context, Result, bail};
use honeycomb_core::package;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Installed {
    pub app_id: String,
    pub version: String,
    pub target: String,
    pub directory: PathBuf,
    pub commands: BTreeMap<String, PathBuf>,
    pub aliases: BTreeMap<String, String>,
    pub sha256: String,
}
/// Validate, stage and activate all commands. Never overwrite a command belonging to someone else.
/// `previous` is supplied by the caller's installation registry when updating.
pub fn install_archive(
    archive: &Path,
    state_dir: &Path,
    aliases: &BTreeMap<String, String>,
    previous: Option<&Installed>,
) -> Result<Installed> {
    let expected_sha = package::sha256(archive)?;
    fs::create_dir_all(state_dir)?;
    let state_dir = state_dir.canonicalize()?;
    let stage = tempfile::tempdir_in(&state_dir)?;
    let m = package::unpack(archive, stage.path())?;
    let target = package::current_target()?;
    let payload = m
        .targets
        .get(target)
        .with_context(|| format!("{} has no payload for {target}", m.app_id))?;
    if let Some(p) = previous
        && p.app_id != m.app_id
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
        // Include commands already found elsewhere on PATH in collision detection.
        if !owned && let Some(paths) = std::env::var_os("PATH") {
            for p in std::env::split_paths(&paths) {
                let candidate = p.join(command.as_str());
                if candidate.exists() && candidate != destination {
                    bail!(
                        "Command collision: {} is already on PATH. Use --alias {original}=another-name",
                        candidate.display()
                    );
                }
            }
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
    let directory = state_dir
        .join("packages")
        .join(m.app_id.replace('>', "/"))
        .join(format!("{}-{}", m.version, &expected_sha[..12]));
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
    let result = (|| -> Result<()> {
        for (original, (destination, relative)) in &commands {
            let temp = bin.join(format!(".honeycomb-{}", uuid::Uuid::new_v4()));
            let source = directory.join(relative);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&source, &temp)?;
            #[cfg(windows)]
            {
                use std::io::Write;
                let mut f = fs::File::create(&temp)?;
                writeln!(f, "@echo off\r\n\"{}\" %*", source.display())?;
            }
            if destination.symlink_metadata().is_ok() {
                let backup = bin.join(format!(".honeycomb-backup-{}", uuid::Uuid::new_v4()));
                fs::rename(destination, &backup)?;
                backups.push((destination.clone(), backup));
            }
            fs::rename(&temp, destination)
                .with_context(|| format!("Cannot activate command {original}"))?;
            activated.push(destination.clone());
        }
        Ok(())
    })();
    if let Err(e) = result {
        for p in activated {
            let _ = fs::remove_file(p);
        }
        for (p, b) in backups {
            let _ = fs::rename(b, p);
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
        app_id: m.app_id,
        version: m.version,
        target: target.into(),
        directory,
        commands: commands.into_iter().map(|(k, (p, _))| (k, p)).collect(),
        aliases: aliases.clone(),
        sha256: expected_sha,
    })
}
fn owns_command(record: &Installed, path: &Path) -> bool {
    #[cfg(unix)]
    {
        fs::read_link(path).is_ok_and(|p| p.starts_with(&record.directory))
    }
    #[cfg(windows)]
    {
        fs::read_to_string(path)
            .is_ok_and(|s| s.contains(&record.directory.to_string_lossy().to_string()))
    }
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
    if record.directory.exists() {
        fs::remove_dir_all(&record.directory)?;
    }
    Ok(())
}
