# Honeycomb 0.3.3 verification — September 23, 2026

## Published and deployed

Release source is `8660b2bc0d73b5576e9167d4cc9c8e0eef9608d5`, tagged `v0.3.3`.
The imported testing-application revision and rotation recovery fixes are
published and deployed to the production backend. After IAM 3.0.2 was verified
live, SSM command `17593180-e1e2-46f0-ba4b-6daad01ba536` successfully replaced
only `honeycomb-backend` on host `i-06986627793021fb2` in `us-east-2`.

The verified Linux ARM64 image is
`234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:74f61449a04a38b9f93bcf88c9c8c81fadf48fbaf9835d80ee9a6ffb14567fa6`.
Its source revision label matches the release. The remote OCI index, ARM64
manifest and configuration digest were checked before deployment. Fresh AWS SSO
and ECR authentication resolved the earlier expired-login push failure.

Public `/health`, `/api/v1/iam` and `/api/contracts` returned 200 at
`2026-09-23T07:21:25Z`; IAM metadata reported Honeycomb 0.3.3 and `tos>honeycomb`.
The existing SQLite database passed integrity checks before and after rollout.
A consistent backup remains on the host at
`/var/lib/silicon-honeycomb/backups/release-0.3.3-20260923T072046Z/honeycomb.db`;
the stopped prior container is retained for rollback.

All three environment files, Caddy configuration and start script retained their
hashes. Library, console and proxy container IDs, start times, persistent mounts,
and restart policies remained identical. Their encrypted web session storage was
not replaced. Docker is enabled and the new backend uses `unless-stopped`.

## Live credential recovery

In task environment `1d32b4c6-dc84-4c44-b7b7-a16be7a31d06`, a new authorized
Ting rotation `3b01b022-ca4b-4610-92e2-abdd49f8b81f` was accepted as credential
version 3. The official IAM Rust SDK 3.1.0 then performed a fresh DM Carbon login
and a fresh signed `subscriptions.register` exchange. The audience credential
returned by that exchange matched the new Ting credential and authenticated
IAM's testing-context endpoint for the exact environment and `tos>ting`. Its IAM
key also matched. The diagnostic proof was neither consumed nor persisted, and
no alternate credential was substituted.

The older operation `a2b700ff-90dd-4795-b6c1-27e98871ba9c` in environment
`d70c8674-6d2e-41d4-bf8d-96ddd882edbd` was recovered on 2026-09-23 after IAM
3.0.3 source `9fd370c5589e099399a377e5667b9408cc2d13fd` was verified live on
both main and scoped APIs with a healthy worker. Supported exact-operation
recovery under the original `dm-ting-tester` actor returned `rejected` with
`testing_configuration_revision_conflict`; a fresh read confirmed the terminal
state. Its immutable environment revision remained 19 while IAM was at 21,
with generation and key version still 1. Receipt-first recovery reached the
authoritative rejection without authorizing a new mutation under stale lifecycle
state.

Supported reconciliation then returned accepted. A new authorized rotation with
a new saved idempotency key, `344d03bc-1d3d-45f7-a363-42b1464088ce`, was accepted
as credential version 2. A fresh official IAM Rust SDK 3.1.0 DM login and signed
OBO exchange returned exactly that new Ting credential and the matching IAM key.
The returned credential authenticated IAM's testing-context endpoint for the
original environment and `tos>ting`. No saved request or database state was
manually changed; no alternate credential was substituted. The diagnostic proof
was neither consumed nor persisted. The earlier generic revision-conflict
recovery attempt under IAM 3.0.2 remains retained as historical evidence.

Sanitized current proof is in
`credential-recovery/original-obo-validation-iam303.json` under the task evidence
directory. Both original-operation recovery and fresh-environment audience
credential consistency are now verified; these checks do not alone establish
DM message delivery.

## Published artifacts and checks

- [Source CI 35791473082](https://github.com/teamofsilicons/silicon-honeycomb/actions/runs/35791473082)
  passed Rust, website and documentation checks. Local server validation passed
  150 tests and workspace Clippy with warnings denied.
- [Native CI 35791506669](https://github.com/teamofsilicons/silicon-honeycomb/actions/runs/35791506669)
  passed tests, Cargo source installation and release builds on all six targets.
  [GitHub v0.3.3](https://github.com/teamofsilicons/silicon-honeycomb/releases/tag/v0.3.3)
  contains six native archives and six checksum sidecars. Archive checksums and
  binary architectures were reviewed; the macOS ARM binary and license ran locally.
- Core, client and CLI crates 0.3.3 were packaged, verified and published in
  dependency order. Registry availability and a fresh isolated CLI installation
  from crates.io were verified.
- Production catalog release `f1b4a19b-53c8-4c9d-a8ce-293db30b0724` is accepted;
  publication `ecbd9d5b-919f-4719-b305-b333ed636317` remains published against
  accepted application revision 1. The all-target archive is 20,043,776 bytes,
  SHA-256 `fcbc0f345956143a979a3ed3294fa774db2750967efa5f9dec931d594a11a4ea`.
- [Recovery documentation](https://docs.honeycomb.teamofsilicons.com/operations/)
  was deployed and verified publicly after its build and 12 browser checks passed.

## Installation and retained state

A fresh anonymous catalog installation ran CLI 0.3.3. The normal zsh shell
installer also passed using isolated `SILICON_HOME` and `ZDOTDIR` directories.
The installed global CLI was upgraded through its supported `self-update`
command; it reports 0.3.3 and the original saved Carbon login still authenticates.
All three existing Honeycomb updater configuration files retained their hashes.

One preexisting installer edge case remains: on macOS Bash 3.2,
`HONEYCOMB_NO_MODIFY_PATH=1` prints an empty-array `unbound variable` diagnostic
after binary activation, despite returning exit status zero. Normal zsh
installation passes. This was not repaired by rewriting published artifacts.

Nonsecret evidence is retained under `/tmp/ting-rotation-release-20260923/`,
including `honeycomb-release-status.json`, `honeycomb-install-verification.json`,
`honeycomb-native-artifact-review.json`, `honeycomb-crates-live.json`,
`honeycomb-catalog-publication.json` and `honeycomb-docs-live.json`.
