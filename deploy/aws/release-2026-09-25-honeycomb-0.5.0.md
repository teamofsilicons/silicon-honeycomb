# Honeycomb 0.5.0 deployment — September 25, 2026

Source `7403c6d76083d0ad89c1ae64d97f6f7583bfd826`, tagged `v0.5.0`, adds package
install scripts. Each target in `honeycomb.yaml` may declare an optional
`install_script`, which `honeycomb install` runs once the installation has
completed on that platform. Updates and automatic updates never run it. A failing
script keeps the installation and exits 1; `--skip-install-script` opts out.
[PR #7](https://github.com/teamofsilicons/silicon-honeycomb/pull/7) was merged by
fast-forwarding `main` to the release commit, so the tag, native builds, crates
and backend image share one revision.

Clients older than 0.5.0 reject manifests carrying `install_script`. The backend
had to run 0.5.0 before such packages could be uploaded; it does now.

## Production deployment

Backend deployment through SSM command `d4a2fbe9-6293-4789-99ec-3b09bf9356f9`
succeeded on `i-06986627793021fb2` in `us-east-2`. It replaced the backend
running `33c57ee` (the validator-gate fix, which this release contains). The
immutable ARM64 image, tagged `0.5.0-7403c6d`, is:

`234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:009de491ec6530a49edb8207883630edcdb09407c2931b64c82d4074bcd10ee6`

The running image's source label matches the release commit. There is no new
migration; the schema remains at 27. SQLite integrity passed, preserving 20
applications and 81 releases, all public. A consistent pre-rollout backup
remains at
`/var/lib/silicon-honeycomb/backups/release-0.5.0-20260925T102122Z/honeycomb.db`.
The stopped prior container is retained as `honeycomb-backend-before-050`.
All three service environment hashes and the library, console and proxy
containers were preserved. Follow-up SSM command
`b69b2057-ce44-4281-8a23-963a3ed006a7` verified foreign keys, no pending public
app configurations, and logged Docker out of ECR.

Public `/health`, `/api/v1/iam`, `/api/contracts` and the v1/v2 release lists
returned 200; `/api/v1/iam` reports 0.5.0 and `/api/v1/cli/latest` reports
`v0.5.0` with all six targets.

The documentation was promoted to production on Vercel as
`B8DnCVxUgaBvn9HkKmvG9AEkn1q8`; the package format page carries the new Install
script section and the CLI reference lists `--skip-install-script`. The console
and library have no changes in this release and were not redeployed.

## Published artifacts and verification

- [Source CI](https://github.com/teamofsilicons/silicon-honeycomb/actions/runs/36122578087)
  passed Rust, website and documentation checks for the exact source commit,
  including the new `scripts/test_install_script.py`.
- [Native release CI](https://github.com/teamofsilicons/silicon-honeycomb/actions/runs/36122633790)
  passed workspace tests and release builds on all six platforms, the first
  execution of the Windows install-script path. The
  [GitHub release](https://github.com/teamofsilicons/silicon-honeycomb/releases/tag/v0.5.0)
  contains six native archives and six checksum sidecars. All archive hashes and
  executable architectures were verified; the macOS ARM64 binary ran locally.
- Core, client and CLI crates 0.5.0 were published in dependency order. A fresh
  isolated installation from crates.io reports 0.5.0 and exposes
  `--skip-install-script`.
- Catalog release `4f67b435-a147-4dc6-9135-cdfd11357d24` was accepted against
  configuration revision 1 with public visibility and no additional approval
  required. The archive is 20,118,871 bytes, SHA-256
  `e8311921d7dbd2749773e76ab4074dacfbbcf1b93fabb48bf67fb13c50ae134c`.
  Publication `ecbd9d5b-919f-4719-b305-b333ed636317` remains published.
- A fresh anonymous catalog installation into an isolated `SILICON_HOME`, using
  alias `honeycomb-050-smoke`, installed and ran 0.5.0.
- Local verification included 209 workspace tests, Clippy with warnings denied,
  formatting, the generated reference check, the documentation example check,
  and the identifier, IAM import, legacy adoption, CLI, installer, progress,
  install-script and release-channel integration scripts.

No production application declares an install script yet; live checks verify
deployed code, preserved data, and public installation. Nonsecret working
evidence is retained under `/tmp/honeycomb050/`.
