//! Persist app command discovery after a successful foreground installation.
#[cfg(not(windows))]
use anyhow::Context;
use anyhow::Result;
#[cfg(not(windows))]
use fs2::FileExt;
use serde::Serialize;
use std::path::Path;
#[cfg(not(windows))]
use std::{fs, io::Write};

#[derive(Serialize)]
pub struct Setup {
    pub status: &'static str,
    pub message: String,
    pub activation_command: Option<String>,
}

pub fn configure(bin: &Path, persistent: bool) -> Setup {
    let activation = activation(bin);
    let reachable = std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|path| {
            path == bin
                || path
                    .canonicalize()
                    .ok()
                    .zip(bin.canonicalize().ok())
                    .is_some_and(|(a, b)| a == b)
        })
    });
    let skipped = if !persistent {
        Some("Testing environments require explicit shell activation.")
    } else if std::env::var("HONEYCOMB_NO_MODIFY_PATH").as_deref() == Ok("1") {
        Some("Automatic PATH setup is disabled by HONEYCOMB_NO_MODIFY_PATH=1.")
    } else {
        None
    };
    let (status, mut message) = if let Some(reason) = skipped {
        ("skipped", reason.to_owned())
    } else {
        match persist(bin) {
            Ok(()) => (
                "configured",
                "Added the app command directory to PATH for future terminals.".to_owned(),
            ),
            Err(error) => (
                "manual",
                format!("The app is installed, but automatic PATH setup failed: {error:#}."),
            ),
        }
    };
    if !reachable {
        message.push_str(&format!(
            "\nActivate it in this terminal with:\n  {activation}"
        ));
    }
    Setup {
        status,
        message,
        activation_command: (!reachable).then_some(activation),
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn shell() -> String {
    std::env::var_os("SHELL")
        .and_then(|p| {
            Path::new(&p)
                .file_name()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "sh".to_owned())
}

fn activation(bin: &Path) -> String {
    let value = bin.to_string_lossy();
    if cfg!(windows) {
        format!(
            "$env:Path = '{}' + ';' + $env:Path",
            value.replace('\'', "''")
        )
    } else if shell() == "fish" {
        format!("fish_add_path --path {}", fish_quote(&value))
    } else {
        format!("export PATH={}:\"$PATH\"", quote(&value))
    }
}

fn fish_quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(not(windows))]
fn persist(bin: &Path) -> Result<()> {
    let home = crate::base_home()?;
    // SILICON_HOME is also the shell-configuration boundary for embedded runtimes.
    let isolated = std::env::var_os("SILICON_HOME").is_some();
    let scoped = |key: &str| {
        std::env::var_os(key)
            .map(std::path::PathBuf::from)
            .filter(|path| !isolated || path.starts_with(&home))
    };
    let shell = shell();
    let files = match shell.as_str() {
        "zsh" => vec![
            scoped("ZDOTDIR")
                .unwrap_or_else(|| home.clone())
                .join(".zshrc"),
        ],
        "bash" => {
            let login = [".bash_profile", ".bash_login", ".profile"]
                .into_iter()
                .find(|name| home.join(name).exists())
                .unwrap_or(".profile");
            vec![home.join(".bashrc"), home.join(login)]
        }
        "sh" | "dash" | "ksh" => vec![home.join(".profile")],
        "fish" => vec![
            scoped("XDG_CONFIG_HOME")
                .unwrap_or_else(|| home.join(".config"))
                .join("fish/conf.d/honeycomb.fish"),
        ],
        _ => anyhow::bail!(
            "unsupported shell {shell}; add the printed activation command to its startup file"
        ),
    };
    let root = bin
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("App command directory has no Honeycomb root")?;
    // Different contexts can install concurrently; serialize shared startup-file edits.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(home.join(".honeycomb/shell.lock"))?;
    lock.lock_exclusive()?;
    let env = root.join(if shell == "fish" {
        "app-env.fish"
    } else {
        "app-env"
    });
    let body = if shell == "fish" {
        format!("{}\n", activation(bin))
    } else {
        let value = quote(&bin.to_string_lossy());
        format!(
            "case \":$PATH:\" in\n  *:{value}:*) ;;\n  *) export PATH={value}:\"$PATH\" ;;\nesac\n"
        )
    };
    let temp = root.join(format!(".app-env-{}", uuid::Uuid::new_v4()));
    fs::write(&temp, body)?;
    if let Err(error) = fs::rename(&temp, &env) {
        let _ = fs::remove_file(&temp);
        return Err(error.into());
    }
    let line = if shell == "fish" {
        format!(
            "test ! -f {0}; or source {0}",
            fish_quote(&env.to_string_lossy())
        )
    } else {
        format!("[ ! -f {0} ] || . {0}", quote(&env.to_string_lossy()))
    };
    for file in files {
        append_once(&file, &line)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn append_once(path: &Path, line: &str) -> Result<()> {
    let existing = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).with_context(|| format!("Cannot read {}", path.display())),
    };
    if !existing.lines().any(|value| value == line) {
        fs::create_dir_all(path.parent().context("Startup file has no parent")?)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("Cannot update {}", path.display()))?;
        writeln!(file, "\n# Honeycomb app commands\n{line}")?;
    }
    Ok(())
}

#[cfg(windows)]
fn persist(bin: &Path) -> Result<()> {
    // An embedded runtime must not register its private context in the user's global PATH.
    anyhow::ensure!(
        std::env::var_os("SILICON_HOME").is_none(),
        "SILICON_HOME is isolated; activate its commands explicitly on Windows"
    );
    let script = r#"$ErrorActionPreference = 'Stop'
$bin = $env:HONEYCOMB_PATH_ENTRY
$path = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($path -split ';') -notcontains $bin) {
    [Environment]::SetEnvironmentVariable('Path', ($bin + ';' + $path), 'User')
    Add-Type -Namespace Honeycomb -Name Native -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern System.IntPtr SendMessageTimeout(System.IntPtr hwnd, uint msg, System.UIntPtr wparam, string lparam, uint flags, uint timeout, out System.UIntPtr result);
'@
    $result = [UIntPtr]::Zero
    [void][Honeycomb.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1a, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
}"#;
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("HONEYCOMB_PATH_ENTRY", bin)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "Cannot update user PATH: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
