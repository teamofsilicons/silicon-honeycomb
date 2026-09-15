# Silicon Honeycomb

Licensed under [MIT](LICENSE). Vendored dependencies retain their
[third-party notices](THIRD-PARTY-NOTICES.md).
Run `honeycomb license` to read the notice embedded in a native CLI binary.

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
requirements and unfinished work are tracked in the [local handoff](docs/LOCAL-HANDOFF.md),
[current IAM contract review](docs/IAM-CONTRACT-REVIEW.md), and
[implementation history](docs/IMPLEMENTATION.md). This repository is under active
implementation. Live IAM management and ecosystem lifecycle integration are still
pending; production operations remain pending until those services accept them.

See [API contracts and compatibility](docs/API-CONTRACTS.md) for version selection,
consumer compatibility, deprecation and the seven-day idle retirement policy.

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
It adds Honeycomb to Bash and Zsh startup files automatically (including a custom
`ZDOTDIR`). Open a new terminal after installation, or run the printed activation
command in your existing terminal. Set `HONEYCOMB_NO_MODIFY_PATH=1` to manage
your shell configuration yourself. Downloads display progress and time out with
an error instead of waiting indefinitely.
Cargo and release binaries also support Windows. The hourly worker checks for
verified vendor releases, honors `config set auto_update false`, and works without
a Rust toolchain. `honeycomb daemon --once` runs one scheduled check. The stateless
client exposes update functions; a library cannot replace a running application's
compiled dependency, so dependency version changes remain a host build decision.

Diagnostics are enabled by default when the backend's Space Station key is
configured. Use `honeycomb config set telemetry false` to opt out, or turn off
**Share usage and diagnostics** in either website's **Telemetry settings**.
See [telemetry setup and event details](docs/TELEMETRY.md).

## A typical application workflow

The console accepts either a logo URL or a PNG, JPEG or WebP file. Files may be up
to 2 MiB and 2048 × 2048 pixels. Honeycomb removes metadata, converts the image to
PNG and stores it beside the release archives in Briefcase. Uploaded logos have a
publicly viewable link, including logos used by private applications.

From the CLI, upload a logo before creating or editing the application:

```sh
honeycomb apps upload-logo my-org ./logo.png
```

Set the returned `logo_url` in `application.json`. Use the same explicit
`--idempotency-key` and file when retrying an interrupted upload. The Rust client
provides the equivalent `Client::upload_logo` method.

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
cargo package --workspace --exclude silicon-honeycomb-server --locked
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

## Review application access

```sh
honeycomb publication inbox
honeycomb publication review REQUEST_ID 'tos>provider'
honeycomb publication review-reply REQUEST_ID 'tos>provider' --message 'Please explain this scope.'
honeycomb publication decide REQUEST_ID 'tos>provider' approve --revision 1 --reason 'Access reviewed.'
```

Provider administrators review their own critical scopes. IAM reviewers require
IAM's explicit authority; Honeycomb validators act only after provider approvals
are accepted. Denials require a reason. The console's Review requests page provides
the same discussions and decisions. Publication remains pending until IAM accepts
activation and archive access is reconciled.

## Approve webhooks and rotate signing material

Use the console's **Access & secrets → Webhook management**, or the same workflow
from the CLI. You must be a current owner/admin of the application's organization.

```sh
honeycomb apps webhook status 'tos>example'
honeycomb apps webhook approve 'tos>example' --endpoint PENDING_ENDPOINT_UUID --revision 1 --step-up-file /secure/path/assertion
honeycomb apps webhook rotate-secret 'tos>example' --revision 1 --secret-file /secure/path/new-signing-secret --step-up-file /secure/path/assertion
honeycomb apps webhook retry OPERATION_ID --step-up-file /secure/path/fresh-assertion
```

Read the current application revision with `honeycomb apps get`. The status command
returns IAM's exact pending endpoint ID and internal resource ID. Obtain an IAM
verified-channel step-up assertion bound to that resource and the appropriate action:
`application.webhook.approve` or `application.webhook_secret.rotate`. A normal login
token is not a step-up assertion. Keep assertion and signing-secret files private.
The signing secret must contain 32–4096 bytes without control characters.

A pending change returns an operation ID. Retry that ID with fresh verification;
you do not need to resubmit the signing secret. Honeycomb preserves the exact IAM
request, encrypts stored signing material and does not persist verification proofs.
After rotation, update the receiving application's secret configuration. Honeycomb
keeps its encrypted copy current so a later configuration save cannot reinstate the
old secret. IAM controls the signing-key overlap policy.

The stateless Rust client exposes `webhook_state`, `approve_webhook`,
`rotate_webhook_secret` and `retry_webhook_operation`. These use the same durable
backend operations as the CLI and console. IAM 1.10's management routes currently
support production applications; isolated test-app administration remains a contract
gap. IAM's accepted record omits the active destination URL, so the console labels
its URL as requested and never presents it as verified active state.

## Track test activity and retention

```sh
honeycomb environments retention ENVIRONMENT_ID
honeycomb environments set-retention ENVIRONMENT_ID --days 60 --revision 1
honeycomb environments activity ENVIRONMENT_ID 'tos>example' --generation 1 --key-version 1
```

Application integrations can report with the current environment root key through
`Client::with_environment`, or the CLI's existing `--test` environment context.
A current environment manager can also report with their authenticated session.
Report actual use, not a periodic idle heartbeat. Reports use server time and must
include the current generation and key version. Reuse the mutation's idempotency key
for a retry: replay returns the original timestamp without extending inactivity.
A report cannot create an application link or reach a different environment.

Set `testing_idle_days` in application configuration (default 30, range 1–36500).
The console exposes this beside the other application settings. Imported apps use
their production organization's accepted retention policy. The environment's own
idle period is configurable in **Testing environments → Manage → Activity & retention**.
Changes require the current environment revision and no pending lifecycle operation.

The retention view includes per-app activity, deadlines and transitive dependency
protection, including dependency cycles. An idle provider remains protected while
an active app requires it; longer app retention extends the shared environment's
deadline. A worker checks due actions every minute. It stores automatic app-retirement,
soft-delete and expiry-purge jobs durably, retries incomplete service work with
backoff, and exposes progress here and in the console. Soft deletion starts its
30-day recovery window only after every participant confirms disabled access.
Retirement removes only the selected apps and their owned test data; active apps
and required dependencies remain available. Cleanup itself does not refresh activity.

Participating services must implement the protected automatic-retention transport.
The current IAM adapter has not yet mapped that cross-service contract: these jobs
remain visibly pending until it is available. The worker never borrows a user session
or treats missing service receipts as success. Explicit lifecycle commands remain
available for retry and recovery.
