# Silicon Honeycomb

The application library, release manager and testing-environment coordinator for
Carbons and Silicons. Honeycomb owns application workflows; IAM remains the
identity and authorization authority.

| Component | Location | Purpose |
| --- | --- | --- |
| Backend | `crates/server` | IAM authentication, catalog, configuration, releases, reviews and lifecycle records |
| Rust client | `crates/client` | Stateless API and safe installation/maintenance functions |
| CLI | `crates/cli` | Persistent local sessions, packaging, installation and administration |
| Library | `web` (library mode) | Public catalog and authorized private discovery |
| Console | `web` (console mode) | Authenticated application creation and release management |

The human-owned `UNDERSTANDING.md` is the product specification. Integration
requirements and unfinished work are tracked in [IAM-HANDOFF](docs/IAM-HANDOFF.md)
and [IMPLEMENTATION](docs/IMPLEMENTATION.md). This repository is under active
implementation. Live IAM management and ecosystem lifecycle integration are still
pending; production operations remain pending until those services accept them.

## Build and run

Requires Rust 1.98+ and Node 24+.

```sh
cargo build --workspace --locked
cargo run -p silicon-honeycomb-cli -- --help
cd web
npm ci
npm run build
```

Copy the root `.env.example` into your process environment, create the data
directory, then start `cargo run -p silicon-honeycomb-server`. The server does not
automatically load dotenv files. Keep credentials out of shell history and Git.
Use `web/.env.example` and [web/README.md](web/README.md) for both website processes.

For a disposable local demonstration, use
`cargo run -p silicon-honeycomb-server --example e2e_fixture` and configure the
websites' backend to `http://127.0.0.1:18080`. This fixture explicitly substitutes
IAM and Briefcase; it is excluded from the production image.

## Install the CLI

The following public commands become available after the first repository release
and crates publication. No release or crate has been published by this local work.

```sh
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/teamofsilicons/silicon-honeycomb/main/install.sh)"
```

Or:

```sh
cargo install silicon-honeycomb-cli --locked
honeycomb service install
```

Install from this checkout today:

```sh
cargo install --path crates/cli --locked
honeycomb service install
```

The shell installer uses prebuilt macOS/Linux binaries and SHA-256 verification.
Cargo and release binaries also support Windows. The hourly worker checks for
verified vendor releases, honors `config set auto_update false`, and works without
a Rust toolchain. `honeycomb daemon --once` runs one scheduled check. The stateless
client exposes update functions; a library cannot replace a running application's
compiled dependency, so dependency version changes remain a host build decision.

## A typical application workflow

```sh
honeycomb iam --json
honeycomb login '<IAM short-lived token>'
honeycomb login status --json
honeycomb validate ./my-app
honeycomb pack ./my-app --output my-app-1.0.0.tar.gz
honeycomb apps create application.json
honeycomb releases upload 'my-org>my-app' my-app-1.0.0.tar.gz --revision 1
honeycomb publication request 'my-org>my-app' --revision 1 --message 'Why this should be public'
```

Configuration revisions are different from semantic release versions. Reuse
`--idempotency-key` when retrying an uncertain mutation; use `operations get` to
inspect saved work. New applications remain private. Publication requires a valid
six-target CLI release, provider approvals, Honeycomb validation and IAM acceptance.

```sh
honeycomb search briefcase
honeycomb install 'tos>briefcase'
honeycomb update 'tos>briefcase'
honeycomb uninstall 'tos>briefcase'
honeycomb logout
honeycomb report "What happened and how to reproduce it" --pr https://github.com/teamofsilicons/silicon-honeycomb/pull/123
```

Set `SILICON_HOME` or `honeycomb config home /existing/directory` to choose local
storage. Run `honeycomb config env` for the PATH entries. Installations and sessions
are isolated by API origin and test key. `--test <saved-id-or-key>` selects an
isolated environment and never falls back to production on failure.

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --examples --bins --locked
python3 scripts/test_cli.py
python3 scripts/test_install.py
python3 scripts/test_cargo_install.py
cd web
npm ci
npx playwright install chromium
npm run test:e2e
```

Tests cover the real backend/CLI/web implementation with explicit service fixtures.
`crates/server/tests/iam_sdk.rs` also exercises the published IAM SDK over HTTP.
Cross-service production acceptance is separate and requires the IAM handoff.

See [deployment and release instructions](deploy/README.md) for container builds,
requested domains and six-platform release candidates.

## Import an application into a test environment

```sh
honeycomb environments get ENVIRONMENT_ID
honeycomb environments import ENVIRONMENT_ID 'tos>example' --revision 1
# Explicitly refresh existing pins; use the current environment revision.
honeycomb environments import ENVIRONMENT_ID 'tos>example' --revision 2 --refresh
```

Imports recursively include external-scope dependencies, preserve organizations,
and pin accepted configurations and selected releases. Private dependencies require
production access. Follow per-service progress with `environments get`; use
`environments action ENVIRONMENT_ID retry --revision N` for an incomplete operation.
The console exposes the same workflow under Testing environments → Manage.

## Rotate or recover an application secret

```sh
honeycomb apps rotate-secret 'tos>example' --revision 1
# If IAM requires fresh verification, retry with the ORIGINAL idempotency key.
honeycomb --idempotency-key ORIGINAL_KEY apps rotate-secret 'tos>example' --revision 1 --step-up-file /secure/path/assertion
honeycomb operations recover-secret OPERATION_ID
```

IAM controls the replay window and revokes the previous credential according to
its version policy. Honeycomb prints a returned secret once and never saves it.
The console's Access & secrets tab shows requested/effective scopes and your
configuration operations, with retry and recovery controls.
