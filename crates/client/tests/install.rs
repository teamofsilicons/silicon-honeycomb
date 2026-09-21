use silicon_honeycomb_client::{installer, package};
use std::{collections::BTreeMap, fs};
fn fixture(root: &std::path::Path, version: &str) -> std::path::PathBuf {
    fixture_named(root, version, "honeycomb-test-hello", "honeycomb-test-hi")
}
/// Distinct command names keep a test that steers PATH from shadowing another test's package.
fn fixture_named(
    root: &std::path::Path,
    version: &str,
    first: &str,
    second: &str,
) -> std::path::PathBuf {
    let mut targets = serde_json::Map::new();
    for t in package::REQUIRED_TARGETS {
        let dir = root.join(format!("targets/{t}/bin"));
        fs::create_dir_all(&dir).unwrap();
        let windows = t.starts_with("windows-");
        let filename = if windows { "hello.cmd" } else { "hello" };
        let path = dir.join(filename);
        let script = if windows {
            format!("@echo off\r\necho hello-{version}\r\n")
        } else {
            format!("#!/bin/sh\nprintf 'hello-{version}\\n'\n")
        };
        fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        targets.insert(
            t.into(),
            serde_json::json!({"root":format!("targets/{t}"),"executables":{"app":format!("bin/{filename}")}}),
        );
    }
    let manifest = serde_json::json!({"format_version":1,"app_id":"tos>hello","version":version,"bin":{first:"app",second:"app"},"targets":targets});
    fs::write(root.join("honeycomb.yaml"), manifest.to_string()).unwrap();
    package::pack(root, None).unwrap()
}
#[test]
fn optional_identity_uses_selected_app_and_preserves_ownership() {
    let _serial = serial();
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let archive = fixture(src.path(), "1.0.0");
    assert!(
        installer::install_archive_for("tos>other", &archive, dest.path(), &BTreeMap::new(), None)
            .is_err()
    );
    let path = src.path().join("honeycomb.yaml");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    manifest.as_object_mut().unwrap().remove("app_id");
    fs::write(&path, manifest.to_string()).unwrap();
    let archive = package::pack(src.path(), Some(&src.path().join("optional.tar.gz"))).unwrap();
    assert!(installer::install_archive(&archive, dest.path(), &BTreeMap::new(), None).is_err());
    assert!(
        installer::install_archive_for("../unsafe", &archive, dest.path(), &BTreeMap::new(), None)
            .is_err()
    );
    let installed = installer::install_archive_for(
        "tos>selected",
        &archive,
        dest.path(),
        &BTreeMap::new(),
        None,
    )
    .unwrap();
    assert_eq!(installed.app_id, "tos>selected");
    assert!(
        installed.directory.starts_with(
            dest.path()
                .canonicalize()
                .unwrap()
                .join("packages/tos/selected")
        )
    );
    assert!(
        installer::install_archive_for(
            "tos>other",
            &archive,
            dest.path(),
            &BTreeMap::new(),
            Some(&installed)
        )
        .is_err()
    );
    installer::uninstall(&installed, dest.path()).unwrap();
}
#[test]
fn install_update_execute_and_uninstall() {
    let _serial = serial();
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let first = fixture(src.path(), "1.0.0");
    let aliases = BTreeMap::from([(
        "honeycomb-test-hello".into(),
        "honeycomb-test-custom".into(),
    )]);
    let installed = installer::install_archive(&first, dest.path(), &aliases, None).unwrap();
    assert_eq!(installed.commands.len(), 2);
    {
        let result = std::process::Command::new(&installed.commands["honeycomb-test-hello"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "launcher failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).replace("\r\n", "\n"),
            "hello-1.0.0\n"
        );
    }
    let second = fixture(src.path(), "1.1.0");
    let updated =
        installer::install_archive(&second, dest.path(), &aliases, Some(&installed)).unwrap();
    assert!(!installed.directory.exists());
    assert_eq!(updated.version, "1.1.0");
    {
        let result = std::process::Command::new(&updated.commands["honeycomb-test-hello"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "updated launcher failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).replace("\r\n", "\n"),
            "hello-1.1.0\n"
        );
    }
    installer::uninstall(&updated, dest.path()).unwrap();
    assert!(!updated.directory.exists());
    assert!(
        updated
            .commands
            .values()
            .all(|p| p.symlink_metadata().is_err())
    );
}
#[test]
fn collision_does_not_modify_existing_commands() {
    let _serial = serial();
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let archive = fixture(src.path(), "1.0.0");
    fs::create_dir(dest.path().join("bin")).unwrap();
    let collision = dest.path().join(if cfg!(windows) {
        "bin/honeycomb-test-hi.cmd"
    } else {
        "bin/honeycomb-test-hi"
    });
    fs::write(&collision, "do not replace").unwrap();
    let result = installer::install_archive(&archive, dest.path(), &BTreeMap::new(), None);
    assert!(result.unwrap_err().to_string().contains("collision"));
    assert_eq!(fs::read_to_string(collision).unwrap(), "do not replace");
    assert!(
        !dest
            .path()
            .join(if cfg!(windows) {
                "bin/honeycomb-test-hello.cmd"
            } else {
                "bin/honeycomb-test-hello"
            })
            .exists()
    );
}
#[test]
fn uninstall_preserves_a_replaced_command() {
    let _serial = serial();
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let archive = fixture(src.path(), "1.0.0");
    let installed =
        installer::install_archive(&archive, dest.path(), &BTreeMap::new(), None).unwrap();
    let replaced = &installed.commands["honeycomb-test-hi"];
    fs::remove_file(replaced).unwrap();
    fs::write(replaced, "user replacement").unwrap();
    installer::uninstall(&installed, dest.path()).unwrap();
    assert_eq!(fs::read_to_string(replaced).unwrap(), "user replacement");
}
#[test]
fn contexts_do_not_share_sessions() {
    use silicon_honeycomb_client::context_fingerprint as f;
    assert_ne!(
        f("https://one.example", None),
        f("https://two.example", None)
    );
    assert_ne!(
        f("https://one.example", Some("test-key-one")),
        f("https://one.example", Some("test-key-two"))
    );
    assert_ne!(
        f("https://one.example", None),
        f("https://one.example", Some("test-key-one"))
    );
}

/// PATH is process-wide and the installer reads it, so every test that reaches the installer
/// takes this lock - reading it while another test replaces it is a data race.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn serial() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

struct PathGuard(Option<std::ffi::OsString>);
impl Drop for PathGuard {
    fn drop(&mut self) {
        match self.0.take() {
            Some(previous) => unsafe { std::env::set_var("PATH", previous) },
            None => unsafe { std::env::remove_var("PATH") },
        }
    }
}

/// Put `dir` first on PATH, keeping the rest so ordinary tools still resolve.
fn with_path(dir: &std::path::Path) -> PathGuard {
    let previous = std::env::var_os("PATH");
    let mut entries = vec![dir.to_path_buf()];
    if let Some(current) = &previous {
        entries.extend(std::env::split_paths(current));
    }
    unsafe { std::env::set_var("PATH", std::env::join_paths(entries).unwrap()) };
    PathGuard(previous)
}

/// A command from some other installation, reporting its version like a real CLI.
fn other_installation(dir: &std::path::Path, name: &str, version: &str) -> std::path::PathBuf {
    let windows = cfg!(windows);
    let path = dir.join(if windows {
        format!("{name}.cmd")
    } else {
        name.to_owned()
    });
    let script = if windows {
        format!("@echo off\r\necho other {version}\r\n")
    } else {
        format!("#!/bin/sh\nprintf 'other {version}\\n'\n")
    };
    fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[test]
fn a_new_enough_command_on_path_resolves_the_dependency() {
    let _serial = serial();
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let archive = fixture_named(
        src.path(),
        "1.0.0",
        "honeycomb-path-hello",
        "honeycomb-path-hi",
    );
    let existing = other_installation(elsewhere.path(), "honeycomb-path-hi", "2.0.0");
    let _path = with_path(elsewhere.path());
    let installed =
        installer::install_archive(&archive, dest.path(), &BTreeMap::new(), None).unwrap();
    assert_eq!(installed.resolved.len(), 1);
    assert_eq!(installed.resolved[0].command, "honeycomb-path-hi");
    assert_eq!(
        installed.resolved[0].existing_version.as_deref(),
        Some("2.0.0")
    );
    assert_eq!(installed.resolved[0].required_version, "1.0.0");
    assert!(installed.resolved[0].satisfied);
    assert!(installed.rewritten.is_empty());
    // The other installation keeps its command.
    assert!(fs::read_to_string(&existing).unwrap().contains("2.0.0"));
    assert!(
        installed.commands["honeycomb-path-hi"]
            .symlink_metadata()
            .is_ok()
    );
}

#[test]
fn an_older_command_on_path_reports_both_versions_and_installs_nothing() {
    let _serial = serial();
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let archive = fixture_named(
        src.path(),
        "1.0.0",
        "honeycomb-path-hello",
        "honeycomb-path-hi",
    );
    let existing = other_installation(elsewhere.path(), "honeycomb-path-hi", "0.5.0");
    let _path = with_path(elsewhere.path());
    let error = installer::install_archive_resolving(
        "tos>hello",
        &archive,
        dest.path(),
        &BTreeMap::new(),
        None,
        false,
    )
    .unwrap_err();
    let message = error.to_string();
    let unresolved = error
        .downcast_ref::<installer::UnresolvedCommands>()
        .expect("an older command must be reported as a dependency question");
    assert_eq!(unresolved.0.len(), 1);
    assert_eq!(unresolved.0[0].existing_version.as_deref(), Some("0.5.0"));
    assert_eq!(unresolved.0[0].required_version, "1.0.0");
    assert!(!unresolved.0[0].satisfied);
    assert!(
        message.contains("0.5.0") && message.contains("1.0.0"),
        "{message}"
    );
    // Nothing was installed and nothing was replaced.
    assert!(fs::read_to_string(&existing).unwrap().contains("0.5.0"));
    assert!(
        !dest
            .path()
            .join(if cfg!(windows) {
                "bin/honeycomb-path-hello.cmd"
            } else {
                "bin/honeycomb-path-hello"
            })
            .exists()
    );
}

#[test]
fn rewriting_replaces_an_older_command_and_uninstall_puts_it_back() {
    let _serial = serial();
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let archive = fixture_named(
        src.path(),
        "1.0.0",
        "honeycomb-path-hello",
        "honeycomb-path-hi",
    );
    let existing = other_installation(elsewhere.path(), "honeycomb-path-hi", "0.5.0");
    let _path = with_path(elsewhere.path());
    let installed = installer::install_archive_resolving(
        "tos>hello",
        &archive,
        dest.path(),
        &BTreeMap::new(),
        None,
        true,
    )
    .unwrap();
    assert_eq!(installed.rewritten.len(), 1);
    assert_eq!(installed.rewritten["honeycomb-path-hi"].path, existing);
    let output = std::process::Command::new(&existing).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        "hello-1.0.0\n"
    );
    let newer = fixture_named(
        src.path(),
        "2.0.0",
        "honeycomb-path-hello",
        "honeycomb-path-hi",
    );
    let updated = installer::install_archive_channel_resolving(
        "tos>hello",
        silicon_honeycomb_client::ReleaseChannel::Dev,
        &newer,
        dest.path(),
        &BTreeMap::new(),
        Some(&installed),
        false,
    )
    .unwrap();
    assert_eq!(
        updated.rewritten["honeycomb-path-hi"].backup,
        installed.rewritten["honeycomb-path-hi"].backup
    );
    assert!(
        String::from_utf8_lossy(
            &std::process::Command::new(&existing)
                .output()
                .unwrap()
                .stdout
        )
        .contains("2.0.0")
    );
    installer::uninstall(&updated, dest.path()).unwrap();
    let output = std::process::Command::new(&existing).output().unwrap();
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("0.5.0"),
        "the displaced command must come back"
    );
}

#[test]
fn removing_a_rewritten_command_restores_its_original_during_channel_switch() {
    let _serial = serial();
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let archive = fixture(src.path(), "1.0.0");
    let existing = other_installation(elsewhere.path(), "honeycomb-test-hi", "0.5.0");
    let _path = with_path(elsewhere.path());
    let installed = installer::install_archive_resolving(
        "tos>hello",
        &archive,
        dest.path(),
        &BTreeMap::new(),
        None,
        true,
    )
    .unwrap();
    fixture(src.path(), "2.0.0");
    let manifest = src.path().join("honeycomb.yaml");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    value["bin"]
        .as_object_mut()
        .unwrap()
        .remove("honeycomb-test-hi");
    fs::write(manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    let archive =
        package::pack(src.path(), Some(&src.path().join("removed-command.tar.gz"))).unwrap();
    let updated = installer::install_archive_channel_resolving(
        "tos>hello",
        silicon_honeycomb_client::ReleaseChannel::Dev,
        &archive,
        dest.path(),
        &BTreeMap::new(),
        Some(&installed),
        false,
    )
    .unwrap();
    assert!(updated.rewritten.is_empty());
    assert!(!installed.directory.exists());
    assert!(
        String::from_utf8_lossy(
            &std::process::Command::new(&existing)
                .output()
                .unwrap()
                .stdout
        )
        .contains("0.5.0")
    );
    installer::uninstall(&updated, dest.path()).unwrap();
    assert!(existing.exists());
}
