//! Strict portable archives. Reject links, traversal, devices and decompression bombs before extraction.
use anyhow::{Context, Result, bail};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub const REQUIRED_TARGETS: [&str; 6] = [
    "linux-x86_64",
    "linux-aarch64",
    "windows-x86_64",
    "windows-aarch64",
    "macos-x86_64",
    "macos-aarch64",
];
pub const OPTIONAL_TARGETS: [&str; 3] = ["linux-i686", "linux-armv7hf", "windows-i686"];
pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_EXPANDED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 50_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub app_id: String,
    pub version: String,
    pub bin: BTreeMap<String, String>,
    pub targets: BTreeMap<String, Target>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub root: String,
    pub executables: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Validation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub manifest: Option<Manifest>,
}

pub fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
        && !path.chars().any(char::is_control)
        && !path.starts_with('/')
        && path.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && !p.ends_with([' ', '.'])
                && !windows_reserved(p)
        })
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
fn windows_reserved(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul" | "clock$")
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}
pub fn safe_command(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !s.ends_with('.')
        && !windows_reserved(s)
}

pub fn validate_dir(root: &Path) -> Validation {
    let mut errors = vec![];
    let manifest_path = root.join("honeycomb.yaml");
    if fs::symlink_metadata(&manifest_path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Validation {
            valid: false,
            errors: vec!["honeycomb.yaml must not be a symbolic link".into()],
            manifest: None,
        };
    }
    let bytes = match fs::File::open(&manifest_path).and_then(|file| {
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        Ok(bytes)
    }) {
        Ok(b) if b.len() <= 1024 * 1024 => b,
        Ok(_) => {
            return Validation {
                valid: false,
                errors: vec!["honeycomb.yaml exceeds 1 MiB".into()],
                manifest: None,
            };
        }
        Err(e) => {
            return Validation {
                valid: false,
                errors: vec![format!("honeycomb.yaml: {e}")],
                manifest: None,
            };
        }
    };
    let m: Manifest = match serde_yaml::from_slice(&bytes) {
        Ok(m) => m,
        Err(e) => {
            return Validation {
                valid: false,
                errors: vec![format!("honeycomb.yaml: {e}")],
                manifest: None,
            };
        }
    };
    if m.format_version != 1 {
        errors.push("format_version: only version 1 is supported".into());
    }
    if !crate::valid_app_id(&m.app_id) {
        errors.push("app_id: expected org>app using lowercase handles".into());
    }
    if semver::Version::parse(&m.version).is_err() {
        errors.push("version: expected a semantic version such as 1.0.0".into());
    }
    if m.bin.is_empty() {
        errors.push("bin: declare at least one command".into());
    }
    let mut commands = BTreeSet::new();
    for (command, executable) in &m.bin {
        if !safe_command(command) || !commands.insert(command.to_lowercase()) {
            errors.push(format!(
                "bin.{command}: unsafe or case-colliding command name"
            ));
        }
        if !safe_command(executable) {
            errors.push(format!("bin.{command}: invalid executable identifier"));
        }
    }
    for target in REQUIRED_TARGETS {
        if !m.targets.contains_key(target) {
            errors.push(format!(
                "targets.{target}: required 64-bit target is missing"
            ));
        }
    }
    let mut roots: Vec<&str> = vec![];
    for (id, t) in &m.targets {
        if !REQUIRED_TARGETS.contains(&id.as_str()) && !OPTIONAL_TARGETS.contains(&id.as_str()) {
            errors.push(format!("targets.{id}: unsupported target"));
        }
        if !safe_relative(&t.root) || !t.root.starts_with("targets/") {
            errors.push(format!(
                "targets.{id}.root: expected a safe path below targets/"
            ));
            continue;
        }
        let mut ancestor = root.to_path_buf();
        let has_link = t.root.split('/').any(|segment| {
            ancestor.push(segment);
            fs::symlink_metadata(&ancestor).is_ok_and(|m| m.file_type().is_symlink())
        });
        if has_link {
            errors.push(format!(
                "targets.{id}.root: symbolic links in target paths are forbidden"
            ));
            continue;
        }
        if roots.iter().any(|r| {
            *r == t.root
                || t.root.starts_with(&format!("{r}/"))
                || r.starts_with(&format!("{}/", t.root))
        }) {
            errors.push(format!("targets.{id}.root: target roots must not overlap"));
        }
        roots.push(&t.root);
        for exec in m.bin.values() {
            if !t.executables.contains_key(exec) {
                errors.push(format!(
                    "targets.{id}.executables.{exec}: command mapping is missing"
                ));
            }
        }
        for (exec, relative) in &t.executables {
            if !safe_command(exec) || !safe_relative(relative) {
                errors.push(format!(
                    "targets.{id}.executables.{exec}: unsafe executable path"
                ));
                continue;
            }
            let path = root.join(&t.root).join(relative);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if !id.starts_with("windows") && metadata.permissions().mode() & 0o111 == 0
                        {
                            errors
                                .push(format!("{}: not executable; run chmod +x", path.display()));
                        }
                    }
                    if metadata.len() == 0 {
                        errors.push(format!("{}: executable is empty", path.display()));
                    }
                }
                _ => errors.push(format!(
                    "targets.{id}.executables.{exec}: {} is not a regular file",
                    path.display()
                )),
            }
        }
        for entry in walkdir::WalkDir::new(root.join(&t.root)).follow_links(false) {
            match entry {
                Ok(e)
                    if e.file_type().is_symlink()
                        || (!e.file_type().is_file() && !e.file_type().is_dir()) =>
                {
                    errors.push(format!(
                        "{}: links and special files are forbidden",
                        e.path().display()
                    ))
                }
                Err(e) => errors.push(e.to_string()),
                _ => {}
            }
        }
    }
    Validation {
        valid: errors.is_empty(),
        errors,
        manifest: Some(m),
    }
}

pub fn unpack(archive: &Path, destination: &Path) -> Result<Manifest> {
    if fs::metadata(archive)?.len() > MAX_ARCHIVE_BYTES {
        bail!("archive exceeds 512 MiB compressed limit");
    }
    if fs::symlink_metadata(destination).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("destination must not be a symbolic link");
    }
    fs::create_dir_all(destination)?;
    let mut tar = tar::Archive::new(GzDecoder::new(fs::File::open(archive)?));
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for (i, entry) in tar.entries()?.enumerate() {
        if i >= MAX_ENTRIES {
            bail!("archive exceeds {MAX_ENTRIES} entries");
        }
        let mut entry = entry?;
        let name = entry
            .path()?
            .to_str()
            .context("archive paths must be UTF-8")?
            .trim_end_matches('/')
            .to_string();
        if !safe_relative(&name)
            || (name != "honeycomb.yaml" && name != "targets" && !name.starts_with("targets/"))
        {
            bail!("unsafe or unexpected archive path: {name}");
        }
        if !seen.insert(name.to_lowercase()) {
            bail!("duplicate or case-colliding archive path: {name}");
        }
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            bail!("{name}: links and special files are forbidden");
        }
        total = total
            .checked_add(entry.size())
            .context("archive size overflow")?;
        if total > MAX_EXPANDED_BYTES {
            bail!("archive exceeds 2 GiB expanded limit");
        }
        let path = destination.join(&name);
        // Never traverse preexisting symbolic links in a caller-supplied destination.
        let mut parent = destination.to_path_buf();
        for segment in name.split('/') {
            parent.push(segment);
            if fs::symlink_metadata(&parent).is_ok_and(|m| m.file_type().is_symlink()) {
                bail!("destination contains a symbolic link");
            }
        }
        if kind.is_dir() {
            fs::create_dir_all(&path)?;
        } else {
            fs::create_dir_all(path.parent().context("missing parent")?)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            std::io::copy(&mut entry, &mut file)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = if entry.header().mode()? & 0o111 != 0 {
                    0o755
                } else {
                    0o644
                };
                file.set_permissions(fs::Permissions::from_mode(mode))?;
            }
        }
    }
    let result = validate_dir(destination);
    if !result.valid {
        bail!("archive validation failed:\n{}", result.errors.join("\n"));
    }
    result.manifest.context("manifest missing")
}

pub fn validate(path: &Path) -> Validation {
    if path.is_dir() {
        return validate_dir(path);
    }
    match tempfile::tempdir() {
        Ok(d) => match unpack(path, d.path()) {
            Ok(m) => Validation {
                valid: true,
                errors: vec![],
                manifest: Some(m),
            },
            Err(e) => Validation {
                valid: false,
                errors: vec![e.to_string()],
                manifest: None,
            },
        },
        Err(e) => Validation {
            valid: false,
            errors: vec![e.to_string()],
            manifest: None,
        },
    }
}

pub fn pack(root: &Path, output: Option<&Path>) -> Result<PathBuf> {
    if !root.join("honeycomb.yaml").exists() {
        fs::write(
            root.join("honeycomb.yaml"),
            include_str!("../templates/honeycomb.yaml"),
        )?;
        bail!(
            "This repo doesn't have honeycomb.yaml. The file has been initiated; fill in the application, version and target details, run `honeycomb validate`, then run `honeycomb pack` again."
        );
    }
    let validation = validate_dir(root);
    if !validation.valid {
        bail!("validation failed:\n{}", validation.errors.join("\n"));
    }
    let m = validation.manifest.context("manifest missing")?;
    let default = root.join(format!(
        "{}-{}.tar.gz",
        m.app_id.split('>').next_back().unwrap(),
        m.version
    ));
    let output = output.unwrap_or(&default);
    if output.exists() {
        bail!(
            "{} already exists; choose a new --output path",
            output.display()
        );
    }
    let staged = tempfile::NamedTempFile::new_in(
        output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let mut tar = tar::Builder::new(GzEncoder::new(staged.reopen()?, Compression::default()));
    tar.append_path_with_name(root.join("honeycomb.yaml"), "honeycomb.yaml")?;
    for t in m.targets.values() {
        tar.append_dir_all(&t.root, root.join(&t.root))?;
    }
    let mut file = tar.into_inner()?.finish()?;
    file.flush()?;
    let verified = validate(staged.path());
    if !verified.valid {
        bail!(
            "packed archive failed verification: {}",
            verified.errors.join("\n")
        );
    }
    staged.persist_noclobber(output)?;
    Ok(output.to_path_buf())
}

pub fn sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    Ok(hex::encode(digest.finalize()))
}
pub fn current_target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("linux-x86_64"),
        ("linux", "aarch64") => Ok("linux-aarch64"),
        ("linux", "x86") => Ok("linux-i686"),
        ("macos", "x86_64") => Ok("macos-x86_64"),
        ("macos", "aarch64") => Ok("macos-aarch64"),
        ("windows", "x86_64") => Ok("windows-x86_64"),
        ("windows", "aarch64") => Ok("windows-aarch64"),
        ("windows", "x86") => Ok("windows-i686"),
        (os, arch) => bail!("unsupported platform {os}/{arch}"),
    }
}
