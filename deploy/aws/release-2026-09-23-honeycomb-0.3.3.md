# Honeycomb 0.3.3 verification — September 23, 2026

## Publication complete; backend deployment pending

Release source is `8660b2bc0d73b5576e9167d4cc9c8e0eef9608d5`, tagged `v0.3.3`.
The imported testing-application revision and rotation recovery fixes are
published, but are **not deployed to the production backend**. A live
`/api/v1/iam` read at `2026-09-22T22:41:13Z` still returned version `0.3.0`.

AWS SSO requires renewal. One push to the existing ECR repository returned
`403 Forbidden`; no remote backend artifact was confirmed. The locally built
ARM64 image `silicon-honeycomb-backend:0.3.3` has the exact source revision label
and passed startup, health, version and contract checks. Deploy only after IAM
backend 3.0.2 is confirmed live, then verify recovery of the previously pending
rotation and fresh credential use. Those live recovery checks remain outstanding.

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
