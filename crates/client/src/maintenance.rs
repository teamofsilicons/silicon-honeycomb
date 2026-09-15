//! Explicit maintenance actions. Scheduling and preferences belong to the caller.
use anyhow::{Context, Result, bail};
use std::{fs, path::Path, process::Command};

/// Fetch the latest vendor release, verify its checksum, and atomically replace the CLI.
/// Prebuilt installs need no Rust toolchain for their hourly updates.
pub async fn update_cli(_prefix: &Path, current_executable: &Path) -> Result<bool> {
    let http = reqwest::Client::builder()
        .user_agent(concat!("honeycomb-updater/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let host = attempt.url().host_str().unwrap_or("");
            if attempt.previous().len() < 5
                && attempt.url().scheme() == "https"
                && (host == "github.com" || host.ends_with(".githubusercontent.com"))
            {
                attempt.follow()
            } else {
                attempt.error("Untrusted release redirect")
            }
        }))
        .build()?;
    let metadata = bounded(
        http.get("https://crates.io/api/v1/crates/silicon-honeycomb-cli")
            .send()
            .await?,
        1024 * 1024,
    )
    .await?;
    let metadata: serde_json::Value = serde_json::from_slice(&metadata)?;
    let latest = metadata["crate"]["max_stable_version"]
        .as_str()
        .or_else(|| metadata["crate"]["max_version"].as_str())
        .context("Registry returned no published version")?;
    let version = semver::Version::parse(latest)?;
    if version <= semver::Version::parse(env!("CARGO_PKG_VERSION"))? {
        return Ok(false);
    }
    if !version.pre.is_empty() {
        bail!("The latest registry version is a prerelease; automatic update deferred");
    }
    let target = crate::package::current_target().context("No prebuilt CLI for this platform")?;
    let asset = format!("honeycomb-{target}.tar.gz");
    let base =
        format!("https://github.com/teamofsilicons/silicon-honeycomb/releases/download/v{version}");
    let checksum = bounded(
        http.get(format!("{base}/{asset}.sha256")).send().await?,
        1024,
    )
    .await?;
    let checksum = std::str::from_utf8(&checksum)?
        .split_whitespace()
        .next()
        .context("Missing release checksum")?;
    let archive = bounded(
        http.get(format!("{base}/{asset}")).send().await?,
        128 * 1024 * 1024,
    )
    .await?;
    install_cli_release(
        &archive,
        checksum,
        &format!("honeycomb {version}"),
        current_executable,
    )
    .await?;
    Ok(true)
}
async fn bounded(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        bail!("Release download returned HTTP {}", response.status());
    }
    if response.content_length().is_some_and(|n| n > limit as u64) {
        bail!("Release response exceeds size limit");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > limit {
            bail!("Release response exceeds size limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
/// Verify a downloaded release without installing any unowned package files.
pub async fn install_cli_release(
    archive: &[u8],
    checksum: &str,
    expected_version: &str,
    current_executable: &Path,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    if checksum.len() != 64 || hex::encode(Sha256::digest(archive)) != checksum.to_lowercase() {
        bail!("CLI release checksum verification failed");
    }
    let parent = current_executable
        .parent()
        .context("Cannot find CLI executable directory")?;
    let staging_dir = tempfile::tempdir_in(parent)?;
    let name = if cfg!(windows) {
        "honeycomb.exe"
    } else {
        "honeycomb"
    };
    let staging = staging_dir.path().join(name);
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    let mut count = 0;
    for entry in tar.entries()? {
        let mut entry = entry?;
        if entry.path()?.as_ref() != Path::new(name)
            || !entry.header().entry_type().is_file()
            || entry.size() > 256 * 1024 * 1024
            || count > 0
        {
            bail!("CLI release must contain exactly one regular executable");
        }
        entry.unpack(&staging)?;
        count += 1;
    }
    if count != 1 {
        bail!("CLI release is empty");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))?;
    }
    let version = tokio::process::Command::new(&staging)
        .arg("--version")
        .output()
        .await?;
    if !version.status.success()
        || String::from_utf8_lossy(&version.stdout).trim() != expected_version
    {
        bail!("Updated CLI did not pass version verification");
    }
    #[cfg(windows)]
    {
        let previous = parent.join("honeycomb.previous.exe");
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        fs::rename(current_executable, &previous)?;
        if let Err(error) = fs::rename(&staging, current_executable) {
            let _ = fs::rename(previous, current_executable);
            return Err(error.into());
        }
    }
    #[cfg(not(windows))]
    fs::rename(staging, current_executable)
        .context("Cannot replace CLI executable; rerun its original installer")?;
    Ok(())
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
/// Render a launchd worker without interpolating unescaped paths into XML.
pub fn launchd_plist(home: &Path, executable: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>com.teamofsilicons.honeycomb.updater</string>
<key>ProgramArguments</key><array><string>{}</string><string>daemon</string></array>
<key>EnvironmentVariables</key><dict><key>SILICON_HOME</key><string>{}</string></dict>
<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
<key>ThrottleInterval</key><integer>60</integer>
</dict></plist>
"#,
        xml(&executable.to_string_lossy()),
        xml(&home.to_string_lossy())
    )
}
fn systemd_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
pub fn systemd_unit(home: &Path, executable: &Path) -> String {
    format!(
        "[Unit]\nDescription=Honeycomb hourly CLI updater\n[Service]\nExecStart=\"{}\" daemon\nEnvironment=\"SILICON_HOME={}\"\nRestart=on-failure\nRestartSec=60\n[Install]\nWantedBy=default.target\n",
        systemd_escape(&executable.to_string_lossy()),
        systemd_escape(&home.to_string_lossy())
    )
}
/// Register the per-user worker. This does not require root privileges.
pub fn install_service(home: &Path, executable: &Path) -> Result<()> {
    match std::env::consts::OS {
        "macos" => {
            let dir = dirs::home_dir()
                .context("Cannot find OS user home")?
                .join("Library/LaunchAgents");
            fs::create_dir_all(&dir)?;
            let path = dir.join("com.teamofsilicons.honeycomb.updater.plist");
            fs::write(&path, launchd_plist(home, executable))?;
            let uid = Command::new("id").arg("-u").output()?;
            let uid = String::from_utf8(uid.stdout)?.trim().to_owned();
            let domain = format!("gui/{uid}");
            let _ = Command::new("launchctl")
                .args(["bootout", &domain])
                .arg(&path)
                .output();
            let result = Command::new("launchctl")
                .args(["bootstrap", &domain])
                .arg(&path)
                .output()?;
            if !result.status.success() {
                bail!(
                    "CLI installed, but launchd could not start its worker: {}. Run honeycomb daemon in a persistent session.",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
        "linux" => {
            let dir = dirs::config_dir()
                .context("Cannot find OS configuration directory")?
                .join("systemd/user");
            fs::create_dir_all(&dir)?;
            fs::write(
                dir.join("honeycomb-updater.service"),
                systemd_unit(home, executable),
            )?;
            for args in [
                vec!["--user", "daemon-reload"],
                vec!["--user", "enable", "--now", "honeycomb-updater.service"],
            ] {
                let status = Command::new("systemctl").args(args).status().context(
                    "No user systemd session; run honeycomb daemon under your process supervisor",
                )?;
                if !status.success() {
                    bail!(
                        "Cannot start the user update worker; run honeycomb daemon under your process supervisor"
                    );
                }
            }
        }
        "windows" => {
            let directory = home.join(".honeycomb/dir/system");
            fs::create_dir_all(&directory)?;
            let script = directory.join("update.cmd");
            let escape = |path: &Path| -> Result<String> {
                let value = path.to_string_lossy();
                if value.contains(['\r', '\n', '"']) {
                    bail!("Invalid updater path");
                }
                Ok(value.replace('%', "%%"))
            };
            fs::write(
                &script,
                format!(
                    "@echo off\r\nset \"SILICON_HOME={}\"\r\n\"{}\" daemon --once\r\n",
                    escape(home)?,
                    escape(executable)?
                ),
            )?;
            let task = format!("cmd.exe /d /c \"{}\"", script.display());
            let status = Command::new("schtasks")
                .args([
                    "/Create",
                    "/F",
                    "/SC",
                    "HOURLY",
                    "/TN",
                    "HoneycombUpdater",
                    "/TR",
                    &task,
                ])
                .status()?;
            if !status.success() {
                bail!("Could not register HoneycombUpdater scheduled task");
            }
        }
        _ => bail!("Install honeycomb daemon with your operating system's process supervisor"),
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_paths_are_escaped() {
        let xml = launchd_plist(Path::new("/tmp/a&b"), Path::new("/tmp/a<b/honeycomb"));
        assert!(xml.contains("a&amp;b"));
        assert!(xml.contains("a&lt;b"));
        let unit = systemd_unit(
            Path::new("/tmp/100%"),
            Path::new("/tmp/space dir/honeycomb"),
        );
        assert!(unit.contains("100%%"));
        assert!(unit.contains("ExecStart=\"/tmp/space dir/honeycomb\" daemon"));
    }
}
