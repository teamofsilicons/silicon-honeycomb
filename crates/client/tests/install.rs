use silicon_honeycomb_client::{installer, package};
use std::{collections::BTreeMap, fs};
fn fixture(root: &std::path::Path, version: &str) -> std::path::PathBuf {
    let mut targets = serde_json::Map::new();
    for t in package::REQUIRED_TARGETS {
        let dir = root.join(format!("targets/{t}/bin"));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hello");
        fs::write(&path, format!("#!/bin/sh\nprintf 'hello-{version}\\n'\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        targets.insert(
            t.into(),
            serde_json::json!({"root":format!("targets/{t}"),"executables":{"app":"bin/hello"}}),
        );
    }
    let manifest = serde_json::json!({"format_version":1,"app_id":"tos>hello","version":version,"bin":{"honeycomb-test-hello":"app","honeycomb-test-hi":"app"},"targets":targets});
    fs::write(root.join("honeycomb.yaml"), manifest.to_string()).unwrap();
    package::pack(root, None).unwrap()
}
#[test]
fn install_update_execute_and_uninstall() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let first = fixture(src.path(), "1.0.0");
    let aliases = BTreeMap::from([(
        "honeycomb-test-hello".into(),
        "honeycomb-test-custom".into(),
    )]);
    let installed = installer::install_archive(&first, dest.path(), &aliases, None).unwrap();
    assert_eq!(installed.commands.len(), 2);
    #[cfg(unix)]
    {
        let result = std::process::Command::new(&installed.commands["honeycomb-test-hello"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&result.stdout), "hello-1.0.0\n");
    }
    let second = fixture(src.path(), "1.1.0");
    let updated =
        installer::install_archive(&second, dest.path(), &aliases, Some(&installed)).unwrap();
    assert!(!installed.directory.exists());
    assert_eq!(updated.version, "1.1.0");
    #[cfg(unix)]
    {
        let result = std::process::Command::new(&updated.commands["honeycomb-test-hello"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&result.stdout), "hello-1.1.0\n");
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
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let archive = fixture(src.path(), "1.0.0");
    fs::create_dir(dest.path().join("bin")).unwrap();
    let collision = dest.path().join("bin/honeycomb-test-hi");
    fs::write(&collision, "do not replace").unwrap();
    let result = installer::install_archive(&archive, dest.path(), &BTreeMap::new(), None);
    assert!(result.unwrap_err().to_string().contains("collision"));
    assert_eq!(fs::read_to_string(collision).unwrap(), "do not replace");
    assert!(!dest.path().join("bin/honeycomb-test-hello").exists());
}
#[test]
fn uninstall_preserves_a_replaced_command() {
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
