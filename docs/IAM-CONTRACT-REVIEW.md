# IAM integration status

Updated 2026-09-16. This records implementation and verification separately;
local fixtures do not establish production readiness. Requirements remain in the
human-owned [UNDERSTANDING.md](../UNDERSTANDING.md).

## Official contract

Honeycomb pins unchanged official IAM SDK 1.11.0 source at
`21e61b4f6de0b35e1588db9ea651a27409dc7b0e`. Its source hashes and standalone
manifest are recorded in `vendor/silicon-iam-client/UPSTREAM.json` and verified by
`scripts/verify_vendored_iam.py`. IAM also published this SDK version to crates.io.
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
The new publication and testing adapters are not yet deployed. A successful
scope check does not establish user consent, resource authorization, or upload success.

Repeat the read-only check with the configured storage application ID:

```sh
BRIEFCASE_APP_ID='<organization>storage' python3 scripts/check_iam_management.py \
  /path/to/protected-iam-handoff.env --require-ready
```

The helper parses the handoff without shell evaluation. Do not shell-source it:
application IDs contain `>`. Credentials and root keys stay outside the repository.

## Local integration work

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

Testing adapter integration, reciprocal mutation fences and full validation are
still in progress. Do not report these flows as live until deployed E2E succeeds.

## Outstanding production acceptance

1. Deploy IAM's management contracts and the OBO proof-lifetime constraint repair.
   The old 60-second database constraint rejects configured 300/3600-second proofs.
   The repair belongs to a new IAM migration, not a historical migration edit.
2. Confirm IAM supports root-key cleanup with the same revision, generation and
   key-version bounds as the existing environment contract.
3. Finish Honeycomb adapter checks, deploy, then enable IAM's protected service
   finalization capability as part of coordinated deployment. It is not a user
   testing enable key.
4. Assign the intended Honeycomb validator explicitly. Organization admin access
   alone does not grant this capability. Complete real plan/decision/activation
   and verify public archive bytes and anonymous installation.
5. Verify app-owned create/attach, additive import, clean, key rotation, delete,
   restore and retention through real participants. Preserve existing environment
   identities, keys, owners and links during adoption.
6. Keep IAM's legacy-writer cutover disabled until migration and replacement flows
   have passed the relevant live checks.

For archive retries, preserve the original idempotency key, revision and bytes.
Do not recreate applications, rotate credentials, fabricate receipts or write
public visibility to bypass a failed integration.
