## Local prerequisites
Use Rust 1.98+ and Node 24+. Clone the public repository and build:
```sh
cargo build --workspace --locked
cd web
npm ci
npm run build
```
For a disposable demonstration, run the backend's `e2e_fixture` example and point local websites at `http://127.0.0.1:18080`. The fixture substitutes IAM and Briefcase and is excluded from production images. Never use fixture identities or test tokens in production.

## Backend configuration
The backend does not automatically load dotenv files. Inject configuration through a process environment or secret manager. The repository's [.env.example](https://github.com/teamofsilicons/silicon-honeycomb/blob/main/.env.example) is the canonical list.

| Setting | Purpose |
| --- | --- |
| `HONEYCOMB_BIND` | Listener address. |
| `HONEYCOMB_DATABASE` | Persistent SQLite path. |
| `HONEYCOMB_APP_ID`, `HONEYCOMB_APP_SECRET` | Honeycomb's IAM application identity and credential. |
| `HONEYCOMB_ENCRYPTION_KEY` | Stable encryption key; generate with `openssl rand -hex 32`. |
| `HONEYCOMB_WEBHOOK_SECRET` | Runtime IAM webhook validation. |
| `IAM_BASE_URL`, `IAM_LOGIN_URL` | IAM API and login origins. |
| `IAM_HONEYCOMB_SERVICE_CREDENTIAL` | Protected IAM management credential. |
| `IAM_HONEYCOMB_NOTIFICATION_SIGNING_KEY` | Independent management-notification HMAC key. |
| `BRIEFCASE_BASE_URL` | Briefcase API origin. |
| `HONEYCOMB_WEB_ORIGINS` | Explicit library and console origins. |
| `POSTMARK_SERVER_TOKEN` | Optional queued mail delivery. |
| `HONEYCOMB_TELEMETRY` | Enable/disable optional backend diagnostics. |

Keep secrets out of Git, shell transcripts, frontend build variables, and public logs. Website processes require distinct stable `WEB_SESSION_KEY` values plus the settings in [web/.env.example](https://github.com/teamofsilicons/silicon-honeycomb/blob/main/web/.env.example).

## Production services
Build `deploy/Dockerfile.backend` and `deploy/Dockerfile.web`. Use persistent volumes for all SQLite databases and the TLS proxy. Register each website's `/auth/callback` URL with IAM. Configure effective scopes and live user consent before storage acceptance tests.

The [deployment runbook](https://github.com/teamofsilicons/silicon-honeycomb/blob/main/deploy/README.md) and [AWS runbook](https://github.com/teamofsilicons/silicon-honeycomb/blob/main/deploy/aws/README.md) describe image publishing, digest-pinned rollout, and host startup. Back up SQLite consistently before changes; copying a live WAL database file alone is not a consistent backup. Keep the matching encryption keys recoverable in a separate secret store.

## Required verification
```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --examples --bins --locked
python3 scripts/test_cli.py
python3 scripts/test_install.py
python3 scripts/test_cargo_install.py
cargo package --workspace --exclude silicon-honeycomb-server --locked
```
Run the web browser suite from `web` with `npm run test:e2e`. Run docs checks from `docs-site` with `npm ci`, `npm run build`, and `npm test`.

Local fixtures prove code paths against explicit substitutes. Production acceptance separately requires real IAM login, current role disclosure, app creation, Briefcase upload/read, private access boundaries, publication, and participant lifecycle receipts.

## Ship a new CLI release
Build and test all six native targets through the release workflow. Publish the expected `honeycomb-<target>.tar.gz` archives and `.sha256` files under a `v<version>` GitHub release. Publish crates in dependency order: core, client, CLI. Verify public installer and registry installation in isolated homes. macOS/Windows code signing requires the organization's signing identities.

## Documentation updates
Edit `docs-site/content/*.md` and navigation in `docs-site/pages.json`. `npm run build` generates static HTML, page tables of contents, search data, Markdown downloads, and a sitemap. The CLI reference is generated from executable help using `python3 docs-site/scripts/generate_reference.py`; commit the generated content with the code change. Deploy `docs-site` as its own Vercel project; no backend credentials are required for documentation builds.
