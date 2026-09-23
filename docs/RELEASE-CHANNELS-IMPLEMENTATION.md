# Honeycomb requirements implementation — 2026-09-22

Scope: the current changes in the human-owned `UNDERSTANDING.md` covering command collisions, App Release, Auto Update, the organization-app OBO endpoint, and the shared minute updater. The document was reread and left unchanged.

| Requirement | Implementation | Verification |
| --- | --- | --- |
| Explicit dev/prod release type and independent numeric versions | `ReleaseChannel`, required upload channel in the v2 API/SDK/CLI/console, per-channel database keys and immutable upload reservations | `server/tests/release_channels.rs`, `web/tests/releases.spec.ts`, real browser registration/upload journey |
| Promote dev into a new prod version | Version required (or prompted in interactive CLI); source checksum verified; manifest rewritten; deterministic archive with unchanged payload bytes | Archive promotion test, promotion API replay/authorization tests, browser promotion journey |
| Existing releases remain production | Migration `0024_release_channels.sql`; installed records default missing channel to prod; legacy upload retries preserve their hash and storage filename | Migration and legacy pending/accepted upload retry tests |
| Preserve existing API clients | Channel-aware release operations use v2; v1 remains production-only with its original optional-channel and semver behavior; protocol negotiation and retirement are independent | Legacy upload/read tests, v2 channel/version validation, protocol and lifecycle tests |
| `org>app>test` and `@x.y.z` install syntax | Validated CLI selection separates base app identity, channel, and optional version; production remains default | CLI selection unit tests and `scripts/test_release_channels.py` |
| Confirm prod/dev replacement | Interactive confirmation in both directions; `--switch-channel` for automation; aliases permit coexistence | Real CLI prompt/decline, explicit switch, same-version channel switch, independent alias/update/uninstall tests |
| Updates follow each installed variant | Registry keys distinguish channels; updater uses each record's original backend/testing context; downloads verify channel and checksum | Real daemon updates both channels; Rust multi-context updater test retains failed installation |
| One daemon updates apps and Honeycomb every minute at second 01 | Wall-clock-aligned scheduling; per-minute throttling; CLI discovery cache at most 60 seconds; service supervision reloads an updated executable | Timing unit tests, real minute-boundary daemon test, CLI-discovery tests |
| App installs enable the updater by default | Successful and repeat installs register the per-user supervisor; failures remain visible without losing the package; opt-outs retained | Real CLI auto-registration with isolated supervisor double; managed invocation/idempotent service regression |
| Critical OBO inventory including private organization apps | `honeycomb.apps.list`, POST `/api/v1/obo/apps/list`; exact request-bound IAM proof verification, current membership, explicit safe projection, pagination and test-plane fencing | Nine OBO integration tests plus IAM SDK tests; registration fragment in `deploy/honeycomb-obo.json` |
| Command collision and rewrite behavior | New-enough external versions resolve; older/unknown versions require confirmation; rewrites retain original backup through updates/switches and restore removed commands | Client installer tests cover collision, rewrite, update, removed command, and uninstall restoration |
| Installed commands become discoverable | Automatic persistent shell setup, repeat-install repair, activation guidance for the existing terminal, JSON status | Bash/Zsh install tests including quoted homes, opt-out, testing isolation, and startup-file errors |

Validation completed locally:

- Rust workspace tests and strict Clippy; targeted installer/CLI tests rerun after final audit fixes.
- Core, client, and CLI source packages build together at version 0.3.0.
- Real CLI HTTP/database publication, install, update, and uninstall journey.
- Native installer, progress, release-channel, daemon timing, and legacy-adoption checks.
- 58 console/library desktop and mobile browser tests, frontend unit tests, and production build.
- 12 documentation browser tests, documentation build, generated references, and downloadable examples.

Promotion changes the Honeycomb release version in its manifest, not a version compiled into native executables. A promoted binary may therefore report its original development version with `--version`; rebuild/upload when executable version text must change.

This report records implementation validation; live release and deployment evidence is recorded separately under `deploy/`. The OBO registration fragment must be merged with Honeycomb's existing endpoint definitions when deploying; it must not replace unrelated definitions. Windows service registration was implemented and type-checked through the shared Rust code, but was not executed on a Windows host.
