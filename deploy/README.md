# Deployment and release preparation

These are reviewable local deployment artifacts. They have not been deployed and
no domains, credentials, remote repository or public releases were provisioned.

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

`compose.yaml` and `Caddyfile` route:

- `honeycomb.teamofsilicons.com` → library
- `console.honeycomb.teamofsilicons.com` → console
- `backend.honeycomb.teamofsilicons.com` → Rust backend

Before starting on an approved host, point all three DNS records at it. Provide
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

Crates must be published in dependency order (each with `--locked`):

1. `silicon-honeycomb-core`
2. `silicon-honeycomb-client`
3. `silicon-honeycomb-cli`

Run `cargo publish --dry-run -p <crate>` before each publication. The client and CLI
cannot resolve their registry dependencies until the preceding crates exist on
crates.io. Local Cargo source installation is tested independently. The server is
marked `publish = false`.

No cross-platform release job has run yet because this checkout has no Git remote.
Local verification covers native macOS arm64 CLI execution, Linux arm64 container
build/startup, all six archive mappings, and desktop/mobile Chromium journeys.
