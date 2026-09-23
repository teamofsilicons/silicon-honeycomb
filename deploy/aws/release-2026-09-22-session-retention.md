# Session retention deployment — September 22, 2026

## Result

The library and console gateways are deployed with durable refresh recovery. CLI,
core and Rust client 0.3.2 are published from
`3be8bb32b852cc98a0dbfbfb4cf85f0f07b8c516`. The saved CLI session is checked before
an authenticated command; early access rejection renews once under the shared
session lock. Refresh retry keys, request timestamps and received successor
credentials survive interrupted commands. User commands execute once. Temporary
service failures preserve saved credentials.

## Runtime and retained sessions

Web source `fe028a848ee6be76f1377d758fdef2571449b20e` is deployed on
`i-06986627793021fb2` in `us-east-2`, using image
`234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-web@sha256:510f221b1592adedc92806dffcb2558c832c23dcb9765374f0c9ea1725cded29`.
SSM `4cc56305-e66c-4b6c-a23a-d96b55a88c5f` succeeded. Only the two gateways were
replaced; the backend, encryption keys, persistent session mounts and all three
environment files retained their previous values. Both gateways were running
with zero restarts after rollout. The quiesced backup is
`/var/lib/silicon-honeycomb/backups/session-fe028a8-20260921T222951Z` and previous
containers remain available for rollback.

A normal fresh Carbon login loaded all 13 applications in `tos`. Restarting the
console preserved the same browser cookie and the same application reads. The
first restart check raced startup readiness; the subsequent readiness and browser
checks passed. The independent verification session was logged out normally.

The standalone global CLI now reports `honeycomb 0.3.2`. Its original saved Carbon
`saket` login still authenticates for `tos` without a new sign-in. The existing
launchd updater is running from the stable installed system binary. Native macOS
ARM binary SHA-256:
`afa1cfa03a68520dea8951bce0399e39ea0c1ceacc401926290d36cedc0cf8db`.

## Distribution and validation

[Source CI](https://github.com/teamofsilicons/silicon-honeycomb/actions/runs/35666128607)
and [six-platform native CI](https://github.com/teamofsilicons/silicon-honeycomb/actions/runs/35666207661)
passed, including workspace tests and Clippy. Process regressions cover concurrent
refresh, durable pending refresh, logout races, early 401 and outage retention.
Core, client and CLI 0.3.2 were published with Cargo verification; downloaded crate
bytes match the local publication artifacts and source revision.

[GitHub v0.3.2](https://github.com/teamofsilicons/silicon-honeycomb/releases/tag/v0.3.2)
contains six native archives, six checksum sidecars, the catalog archive and its
checksum. All 14 assets were downloaded anonymously and matched local hashes.
Native file formats and CPU architectures were checked; the macOS ARM executable
was executed and its license matched the repository. Linux retains the existing
Ubuntu 24.04/glibc 2.39 baseline.

Honeycomb production release `9abaf81f-fff6-4775-86a7-799020b72004` is accepted and
publication `ecbd9d5b-919f-4719-b305-b333ed636317` is published. Catalog archive:
20,043,778 bytes, SHA-256
`a4bfaccbcd81fd3e15216044a093989c6fb82ef64d6d981f4807035c91e02c9c`.
A fresh anonymous Honeycomb installation downloaded this exact archive and ran
CLI 0.3.2 with the same native binary hash. Documentation built 26 pages and was
deployed; the public installation page returns 200 and identifies 0.3.2.

Nonsecret machine-readable evidence is retained under
`/tmp/session-release-20260922/`: `honeycomb-032-public-assets.json`,
`honeycomb-032-anonymous-proof.json`, `honeycomb-032-global-proof.json`,
`honeycomb-032-docs-public.json`, `honeycomb-032-verification.json`,
`honeycomb-live-browser-proof.json` and `honeycomb-deploy-result.json`.

## Local workspace limitation

The original Documents checkout preserves the human-owned requirements changes.
Final receipts were committed from a clean temporary clone because macOS blocks
content reads/opens under Documents. The deployment and global CLI checks above
completed independently. Maharaj's separate installed applications have their
own activation receipts; this release does not imply their final local upgrades
completed while that host access issue remains unresolved.
