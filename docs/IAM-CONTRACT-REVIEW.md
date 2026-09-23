# IAM integration status

Contract updated 2026-09-23; live evidence below remains dated 2026-09-16.
This records implementation and verification separately;
local fixtures do not establish production readiness. Requirements remain in the
human-owned [UNDERSTANDING.md](../UNDERSTANDING.md).

## Official contract

Honeycomb vendors the official IAM SDK 4.0.0 working-tree snapshot based on
`8111d3206a4d86b3c562e5556af304d428b9fced`, including the public identifier
migration changes. Its source hashes, dirty-tree provenance and standalone
manifest are recorded in `vendor/silicon-iam-client/UPSTREAM.json` and verified by
`scripts/verify_vendored_iam.py`. This snapshot has not been published to crates.io.
Local verification does not deploy the new identifier contract.
The upstream contract is IAM's `docs/HONEYCOMB_INTEGRATION.md` and OpenAPI.

The production management credential is separate from acting-user authority.
Configuration and publication mutations use the protected management API plus
current actor authority. A local role flag never substitutes for IAM eligibility.
Service identities and participant destinations come from deployment configuration;
there are no privileged product handles in the dispatch logic.

## Live verification

The read-only official SDK check on 2026-09-16 confirmed Honeycomb's IAM record
is verified/public at IAM revision 6, with all required storage scopes effective:

- `self.identity.read`, `self.profile.read`, `self.membership.read`, `self.tags.read`.
- On the configured Briefcase provider: `briefcase.uploads.reserve`,
  `briefcase.uploads.commit`, `briefcase.files.read`, `briefcase.link_access.update`.

The current deployed Honeycomb backend is recorded in
[the configured-identities release record](../deploy/aws/release-2026-09-16-configured-identities.md).
The publication and testing adapters are now deployed; the resumed production
checks are recorded in [the release evidence](../deploy/aws/release-2026-09-16-resumed.md). A successful
scope check does not establish user consent, resource authorization, or upload success.

Repeat the read-only check with the configured storage application ID:

```sh
BRIEFCASE_APP_ID='storage' python3 scripts/check_iam_management.py \
  /path/to/protected-iam-handoff.env --require-ready
```

The helper parses the handoff without shell evaluation. Do not shell-source it.
Credentials and root keys stay outside the repository.

## Implemented integration

- Publication planning, current reviewer eligibility, decisions, activation and
  recipient discovery now use the official SDK. Requests are encrypted before
  transmission and replayed unchanged. Activation reuses the reviewed configuration
  and checks IAM's exact accepted publication request ID.
- IAM application snapshots must confirm the current publication request. Local
  visibility alone cannot establish approval. Legacy operator-created catalog
  records need reconciliation before this adapter is rolled out.
- Storage authorization errors distinguish IAM service failures from denials.
  Only known error codes, status and validated request IDs are returned; raw
  upstream messages, details, tokens and proofs are discarded. Seven storage
  wire tests and four publication wire tests pass.
- Shared testing maps one Honeycomb operation to durable IAM phases, then activates
  only after all required service receipts match. IAM revisions, Honeycomb revisions,
  cleaning generations and key versions remain separate.
- App-owned testing uses IAM's immutable production application identity; importing
  a dependency or attaching with a key does not grant environment ownership.
  Imported test apps retain source visibility and IAM source revision pins.

The live create/import/rotate/clean/delete/restore sequence passed against IAM,
Honeycomb and Briefcase on 2026-09-16. Testing OAuth isolation has dedicated adapter
regressions; this shared control-plane pass does not independently prove every
application-owned or test-login flow.

## Production acceptance and remaining work

IAM's proof-lifetime constraint repair and management contracts are deployed.
The protected scheduled-testing capability is enabled; legacy-writer retirement
remains disabled. Saket was explicitly assigned the Honeycomb validator role.
Briefcase's reviewed publication was activated and installed anonymously. Private
IAM registration and installation also passed.

The shared disposable environment passed create, Briefcase import, key rotation,
clean, delete and restore with accepted participant receipts. Capacity cleanup was
explicitly authorized: nine old Briefcase records were already deleted in IAM;
one active legacy environment was adopted and disabled. All ten Briefcase records
were disabled through their protected lifecycle API, preserving keys and data.

Still requiring separate live acceptance: app-owned create/attach, automated
retention/purge, notification delivery and complete legacy catalog adoption.
Keep IAM's legacy-writer cutover disabled until those replacement flows and
migration have passed their relevant checks.

For archive retries, preserve the original idempotency key, revision and bytes.
Do not recreate applications, rotate credentials, fabricate receipts or write
public visibility to bypass a failed integration.
