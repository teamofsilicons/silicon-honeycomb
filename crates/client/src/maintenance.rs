//! Explicit maintenance actions. Scheduling and preferences belong to the caller.
use anyhow::{Context, Result, bail};
use std::{fs, path::Path, process::Command};

/// Install the newest registry CLI into an owned prefix and replace the caller's executable.
pub async fn update_cli(prefix: &Path, current_executable: &Path) -> Result<()> {
    let output = tokio::process::Command::new("cargo")
        .args(["install", "silicon-honeycomb-cli", "--locked", "--root"])
        .arg(prefix)
        .args(["--bin", "honeycomb"])
        .output()
        .await
        .context("Cargo is unavailable; install Rust or rerun the shell installer to update")?;
    if !output.status.success() {
        bail!(
            "The CLI is not yet published on crates.io or Cargo could not install it; retry after publication"
        );
    }
    let binary = prefix.join("bin").join(if cfg!(windows) {
        "honeycomb.exe"
    } else {
        "honeycomb"
    });
    if binary.canonicalize()? == current_executable.canonicalize()? {
        return Ok(());
    }
    let parent = current_executable
        .parent()
        .context("Cannot find CLI executable directory")?;
    let staging = parent.join(format!(".honeycomb-update-{}", uuid::Uuid::new_v4()));
    fs::copy(&binary, &staging)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))?;
    }
    let version = tokio::process::Command::new(&staging)
        .arg("--version")
        .output()
        .await?;
    if !version.status.success() || !version.stdout.starts_with(b"honeycomb ") {
        let _ = fs::remove_file(staging);
        bail!("Updated executable did not pass version verification");
    }
    #[cfg(windows)]
    {
        let previous = parent.join("honeycomb.previous.exe");
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        fs::rename(current_executable, previous)?;
    }
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
            let dir = home.join("Library/LaunchAgents");
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
            let dir = home.join(".config/systemd/user");
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
            let task = format!("\"{}\" self-update", executable.display());
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
