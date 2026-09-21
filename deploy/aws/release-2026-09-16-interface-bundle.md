# Interface bundle through Honeycomb — 2026-09-16

Honeycomb now exposes authenticated bundle discovery and revision-checked configuration through its backend, Rust client, CLI and console. IAM's existing protected management API remains the runtime authority and independently validates actor ownership and organization eligibility. Desired requests and operation identities are persisted before submission; uncertain results are replayable with the same key. Current accepted IAM records remain visible, including legacy bundles.

## Deployment

- Backend image: `234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:0e012e12b60f825292ffd84057f5b5cba49ad6e32f6b67c55801783ad8628e87`.
- Backend SSM rollout: `b3f8982c-64f6-484b-b0cd-4e789ff01c0c`, successful.
- Consistent, integrity-checked SQLite backup: `/var/lib/silicon-honeycomb/backups/interface-bundle-20260916/honeycomb-1789582552.db`.
- Retained previous container: `honeycomb-backend-before-interface-bundle-1789582552`.
- Migration 0021 adds bundle request persistence and excludes overlapping pending configurations. Existing application data, environment keys and web sessions were preserved.
- Console: `https://silicon-honeycomb-console-4bra1tfch-saketdev12-5675s-projects.vercel.app`, aliased to `console.honeycomb.teamofsilicons.com`.
- Documentation: `https://silicon-honeycomb-docs-dqqa5hkwy-saketdev12-5675s-projects.vercel.app`, aliased to `docs.honeycomb.teamofsilicons.com`; 26 pages and 1,478 links/assets validated.
- No IAM backend deployment or Interface runtime deployment was needed.

## Accepted live configuration

The prior application reset removed the old member application identities, leaving `tos>interface` with zero members. Its immutable bundle UUID `8dea284f-a056-4615-b68e-9f64d1a22f09` was retained. IAM accepted operation `2678cc63-9f02-43a5-8318-9c55b670d7da`, configuration revision 1, IAM revision 3, with these ordered members:

`tos>iam`, `tos>dm`, `tos>briefcase`, `tos>commit`, `tos>remind`, `tos>waveform`, `tos>browser`, `tos>starter`.

Starter was absent from IAM. Its original configuration was read from the retained pre-reset backup without restoring the database. Honeycomb registration operation `85df6f97-3a92-4385-aeaa-3c228d183a75` restored its real name, website, webhook and previously declared IAM/Briefcase scopes. A fresh application credential was stored in Starter's existing Secrets Manager runtime, and its existing native backend loaded it through SSM `47883f8b-8d29-49ac-ac5b-abf527c4a819`. Readiness passed. Application data and webhook signing material were preserved.

Starter's new registration is **private to tos**. Its former public status was not silently reinstated: public availability requires Honeycomb's normal publication workflow. Consequently the complete bundle currently requires membership in tos.

## Validation

- `cargo test --workspace --locked` passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- Formatting and diff checks passed.
- Three backend bundle tests cover manager/tenant restrictions, invalid members, IAM rejection, stale revisions, idempotency, lost responses, incorrect receipts and pending-write serialization.
- Console build and desktop bundle-editor retry journey passed.
- Interface's 15 gateway tests passed, including the exact eight-member callback and Starter validation/discard behavior.
- Live anonymous bundle API request returned 401; authenticated CLI read returned the accepted eight-member configuration.
- Live Interface redirects to IAM with `bundle_id=tos>interface`, recognizes Silicon Interface and displays the permission-review screen. Completion of user consent and service exchange is pending; do not treat this as a completed authenticated end-to-end login yet.

The installed public CLI release is unchanged. Bundle commands were exercised using the locally built CLI; native cross-platform package publication remains separate.

## Follow-up: Starter removed

On 2026-09-17 (user timezone), the user canceled Starter publication and requested that Starter also be removed from Interface. Honeycomb operation `7efcf776-781b-4a99-85e2-0e801dd678fb` accepted configuration revision 2 / IAM revision 5. The bundle UUID remains `8dea284f-a056-4615-b68e-9f64d1a22f09`; a subsequent authoritative read lists exactly IAM, DM, Briefcase, Commit, Remind, Waveform, and Browser. `tos>starter` remains a private registered application. Its publication attempt failed the missing-release prerequisite; no review request or CLI upload was created.
