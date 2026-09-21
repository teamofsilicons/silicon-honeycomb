use std::{fs, process::Command};

#[test]
fn process_opt_out_preserves_fresh_and_existing_home_preferences() {
    let home = std::env::temp_dir().join(format!("honeycomb-update-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&home).unwrap();
    let root = home.join(".honeycomb/dir");
    for existing in [false, true] {
        if existing {
            fs::write(root.join("config.json"), r#"{"api":"http://127.0.0.1:1","auto_update":true,"telemetry":false,"last_update_check":0}"#).unwrap();
        }
        let before = fs::read(root.join("config.json")).ok();
        for args in [
            vec!["installed", "--json"],
            vec!["daemon", "--once", "--json"],
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_honeycomb"))
                .args(args)
                .env("SILICON_HOME", &home)
                .env("HONEYCOMB_AUTO_UPDATE", "0")
                .env("HONEYCOMB_TELEMETRY", "0")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(fs::read(root.join("config.json")).ok(), before);
            assert!(!root.join("update.lock").exists());
            assert!(!root.join("package-update.lock").exists());
        }
    }
    fs::remove_dir_all(home).unwrap();
}
