# Honeycomb 0.3.0 release — 2026-09-22

## Source and components

Backend and website runtime images were built from `688a5c7fa086a549e7fb91de5b95e4eab412add2`. Native CLI candidates use `1677bb961b5341119b5a99de197e35a7266ac384`; subsequent commits fix managed CLI updates, avoid unnecessary one-shot daemon hashing, and disambiguate a browser-test locator. They do not change deployed backend or frontend runtime source.

- Backend image: `234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:b2df66a7634d6372c8df36915a171f571cec9b17ae9884ef42ffddf7cb864edb`.
- Session image: `234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-web@sha256:95906c3d85d5a3b6ae04a7bc3ba02ae77a6d65ed177481fc353f46eaea982979`.
- Rollout command: `c500681a-5614-44ac-b589-edb2fe88657f`, completed successfully.
- Recoverable backup: `/var/lib/silicon-honeycomb/backups/release-0.3.0-688a5c7fa086-20260921T200029Z`.

The rollout rehearsed migrations on a consistent SQLite snapshot before stopping writers. Backend, library sessions, console sessions, environment files, proxy configuration, and previous container definitions were backed up. All 25 migration records, database integrity and foreign keys passed; all 34 pre-existing release archives retained their metadata and became production-channel records.

## Runtime checks

Public backend version 0.3.0, active API v1/v2 contracts, both website session proxies, and legacy release reads passed. Authenticated organization inventory returned Honeycomb and a private app. An unsigned OBO inventory request returned 401.

Library deployment `dpl_D8eXBPsPByKpFKLQAKjAYuQuzFWE` and console deployment `dpl_42JZGYXkU5vpa9KDGeqb4E9yZbyS` were aliased to their production domains. Their live frontend asset SHA-256 matched the staged build: `897f9e11c468bd43a3f44483b92c5fc90b7b0e9375c78fff756aa79a3fbc50dd`. Desktop/mobile checks found no JavaScript errors or horizontal overflow; session configuration and v2 channel reads passed.

## Existing identity adoption

The guarded operator importer adopted canonical identity `tos>honeycomb`, preserving its pre-existing public/verified IAM state, configuration revision 0, IAM revision 6, credential version 1, and all granted scopes. It did not create an identity, rotate keys, or write IAM configuration.

- Adoption operation: `a404a931-9aa9-4e19-9a75-4ac56e65b2d4`.
- SSM command: `b6bf6359-90c0-4be4-8cf1-9cea85b768a5`.
- Consistent adoption backup: `/var/lib/silicon-honeycomb/backups/honeycomb-adoption-0.3.0/20260921T200341Z-2f780c6d-1479-414d-8044-097a69c1cda1.db`.
- Desired revision 1 operation: `abfa13a8-e2e2-4f48-9265-9d93e645bb0f`.

Revision 1 adds the backend base URL and critical `honeycomb.apps.list` endpoint, retaining the existing scopes and signing secret. IAM had no existing webhook destination; the configuration introduces Honeycomb's implemented receiver URL. Endpoint approval was accepted through normal verified-channel IAM step-up: operation `9eed0391-1344-48ec-aa95-54b5f22ae6e6`, endpoint `01a0c5a3-31b6-77b3-b9f8-a6bbe0976915`, IAM revision 15. Readback confirms no pending destination remains.

## Publication and final verification

[CI run 35649237172](https://github.com/teamofsilicons/silicon-honeycomb/actions/runs/35649237172) passed Rust validation, three frontend unit tests, 58 browser tests and documentation checks. [Native run 35649251964](https://github.com/teamofsilicons/silicon-honeycomb/actions/runs/35649251964) passed workspace tests, source installation, release compilation and native version/help/license checks on all six target runners.

[GitHub v0.3.0](https://github.com/teamofsilicons/silicon-honeycomb/releases/tag/v0.3.0) resolves exactly to `1677bb961b5341119b5a99de197e35a7266ac384`. All six archives and adjacent checksums were uploaded while the release was a draft, then published after verification. Every checksum, archive member, binary architecture, GitHub upload digest and public archive URL was verified. The live latest-CLI endpoint advertises 0.3.0 and six targets.

Core, client and CLI crates were published at 0.3.0 in dependency order after dry runs. An independent Cargo installation from crates.io passed version, exact MIT license and one-shot updater checks.

The production catalog release was accepted as operation `86f555fd-3578-4950-b2ce-2b4337ab5396`. Its six-target archive is 19,975,525 bytes with SHA-256 `bbd2c10bdfe21a6a85f93cafe0c52f02545daa1ac8b6c9147edb01facaf7c656`. The normal IAM publication plan required only the authorized validator gate; existing provider grants were already effective. Publication request `ecbd9d5b-919f-4719-b305-b333ed636317` reached `published` through accepted decision `dc637e9d-6f86-403f-99bd-633004f9f39a`.

Public and authenticated readbacks confirm `tos>honeycomb` is public/active with desired and effective revision 1 and latest production version 0.3.0. Protected IAM readback confirms the existing application credential version remains 1, all eight effective scopes remain granted, and `honeycomb.apps.list` is enabled/critical at `/api/v1/obo/apps/list` with a 60-second TTL.

An anonymous native installer and an independent anonymous catalog installation passed. A fresh zsh session finds the catalog launcher and runs 0.3.0; its executable matches the verified native Mac ARM payload. The packaged MIT license matches exactly. Direct self-update of the managed package is rejected and leaves the executable hash unchanged. The user's existing standalone CLI was upgraded to 0.3.0; its per-user launchd updater is running. A fresh zsh session also finds and runs the previously installed Waveform 0.3.1.

## Live OBO proof and isolated cleanup

A dedicated testing environment was created through normal APIs. IAM, Honeycomb and Briefcase imports were accepted. Only the isolated Briefcase requester was configured for the new endpoint; production consumers and grants were not changed.

A normal test application token and signed, exact-body-bound IAM exchange returned HTTP 200 with all three private test applications and only the six allowed projection fields. Proof replay, changed body, absent proof and a test proof without its testing header returned 401. An unselected organization returned 403.

Environment `f3e0b5ed-0652-494b-9433-f923149b3c74` was deleted at revision 6 under operation `ccbc7e44-65c8-4cfc-9196-ed541e21ee32`. IAM, Honeycomb and Briefcase acknowledged cleanup with no pending operation. The retired key is rejected by Honeycomb (409) and IAM (401).

## Final documentation and platform baseline

Documentation was deployed from `e642f0d34bdae0d72d56565d299075fe47bd7322` after correcting its version footer. Deployment `dpl_2PB3PySGF77377k9zRjqmWiJwzfG` is aliased to `docs.honeycomb.teamofsilicons.com`. All 26 live page hashes match their build, every footer reports 0.3.0, desktop/mobile page checks and search pass, and the public release link returns 200. The live JS SHA-256 is `6bc0ec2d60c590fd3e813d71f89ce7f366d8a3b4b052b225130be9eb8e27aeeb`.

Linux native archives use the Ubuntu 24.04 build baseline and require glibc 2.39 or newer. A Debian 12 execution check confirms that older baseline is unsupported. This requirement is included in the release notes; source installation is available for older compatible systems. No claim of older-distribution binary compatibility is made.

The human-owned `UNDERSTANDING.md` remains unchanged by this release work.


## Native archive checksums

| Target | SHA-256 |
| --- | --- |
| linux-x86_64 | `f8e450a899818cea4d68bc7303f4e11263a3caef1df5a592d905e1748ccd7f02` |
| linux-aarch64 | `2a8675041f0ee830170818f8ed01420e4306a46721556808065b18cae2f9220f` |
| macos-x86_64 | `b0e0ffcae662f4b9d3caada5250653add34e8e6f32651b161e180f0ff4ab7164` |
| macos-aarch64 | `4cf0fe89b25870c992664e2e8264ea6a3f06f7b3fce94abe5081334478782d6e` |
| windows-x86_64 | `35ed8efc94d32184754aae75d89efe42832828de47d3bdf1935827af3046dc89` |
| windows-aarch64 | `17e1c21a3c8c76c54aef1cb34597ef0f14f9d371a06919383ec1189c762439e2` |
