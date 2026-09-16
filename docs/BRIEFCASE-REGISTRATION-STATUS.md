# Briefcase registration status — 2026-09-16

**Awaiting the user's explicit confirmation before submitting a new registration.**
See [the complete registration preview](BRIEFCASE-REGISTRATION-PREVIEW.md).

The user rejected adoption and requested a completely new app with fresh IAM
credentials. They then explicitly requested removing every existing production IAM
app except Honeycomb directly from the database, freeing `tos>briefcase` for reuse.
That reset completed successfully; see [operator evidence](../deploy/operator/README.md).
Honeycomb's identity and management connection were preserved and verified live.

No new Briefcase record has been created in IAM or Honeycomb, and no application
secret has been issued. The new webhook signing secret and normal registration
payload are prepared outside Git with owner-only permissions. The six-target
`briefcase-1.1.0.tar.gz` archive was found and passed Honeycomb validation.

The old Briefcase IAM identity and credentials are gone as requested. Following
confirmed registration, its fresh credentials must be configured in the service.
A planned permanent archive URL now points to Honeycomb's actual Briefcase folder
and filename; it is not a live uploaded release. Upload OBO proofs for file creation,
reservation and commit are prepared with a 60-minute lifetime.
Archive upload and publication remain separate actions and have not been completed.

## Earlier backend maintenance

`beb9c63` added audited legacy revision-zero adoption support, with passing server,
Clippy, formatting, dependency and Python import checks. Deployment
`482946e1-e129-4fbc-9012-ba0ec253501c` succeeded on Honeycomb's AWS host. That adoption
path was never applied to Briefcase and is not used for this new registration.
