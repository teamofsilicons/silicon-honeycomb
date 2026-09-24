# Deployment and release preparation

Source is public under MIT, and GitHub release `v0.1.0` plus all three crates are
published. All six native release jobs passed. Public curl/bash and crates.io
installation were verified in isolated homes.

The user selected AWS for the backend, Vercel for both SolidJS frontends, and
Namecheap DNS. The dedicated AWS host is in `us-east-2` using the
`silicon-production` profile; see [aws/README.md](aws/README.md). Vercel projects
are `silicon-honeycomb-library` and `silicon-honeycomb-console`. The requested
three DNS records are configured. Deployment alone does not prove live IAM or
Briefcase acceptance; Briefcase setup is deferred until the base is ready.

IAM's management API and `honeycomb` authentication app are deployed.
Protected service reads pass. The IAM handoff is outside Git at
`/Users/codanium/.config/silicon/honeycomb/iam-production.env`; complete runtime
settings are in the AWS Secrets Manager runtime secret. Keep IAM's legacy-writer
cutover and scheduled-testing switches disabled until replacement flows are verified.
See `docs/IAM-CONTRACT-REVIEW.md` for required scopes and outstanding contracts.

## Containers and requested domains

```sh
docker build -f deploy/Dockerfile.backend -t honeycomb-backend:local .
docker build -f deploy/Dockerfile.web -t honeycomb-web:local .
python3 scripts/test_containers.py
```

Both images run as non-root users. SQLite data belongs on persistent local volumes;
run one instance of each service with this deployment. A horizontally scaled
backend requires a different database/deployment design. Back up the databases
consistently, along with the encryption keys stored separately in your secret
manager. Losing an encryption key makes stored credentials unrecoverable.

The production layout routes:

- `honeycomb.teamofsilicons.com` → Vercel library assets
- `console.honeycomb.teamofsilicons.com` → Vercel console assets
- `backend.honeycomb.teamofsilicons.com` → Rust backend plus `/web/library/*` and
  `/web/console/*` persistent session services, selected by Caddy

For an alternative self-managed container deployment, provide
`backend.env` from the root example plus separate `library.env` and `console.env`
with distinct `WEB_SESSION_KEY` values (64 hex characters). These files are ignored
by Git. Caddy obtains HTTPS certificates; website backend calls use the HTTPS
backend hostname. The production web server rejects cleartext non-loopback API
origins. Register both `/auth/callback` URLs with IAM and provision the app secret,
webhook key and delegated Briefcase scopes. IAM's protected management integration
is also required before real applications can activate.

Once host/DNS/secrets are ready, `docker compose -f deploy/compose.yaml up -d --build`
is the deployment action. Run the live acceptance checklist in `docs/IAM-HANDOFF.md`
before opening the service to users. Local fixture identities must never be used
as a production deployment.

## Release candidates

`.github/workflows/release.yml` builds and runs native Rust tests on all six required
OS/architecture combinations, smoke-tests each CLI and produces a tar.gz plus SHA-256
file. The matrix uses GitHub's documented [hosted runner labels](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
It is manually triggered and only uploads candidate artifacts. It does not publish
crates, create a public release or deploy a website.

After reviewing the candidates, publish a `v<version>` GitHub release containing
all six archives/checksums. The shell installer reads these exact filenames; the
updater compares semantic versions and verifies checksum, archive shape and binary
version before activation. Publish the repository's `install.sh` at the documented
raw URL. macOS signing/notarization and any Windows code signing require the
organization's signing identity before distributing signed builds.

Before publication, verify all three distributable crates together:

```sh
cargo package --workspace --exclude silicon-honeycomb-server --locked
```

Cargo 1.98 builds the packages against a temporary local registry, so this verifies
their packaged contents and dependency resolution before the first publication.
This check runs in CI. It does not publish anything or prove crates.io installation.

When repeating local verification after changing an unpublished version, use a
fresh `--target-dir` so Cargo gives its temporary registry a new identity. Otherwise
its immutable-version cache can reuse an older local archive with that same version.
All three distributable crates include the MIT license text. The vendored IAM SDK
is backend-only and retains the separate notice in `THIRD-PARTY-NOTICES.md`.

Crates must be published in dependency order (each with `--locked`):

1. `silicon-honeycomb-core`
2. `silicon-honeycomb-client`
3. `silicon-honeycomb-cli`

Run `cargo publish --dry-run -p <crate>` before each publication. The client and CLI
cannot resolve their registry dependencies until the preceding crates exist on
crates.io. Local Cargo source installation is tested independently. The server is
marked `publish = false`.

The native release matrix also checks isolated Cargo source installation on each
platform and the MIT notice embedded in each binary (`honeycomb license`). Source
publication and all six native hosted-runner verifications are complete.
Local verification covers native macOS arm64 CLI execution, Linux arm64 container
build/startup, all six archive mappings, and desktop/mobile Chromium journeys.
