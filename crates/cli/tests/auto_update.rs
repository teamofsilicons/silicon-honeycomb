use std::{fs, process::Command};

#[cfg(target_os = "macos")]
#[test]
fn worker_registration_keeps_managed_invocation_and_is_idempotent() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let home =
        std::env::temp_dir().join(format!("honeycomb-service-test-{}", uuid::Uuid::new_v4()));
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let cli = bin.join("managed-honeycomb");
    symlink(env!("CARGO_BIN_EXE_honeycomb"), &cli).unwrap();
    let fake = bin.join("launchctl");
    fs::write(&fake, "#!/bin/sh\nif [ \"$1\" = print ]; then test -f \"$HOME/started\"; exit $?; fi\nprintf '%s\\n' \"$*\" >> \"$HOME/calls\"\nif [ \"$1\" = bootstrap ]; then touch \"$HOME/started\"; fi\n").unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let paths = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    for _ in 0..2 {
        let output = Command::new(&cli)
            .args(["service", "install", "--json"])
            .env("HOME", &home)
            .env("SILICON_HOME", &home)
            .env("PATH", &paths)
            .env("HONEYCOMB_AUTO_UPDATE", "0")
            .env("HONEYCOMB_TELEMETRY", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(serde_json::from_slice::<serde_json::Value>(&output.stdout).is_ok());
    }
    let plist = fs::read_dir(home.join("Library/LaunchAgents"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let body = fs::read_to_string(plist).unwrap();
    assert!(body.contains(cli.to_str().unwrap()));
    assert!(!body.contains(env!("CARGO_BIN_EXE_honeycomb")));
    assert_eq!(
        fs::read_to_string(home.join("calls"))
            .unwrap()
            .lines()
            .filter(|line| line.starts_with("bootstrap"))
            .count(),
        1
    );
    let output = Command::new(&cli)
        .args(["self-update", "--json"])
        .env("HOME", &home)
        .env("SILICON_HOME", &home)
        .env("HONEYCOMB_TELEMETRY", "0")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("managed command link"));
    fs::remove_dir_all(home).unwrap();
}

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

#[test]
fn directly_invoked_managed_payload_refuses_self_update_without_network_or_mutation() {
    let home =
        std::env::temp_dir().join(format!("honeycomb-payload-test-{}", uuid::Uuid::new_v4()));
    let root = home.join(".honeycomb/dir");
    let state = root.join("contexts/origin");
    let payload = state.join("packages/tos/honeycomb/0.3.0-123456789012");
    fs::create_dir_all(&payload).unwrap();
    fs::write(state.join("installed.json"), "{}").unwrap();
    let executable = payload.join(format!("honeycomb{}", std::env::consts::EXE_SUFFIX));
    fs::copy(env!("CARGO_BIN_EXE_honeycomb"), &executable).unwrap();
    let before = fs::read(&executable).unwrap();
    let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
    // The same payload remains immutable if SILICON_HOME points at another installation.
    for selected_home in [&home, &home.join("other-home")] {
        let selected_root = selected_home.join(".honeycomb/dir");
        fs::create_dir_all(&selected_root).unwrap();
        let config = br#"{"api":"http://127.0.0.1:1","auto_update":true,"telemetry":false,"last_update_check":0}"#;
        fs::write(selected_root.join("config.json"), config).unwrap();
        let output = Command::new(&executable)
            .args(["self-update", "--json"])
            .env("SILICON_HOME", selected_home)
            .env("HONEYCOMB_AUTO_UPDATE", "0")
            .env("HONEYCOMB_TELEMETRY", "0")
            .env("HTTP_PROXY", &proxy_url)
            .env("HTTPS_PROXY", &proxy_url)
            .env("ALL_PROXY", &proxy_url)
            .env("NO_PROXY", "")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("managed package payload"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(selected_root.join("config.json")).unwrap(), config);
        assert!(!selected_root.join("update.lock").exists());
        assert!(!selected_root.join("system").exists());
        assert!(
            matches!(proxy.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
        assert_eq!(fs::read(&executable).unwrap(), before);
    }
    fs::remove_dir_all(home).unwrap();
}
