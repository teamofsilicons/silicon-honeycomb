# Automatic publication and Sent requests — 2026-09-16

Publication requests now authorize automatic completion after all required
provider and Honeycomb approvals. IAM acceptance, archive sharing and local
reconciliation still gate public catalog visibility. The console separates the
incoming review queue from Sent requests, which includes the publication history
of every application the signed-in user currently manages.

## Authorization and recovery

The coordinator encrypts a current app manager's short-lived IAM access token,
revalidates authority on every attempt, and retries the existing idempotent
activation operation. External validators cannot substitute their credentials
for app-management authority. The original requester remains in review history;
the activation operation separately records the authorizing manager. Existing
operations retain their actor and mutation key during recovery.

This does not add offline delegation. IAM access-token expiry can leave an
approved request pending until the authorizing manager signs in and opens Sent
requests. A manager can enroll an older approved request that has not started
activation. Missing storage consent similarly requires an updated IAM login.
No additional publication confirmation is required. Credentials are removed on
completion, denial, superseding configuration or after 24 hours.

## Deployment

- Console deployment `dpl_5SH3pMnx5GUrAMJwMGiKBuqniovg` was promoted to
  `console.honeycomb.teamofsilicons.com`; deployed script is `index-CMCkGZGW.js`.
- Backend migrations 0018 and 0019 add encrypted publication authorization and
  its authorizing principal. The backend-only rollout retains session services,
  proxy, application credentials and persistent data.
- SSM `f3e2ab72-fde3-456f-95c2-f5761a546ee7` deployed the manager-recovery image
  `sha256:1380272c43efe3fa83efb827e1425e6e74d0b44a3c0580276bd9847966344dbb`.
- The final diagnostic image is
  `sha256:ef8d7c06bbc0173fe2c8e874838ac9cac9919bccae677a662836422eaa8b35b7`,
  with SSM rollout `008678fc-a4f6-4ba6-a129-300267907fb5`. It maps IAM's hidden
  storage-authority response to renewed-login/effective-scope recovery guidance.
- Consistent SQLite backups passed integrity checks at
  `/var/lib/silicon-honeycomb/backups/automatic-publication-20260916` and
  `/var/lib/silicon-honeycomb/backups/publication-manager-20260916`.
- Previous containers are stopped and retained. Database migrations are additive;
  do not roll back binaries without checking migration compatibility.

## Verification

All 111 server tests and strict all-target Clippy passed, including external-validator
completion, storage retries, approval replay, expired authorization, current
manager recovery and superseded-revision isolation. Three frontend loader tests
cover complete pagination/history, failed history and stalled pagination. The
console production build, formatting, diff checks and vendored IAM provenance
check passed. The browser fixture compiles and the publication journey now
expects automatic publication; the full Playwright suite was not rerun here.

Browser checks verified Sent requests, completed and denied history, publication
details and the existing review discussion. Live approval recovery started
activation for Space Station and Silicon Browser without an activation click.
IAM accepted both revisions. The storage exchange returned `404 not_found`:
the current Carbon session contained only four identity scopes, while Honeycomb
already had the four required effective Briefcase scopes. IAM displayed the
updated consent screen; expanded user consent was not granted administratively.

Live public visibility remains pending that consent and archive completion.
