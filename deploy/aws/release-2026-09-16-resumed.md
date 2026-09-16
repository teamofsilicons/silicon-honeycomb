# Resumed release and live acceptance — 2026-09-16

## Source and distribution

- `c6a51bfbdc09c45ba759b519987d07562e885dfa`: isolated IAM testing login,
  constructor wiring, webhook validation and safe upstream diagnostics.
- `096874589b8ae9f722d2f86732a50eaa9840ef34`: strict external participant request projection.
- `2172728772d46d59f078385b1c894f174d0cbeab`: valid webhook fixture subscriptions.
- `26f53a938f8d70b6baca69da86f04b6d42570bff`: safe actionable participant capacity error.
- `7031dda7566964023c5011fd55eb1fea9c70357a`: retain cleaned service participants
  through later rotation, deletion, recovery and purge.
- GitHub [Honeycomb 0.1.1](https://github.com/teamofsilicons/silicon-honeycomb/releases/tag/v0.1.1)
  contains six native archives and their SHA-256 files. Native candidate run
  `35072750967` passed all six platforms at `c6a51bf`.
- Core, client and CLI 0.1.1 are published on crates.io. Public curl/bash and
  crates.io installations passed on macOS ARM64, including version, license,
  updater, unauthenticated status and daemon opt-out in disposable homes.

All distributable crate files and Cargo manifests/lockfile are unchanged between
binary source `c6a51bf` and backend commit `7031dda`. Main verification run
`35073668062` passed at `2172728`; `35074961515` passed at `26f53a9`. The tag's source predates the CLI/browser fixture
correction, so its verification rerun fails those stale fixture inputs; the six
native candidate jobs and corrected main checks are the release evidence.

## Backend deployment

Final image:
`234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:8bbbc53890139224e21daeeb26dbda41547c67b24a5092a9bcad53219396d196`.

SSM rollout `62ea73e0-49d2-482d-adf3-0b172afd1d3a` succeeded on
`i-06986627793021fb2` in us-east-2. SQLite integrity checked backup:
`/var/lib/silicon-honeycomb/backups/resume-clean-participants-20260916T084652Z`.
Existing application, service and encryption credentials were preserved. The
library, console and proxy containers were unchanged. Rollback retained the
previous image, environment and consistent database snapshot.

## Publication and package acceptance

The user approved assigning Saket as validator and creating private `tos>iam`.
IAM reviewer eligibility confirmed `can_decide: true` after the audited grant.
IAM registration revision 2 completed as operation
`179ddbb1-09b9-45dd-949c-6dc5b770c87c`; a real IAM 1.11.0 six-target archive was
uploaded and installed privately, with its executable version checked.

Briefcase's legacy direct publication was superseded through a normal revision-2
configuration update and review, without inventing an IAM approval:

- Request `b7b54fae-0e1d-4c2b-8d00-b48b4007a373`.
- IAM plan `c20143f6-f1c3-4dc9-976b-e07235a8c7a6`.
- Validator decision `7bd30c64-6040-4302-b224-597b4724edb7`.
- Activation `38f6bf25-6840-43fb-bd10-704137abeb0d`.

Briefcase 1.1.0 was anonymously installed, checksum verified and executed. The
anonymous live library displayed Briefcase while IAM remained private in the
signed-in console.

## Shared testing acceptance and capacity cleanup

Environment `687242c4-0407-4674-b46b-230d684f34fa` used real IAM, Honeycomb and
Briefcase participants. Its interrupted import was retried under the original
operation `99927d09-4592-470d-9a48-f49ffb262166` after resolving two failures:

1. Briefcase rejects IAM-only metadata in the strict participant payload. The
   backend now sends only the participant contract.
2. Ten older Briefcase records occupied its active capacity. Nine corresponding
   IAM environments were already deleted. The user explicitly authorized deleting
   all testing environments. Protected adoption export preserved original keys;
   Briefcase prepare/disable receipts completed for all ten. The one active IAM
   environment was adopted and disabled through the official management API.
   No keys, data or application credentials were purged or replaced.

The first clean exposed a participant-retention defect: imports were correctly
removed, but external service links were also removed. Later deletion therefore
left Briefcase capacity active even though core participants accepted deletion.
Commit `7031dda` retains the links while clearing imported configuration. A new
regression verifies every participant receives delete and restore after cleaning.
The environment was restored and Briefcase reimported through normal APIs for
the final three-participant verification.

Initial sequence (delete/restore here covered core participants only):

| Action | Operation | Result |
| --- | --- | --- |
| Import Briefcase | `99927d09-4592-470d-9a48-f49ffb262166` | ready, revision 2 |
| Rotate root key | `803e7f46-085d-403c-9b07-88c89573899c` | ready, revision 3, key version 2 |
| Clean | `dd1033ec-442a-460a-848f-19176371e912` | ready, revision 4, generation 2 |
| Delete | `b6ac6236-df85-4b55-8889-4c7115b75804` | deleted, revision 5 |
| Restore | `d6d291b7-fdde-46d5-b307-c0fca168b110` | ready, revision 6 |

The final corrected sequence and capacity counts are recorded below. Private export,
request and receipt journals are stored under the operator's protected
`~/.config/silicon/honeycomb` directory and are excluded from Git. Legacy recovery
uses the documented IAM and Briefcase protected restore APIs with the unchanged
root and increasing lifecycle revision; these records have not been migrated
into Honeycomb's end-user catalog.

## Validation limits

Workspace Rust tests, strict Clippy, dependency policy, SDK provenance checks,
CLI journey and all 46 desktop/mobile browser cases passed. The final server changes additionally passed all 104 server tests and strict
server Clippy, including the cleaned-participant deletion/restoration regression.

This live pass covers the shared control plane and public/private distribution.
It does not establish all application-owned create/attach flows, automated
retention/purge, live notification delivery, or complete legacy catalog adoption.
IAM's legacy-writer cutover remains disabled. The exact Browser 0.2.3 archive was
unavailable, so that earlier exact-byte upload retry remains unverified.

## Final corrected lifecycle verification

All three participants remained linked after cleaning. Each operation below
completed with matching IAM, Honeycomb and Briefcase receipts, and no pending
operation. The last deletion leaves the disposable environment recoverable.

| Action | Operation | Revision | Generation | Key version |
| --- | --- | --- | --- | --- |
| clean | `1404d6cc-49ba-4b1d-bbaa-a44ce3dd6ef6` | 10 | 3 | 2 |
| rotate-key | `85fccc0f-e27c-4ab2-8ac6-0eafa7b98a98` | 11 | 3 | 3 |
| delete | `1399602f-267e-47c0-8aa2-8b1becd81edc` | 12 | 3 | 3 |
| restore | `74cc69b7-bd91-4c06-83b7-cdfa6443c180` | 13 | 3 | 3 |
| delete | `0ffd1ad8-f491-4681-9560-52f8e007df91` | 14 | 3 | 3 |

Final read-only capacity checks confirmed **0 active IAM environments** and
**0 active Briefcase environments**, with 11 Briefcase service records retained
as disabled. SSM checks: `dec7255a-a1bb-4b65-8f51-0f53e9968ba1` (IAM) and
`5d6d53c5-d5b7-4a51-88a1-2932dacb8e68` (Briefcase). Both succeeded.

Backend and both public sites returned HTTP 200 after the final rollout.
Hosted verification run `35075522203` was still running at handoff; final local
server tests and strict Clippy passed.
