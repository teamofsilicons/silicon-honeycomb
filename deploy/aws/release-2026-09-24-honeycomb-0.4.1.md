# Honeycomb 0.4.1 deployment — September 24, 2026

Source `b97092eb1cb097e579a5b14fddd417f24e14210e`, tagged `v0.4.1`, adds
per-release visibility and holds releases that need permission approval. Existing
installations keep receiving the last public release until the matching
configuration is accepted and archive publication completes. Denied and
superseded releases remain private. IAM retains runtime permission and consent
authority.

## Production deployment

Backend deployment through SSM command `98da1c7c-8bd1-41ea-904a-e76ee9a9cac0`
succeeded on `i-06986627793021fb2` in `us-east-2`. The immutable ARM64 image is:

`234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:49158477230ce8326de070fae02e60b123ff943b2396477d25f8855fc23e7686`

The running image's source label matches the release commit. Migration 27 passed
SQLite integrity and foreign-key checks, preserving 20 applications and 76
releases at deployment: 70 public and six private. A consistent pre-migration
backup remains at
`/var/lib/silicon-honeycomb/backups/release-0.4.1-20260924T093300Z/honeycomb.db`.
The stopped prior container is retained as `honeycomb-backend-before-041`.
All three service environment hashes and the library, console and proxy
containers were preserved. Follow-up SSM command
`c6612e45-9ba5-41c6-a8f9-eb2b5eda41b8` verified runtime state and logged Docker
out of ECR.

The console, library and documentation were promoted to production on Vercel:

- Console: `32BdKqHZ46J5fb4i3gobtmj7qaF1`.
- Library: `24Ct7MjJtSn9sAPanhRtsiP3Qtdw`.
- Documentation: `Cs7zc4PteYZtCTW2EF13gikE96T6`.

Public checks passed for backend health and version 0.4.1, all three site
origins, website API configuration and anonymous sessions, compiled release
approval messaging, and the release-visibility documentation. Anonymous v1/v2
release lists expose public releases with configuration revisions; authenticated
manager history accepts `--include-private`.

## Published artifacts and verification

- [Source CI](https://github.com/teamofsilicons/silicon-honeycomb/actions/runs/35981202540)
  passed Rust, website and documentation checks for the exact source commit.
- [Native release CI](https://github.com/teamofsilicons/silicon-honeycomb/actions/runs/35981203266)
  passed all six platform builds. The
  [GitHub release](https://github.com/teamofsilicons/silicon-honeycomb/releases/tag/v0.4.1)
  contains six native archives and six checksum sidecars. All archive hashes and
  executable architectures were verified; the macOS ARM64 binary ran locally.
- Core, client and CLI crates 0.4.1 were published in dependency order. A fresh
  isolated installation from crates.io reports 0.4.1 and exposes the new
  `--include-private` option.
- Catalog release `2cffa09c-0f37-4c47-9e49-9907ea25d8f0` was accepted against
  configuration revision 1 with public visibility and no additional approval
  required. The archive is 20,054,941 bytes, SHA-256
  `993ded059ca69bc35d7e3eb8fe074a2bbfd00f9047956dd68e043e9dc125d43d`.
  Publication `ecbd9d5b-919f-4719-b305-b333ed636317` remains published.
- A fresh anonymous catalog installation into an isolated `SILICON_HOME`, using
  alias `honeycomb-041-smoke` to coexist with the existing system command,
  installed and ran 0.4.1. The live CLI update endpoint reports 0.4.1 and all six
  targets.
- Local verification included workspace tests, Clippy with warnings denied,
  formatting, ten release-channel integration tests, 60 browser tests, 12
  documentation tests, 12 identifier/migration tests, and actual CLI channel
  update coverage.

Permission approval, rejection, stale revisions and archive-publication retry
were verified through regression tests. No artificial permission change was
submitted to a production application; live checks verify deployed code,
migrated data, publication of an unchanged approved configuration, and public
installation. Nonsecret working evidence is retained under `/tmp/honeycomb041/`.
