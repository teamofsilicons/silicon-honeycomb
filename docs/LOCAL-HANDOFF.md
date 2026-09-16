# Checkpoint — 2026-09-16

Honeycomb 0.1.1 is published on GitHub and crates.io. The production backend
includes official IAM 1.11 integration, publication and testing lifecycle adapters,
isolated testing login, strict participant payloads, and actionable capacity errors.

## Verified in production

- Signed-in Carbon CLI authentication and authenticated console access.
- Explicit Saket validator assignment and IAM-confirmed reviewer eligibility.
- Fresh private `tos>iam` registration, IAM 1.11.0 archive upload, private installation
  and executable version check.
- Normal Briefcase revision-2 publication plan, validator decision and activation;
  anonymous Briefcase 1.1.0 installation, checksum and executable version check.
- Anonymous library shows Briefcase while private IAM remains in the authenticated
  console only.
- Shared disposable environment creation and Briefcase import, then root-key
  rotation, clean, recoverable deletion and restoration across IAM, Honeycomb and
  Briefcase. Every operation was accepted with matching participant receipts.
- Public GitHub curl/bash and crates.io installation of Honeycomb 0.1.1 on macOS
  ARM64, including version, license, updater, authentication status and daemon opt-out.

The ten legacy Briefcase capacity records were recoverably disabled with explicit
user authorization. Nine were already deleted in IAM; the remaining active IAM
environment was adopted and disabled through the official API. Existing keys and
data were retained. The disposable acceptance environment was deleted after testing. Final read-only
checks confirmed zero active IAM and Briefcase environments.

## Validation and provenance

Rust workspace tests, strict Clippy, dependency policy, official SDK source hashes,
CLI fixture journey and all 46 desktop/mobile browser tests passed. All six native
release-candidate jobs passed. GitHub binaries were built at `c6a51bf`; subsequent
changes affect the backend or test fixtures, not distributable crates. Main CI at
`2172728` and `26f53a9` passed; their fixture correction is newer than the binary
source commit. The final backend commit `7031dda` passed all 104 local server
tests and strict Clippy; its hosted verification was still running at handoff.

See [the resumed release record](../deploy/aws/release-2026-09-16-resumed.md) for
operation IDs, deployment provenance and limitations. Human-owned
[UNDERSTANDING.md](../UNDERSTANDING.md) remains unchanged.

## Acceptance limits

The live shared-lifecycle pass does not establish every application-owned
create/attach path, automatic retention/purge, notification delivery or migration
of all legacy environments into Honeycomb's catalog. Legacy writer cutover remains
disabled. The previously reported Browser 0.2.3 archive was not available locally,
so its exact-byte upload retry was not performed. Fixtures and targeted wire tests
cover additional cases; they are not presented as live acceptance.
