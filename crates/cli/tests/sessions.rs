//! Real CLI processes with a rotating identity-provider fixture. No real account is used.
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
struct Provider {
    attempts: Vec<(String, String)>,
    rotations: HashMap<String, (String, Value)>,
    drop_response: bool,
    malformed: bool,
    revoked: bool,
}
struct Fixture {
    home: PathBuf,
    origin: String,
    provider: Arc<Mutex<Provider>>,
    stopped: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new() -> Self {
        let home = std::env::temp_dir().join(format!("honeycomb-session-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(home.join(".honeycomb/dir")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let provider = Arc::new(Mutex::new(Provider::default()));
        let stopped = Arc::new(AtomicBool::new(false));
        let state = provider.clone();
        let halt = stopped.clone();
        let server = thread::spawn(move || {
            let mut workers = Vec::new();
            while !halt.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = state.clone();
                        workers.push(thread::spawn(move || respond(stream, state)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        fs::write(
            home.join(".honeycomb/dir/config.json"),
            serde_json::to_vec(&json!({
                "api": origin, "auto_update": true, "telemetry": false,
                // Keep all fixture maintenance offline, including minute-boundary runs.
                "last_update_check": now() + 86400,
            }))
            .unwrap(),
        )
        .unwrap();
        Self {
            home,
            origin,
            provider,
            stopped,
            server: Some(server),
        }
    }
    fn directory(&self, environment: Option<&str>) -> PathBuf {
        self.home
            .join(".honeycomb/dir/contexts")
            .join(honeycomb_client::context_fingerprint(
                &self.origin,
                environment,
            ))
    }
    fn seed(&self, environment: Option<&str>) -> PathBuf {
        let directory = self.directory(environment);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("session.json"), serde_json::to_vec(&json!({
            "access_token": "access-old", "refresh_token": "refresh-old", "expires_at": now() - 60,
        })).unwrap()).unwrap();
        directory
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_honeycomb"));
        command
            .args(args)
            .arg("--json")
            .env("SILICON_HOME", &self.home)
            .env("HONEYCOMB_AUTO_UPDATE", "0")
            .env("HONEYCOMB_TELEMETRY", "0")
            .env_remove("HONEYCOMB_API_URL");
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn spawn(&self, args: &[&str], daemon: bool) -> Child {
        self.command(args)
            .env("HONEYCOMB_AUTO_UPDATE", if daemon { "1" } else { "0" })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }
    fn saved(&self, environment: Option<&str>) -> Value {
        serde_json::from_slice(&fs::read(self.directory(environment).join("session.json")).unwrap())
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.server.take().unwrap().join().unwrap();
        fs::remove_dir_all(&self.home).unwrap();
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or(Value::Null)
}
fn respond(mut stream: TcpStream, provider: Arc<Mutex<Provider>>) {
    // Accepted sockets inherit O_NONBLOCK on macOS; the fixture reads a whole request.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut bytes = Vec::new();
    let (headers, body) = loop {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).unwrap();
        if count == 0 {
            return;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
            let size: usize = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .unwrap_or("0")
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + size {
                break (headers, bytes[end + 4..].to_vec());
            }
        }
    };
    let mut status = 200;
    let value = if headers.starts_with("post /api/v1/auth/refresh ") {
        let body: Value = serde_json::from_slice(&body).unwrap();
        let token = body["refresh_token"].as_str().unwrap().to_owned();
        let key = headers
            .lines()
            .find_map(|line| line.strip_prefix("idempotency-key: "))
            .unwrap()
            .to_owned();
        let mut state = provider.lock().unwrap();
        state.attempts.push((token.clone(), key.clone()));
        let value = if let Some((original, value)) = state.rotations.get(&token) {
            if *original == key {
                value.clone()
            } else {
                state.revoked = true;
                status = 401;
                json!({"error":{"code":"invalid_grant", "message":"refresh reused with a different key"}})
            }
        } else {
            let generation = state.rotations.len() + 1;
            let value = json!({"access_token": format!("access-{generation}"), "refresh_token": format!("refresh-{generation}"), "expires_in":1800});
            state.rotations.insert(token, (key, value.clone()));
            value
        };
        if state.drop_response {
            state.drop_response = false;
            return;
        }
        if state.malformed {
            state.malformed = false;
            json!({"expires_in": -1})
        } else {
            drop(state);
            // Keep the race reproducible: a second unguarded process enters refresh here.
            thread::sleep(Duration::from_millis(80));
            value
        }
    } else if headers.starts_with("get /api/v1/auth/status ") {
        let valid = !headers.contains("bearer access-old") && !provider.lock().unwrap().revoked;
        if !valid {
            status = 401;
        }
        json!({"authenticated":valid})
    } else if headers.starts_with("post /api/v1/auth/logout ") {
        provider.lock().unwrap().revoked = true;
        json!({})
    } else if headers.starts_with("post /api/v1/auth/login ") {
        json!({"access_token":"access-login", "refresh_token":"refresh-login", "expires_in":1800})
    } else if headers.starts_with("get /api/v1/iam ") {
        json!({"app_id":"fixture>honeycomb"})
    } else {
        status = 404;
        json!({"error":{"code":"not_found", "message":"fixture has no packages"}})
    };
    let data = serde_json::to_vec(&value).unwrap();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        data.len()
    );
    let _ = stream.write_all(&data);
}

#[test]
fn login_status_renews_the_expired_access_token_and_persists_the_successor() {
    let fixture = Fixture::new();
    fixture.seed(None);
    assert_eq!(
        success(fixture.run(&["login", "status"]))["authenticated"],
        true
    );
    assert_eq!(fixture.saved(None)["refresh_token"], "refresh-1");
    success(fixture.run(&["login", "status"]));
    assert_eq!(fixture.provider.lock().unwrap().attempts.len(), 1);
}

#[test]
fn concurrent_commands_and_daemon_rotate_only_once() {
    let fixture = Fixture::new();
    let directory = fixture.seed(None);
    fs::write(
        directory.join("context.json"),
        serde_json::to_vec(&json!({"api":fixture.origin,"testing_key":null})).unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join("installed.json"),
        serde_json::to_vec(&json!({"fixture>app":{
            "app_id":"fixture>app", "version":"1.0.0", "target":"fixture", "directory":directory,
            "commands":{}, "aliases":{}, "sha256":"fixture"
        }}))
        .unwrap(),
    )
    .unwrap();
    let daemon = fixture.spawn(&["daemon", "--once"], true);
    let commands: Vec<_> = (0..5)
        .map(|_| fixture.spawn(&["login", "status"], false))
        .collect();
    success(daemon.wait_with_output().unwrap());
    for child in commands {
        assert_eq!(
            success(child.wait_with_output().unwrap())["authenticated"],
            true
        );
    }
    let provider = fixture.provider.lock().unwrap();
    assert_eq!(provider.attempts.len(), 1);
    assert!(!provider.revoked);
}

#[test]
fn lost_or_malformed_refresh_response_retries_the_persisted_key_after_process_restart() {
    for malformed in [false, true] {
        let fixture = Fixture::new();
        fixture.seed(None);
        {
            let mut provider = fixture.provider.lock().unwrap();
            provider.drop_response = !malformed;
            provider.malformed = malformed;
        }
        assert!(!fixture.run(&["login", "status"]).status.success());
        let pending = fixture.saved(None);
        assert_eq!(pending["refresh_token"], "refresh-old");
        assert!(pending["pending_refresh"]["key"].is_string());
        assert_eq!(
            success(fixture.run(&["login", "status"]))["authenticated"],
            true
        );
        let provider = fixture.provider.lock().unwrap();
        assert_eq!(provider.attempts.len(), 2);
        assert_eq!(provider.attempts[0], provider.attempts[1]);
        assert!(!provider.revoked);
        assert!(fixture.saved(None)["pending_refresh"].is_null());
    }
}

#[test]
fn logout_waits_for_rotation_and_cannot_be_undone_by_a_late_save() {
    let fixture = Fixture::new();
    let directory = fixture.seed(None);
    let refresh = fixture.spawn(&["login", "status"], false);
    for _ in 0..200 {
        if !fixture.provider.lock().unwrap().attempts.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fixture.provider.lock().unwrap().attempts.len(), 1);
    let logout = fixture.spawn(&["logout"], false);
    let status = refresh.wait_with_output().unwrap();
    success(logout.wait_with_output().unwrap());
    // The session lock serializes rotation and deletion. Once rotation releases
    // it, logout may revoke the token before the in-flight status HTTP read.
    // Both a successful read and that explicit revocation are valid outcomes.
    if status.status.success() {
        assert_eq!(success(status)["authenticated"], true);
    } else {
        let error = String::from_utf8_lossy(&status.stderr);
        assert!(error.contains("HTTP 401"), "{error}");
    }
    assert!(!directory.join("session.json").exists());
    assert_eq!(
        success(fixture.run(&["login", "status"]))["authenticated"],
        false
    );
    assert!(fixture.provider.lock().unwrap().revoked);
}

#[test]
fn testing_context_does_not_borrow_or_overwrite_the_production_session() {
    let fixture = Fixture::new();
    fixture.seed(None);
    let test_key = "T".repeat(32);
    fixture.seed(Some(&test_key));
    success(fixture.run(&["--test", &test_key, "login", "status"]));
    assert_eq!(fixture.saved(None)["refresh_token"], "refresh-old");
    assert_eq!(fixture.saved(Some(&test_key))["refresh_token"], "refresh-1");
}
