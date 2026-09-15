use silicon_honeycomb_core::package::*;
use std::{fs, path::Path};
fn fixture(root: &Path) {
    let mut targets = serde_json::Map::new();
    for target in REQUIRED_TARGETS {
        let dir = root.join(format!("targets/{target}/bin"));
        fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("hello");
        fs::write(&binary, b"#!/bin/sh\necho hello\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
        }
        targets.insert(target.into(),serde_json::json!({"root":format!("targets/{target}"),"executables":{"app":"bin/hello"}}));
    }
    let manifest = serde_json::json!({"format_version":1,"app_id":"tos>hello","version":"1.0.0","bin":{"hello":"app","hi":"app"},"targets":targets});
    fs::write(
        root.join("honeycomb.yaml"),
        serde_yaml::to_string(&manifest).unwrap(),
    )
    .unwrap();
}
#[test]
fn round_trip_all_targets_and_aliases() {
    let d = tempfile::tempdir().unwrap();
    fixture(d.path());
    assert!(validate_dir(d.path()).valid);
    let archive = pack(d.path(), None).unwrap();
    let out = tempfile::tempdir().unwrap();
    let m = unpack(&archive, out.path()).unwrap();
    assert_eq!(m.targets.len(), 6);
    assert_eq!(m.bin.len(), 2);
    assert_eq!(
        fs::read(out.path().join("targets/macos-aarch64/bin/hello")).unwrap(),
        b"#!/bin/sh\necho hello\n"
    );
}
#[test]
fn aggregates_missing_targets_and_invalid_metadata() {
    let d = tempfile::tempdir().unwrap();
    fs::write(
        d.path().join("honeycomb.yaml"),
        "format_version: 3\napp_id: nope\nversion: bad\nbin: {}\ntargets: {}\n",
    )
    .unwrap();
    let result = validate_dir(d.path());
    assert!(!result.valid);
    assert!(result.errors.len() >= 10);
}
#[test]
fn rejects_secret_fields() {
    let d = tempfile::tempdir().unwrap();
    fixture(d.path());
    let p = d.path().join("honeycomb.yaml");
    let mut s = fs::read_to_string(&p).unwrap();
    s.push_str("app_secret: must-never-ship\n");
    fs::write(p, s).unwrap();
    assert!(!validate_dir(d.path()).valid);
}
#[test]
fn refuses_archive_links() {
    let d = tempfile::tempdir().unwrap();
    let archive = d.path().join("bad.tar.gz");
    let f = fs::File::create(&archive).unwrap();
    let mut t = tar::Builder::new(flate2::write::GzEncoder::new(
        f,
        flate2::Compression::default(),
    ));
    let mut h = tar::Header::new_gnu();
    h.set_entry_type(tar::EntryType::Symlink);
    h.set_size(0);
    h.set_mode(0o777);
    h.set_link_name("/etc/passwd").unwrap();
    h.set_cksum();
    t.append_data(&mut h, "targets/link", &[][..]).unwrap();
    t.into_inner().unwrap().finish().unwrap();
    assert!(validate(&archive).errors.join(" ").contains("links"));
}
#[test]
fn rejects_cross_platform_traversal() {
    for p in ["../x", "/x", "a/../b", "a\\b", "C:/x", "a//b", "a/./b", ""] {
        assert!(!safe_relative(p), "{p}");
    }
    assert!(safe_relative("targets/linux-x86_64/bin/hello"));
}
#[test]
fn missing_manifest_creates_template_without_overwriting() {
    let d = tempfile::tempdir().unwrap();
    assert!(pack(d.path(), None).is_err());
    assert!(d.path().join("honeycomb.yaml").is_file());
}
#[cfg(unix)]
#[test]
fn rejects_symlinked_target_roots() {
    let d = tempfile::tempdir().unwrap();
    fixture(d.path());
    fs::remove_dir_all(d.path().join("targets/linux-x86_64")).unwrap();
    std::os::unix::fs::symlink("macos-aarch64", d.path().join("targets/linux-x86_64")).unwrap();
    assert!(!validate_dir(d.path()).valid);
}

#[cfg(unix)]
#[test]
fn rejects_linked_target_ancestors_and_destination() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let relocated = tempfile::tempdir().unwrap();
    fs::rename(
        root.path().join("targets"),
        relocated.path().join("targets"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        relocated.path().join("targets"),
        root.path().join("targets"),
    )
    .unwrap();
    assert!(!validate_dir(root.path()).valid);
    let source = tempfile::tempdir().unwrap();
    fixture(source.path());
    let archive = pack(source.path(), None).unwrap();
    let target = tempfile::tempdir().unwrap();
    let link = root.path().join("extraction-link");
    std::os::unix::fs::symlink(target.path(), &link).unwrap();
    assert!(unpack(&archive, &link).is_err());
    assert_eq!(fs::read_dir(target.path()).unwrap().count(), 0);
}
#[test]
fn rejects_windows_device_names_and_nonportable_paths() {
    for name in ["CON", "con.txt", "LPT9", "com2.exe"] {
        assert!(!safe_command(name));
    }
    for path in [
        "targets/a/con.txt",
        "targets/a/name.",
        "targets/a/name ",
        "targets/a/what?",
        "targets/a/b\n",
    ] {
        assert!(!safe_relative(path));
    }
    assert!(safe_relative("targets/linux-x86_64/bin/a tool"));
}
