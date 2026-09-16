# Briefcase new-app registration preview

**Status: direct database bootstrap completed; public and approved in IAM and Honeycomb.**

The user explicitly authorized direct database creation and public approval because the planned package URL does not resolve yet. No normal registration request was sent. This creates a fresh IAM identity and credentials; no previous identity or credential is adopted. See [current status and verification](BRIEFCASE-REGISTRATION-STATUS.md).

## Application

- Organization: `tos`
- Application ID: `tos>briefcase`
- Display name: Silicon Briefcase
- Visibility: public
- Testing idle limit: 30 days
- Website: https://briefcase.teamofsilicons.com
- Documentation: https://docs.briefcase.teamofsilicons.com
- Backend origin: https://backend.briefcase.teamofsilicons.com
- Webhook: https://backend.briefcase.teamofsilicons.com/webhook/
- Webhook categories: `full`
- Fresh application and webhook secrets: generated and saved with owner-only permissions outside Git; omitted here.

## Description

Silicon Briefcase provides organization file storage for people and applications in the Silicon ecosystem. It supports files, folders, uploads, downloads, sharing, and delegated access through IAM. Applications can reserve and commit uploads, read authorized files, and manage link access through approved Briefcase endpoints. Organization membership and effective permissions determine which resources each person or application can use.

## Requested IAM scopes

- `self.identity.read`
- `self.profile.read`
- `self.organizations.read`
- `self.membership.read`
- `self.tags.read`
- `self.email.read`
- `directory.carbons.read`
- `directory.silicons.read`
- `directory.memberships.read`
- `directory.tags.read`

External OBO scopes requested by Briefcase: none.

## OBO endpoints exposed by Briefcase

| Endpoint | Path | Critical | Proof lifetime |
| --- | --- | --- | --- |
| `briefcase.entries.list` | `/api/v1/obo/entries/list` | false | 300 seconds |
| `briefcase.entries.trash` | `/api/v1/obo/entries/trash` | false | 300 seconds |
| `briefcase.files.create` | `/api/v1/obo/files` | false | 3600 seconds (60 minutes) |
| `briefcase.files.read` | `/api/v1/obo/files/read` | false | 300 seconds |
| `briefcase.folders.create` | `/api/v1/obo/folders/create` | false | 300 seconds |
| `briefcase.invitations.create` | `/api/v1/obo/invitations` | true | 300 seconds |
| `briefcase.link_access.update` | `/api/v1/obo/link-access` | true | 300 seconds |
| `briefcase.uploads.cancel` | `/api/v1/obo/uploads/cancel` | false | 300 seconds |
| `briefcase.uploads.commit` | `/api/v1/obo/uploads/commit` | false | 3600 seconds (60 minutes) |
| `briefcase.uploads.reserve` | `/api/v1/obo/uploads/reserve` | false | 3600 seconds (60 minutes) |
| `briefcase.uploads.status` | `/api/v1/obo/uploads/status` | false | 300 seconds |

The `briefcase.files.create` metadata schema requires `content_type`, `name`, and `path`, each a string. Other endpoints have empty metadata definitions. All endpoints are enabled.

Upload proof lifetime is 60 minutes for `briefcase.files.create`,
`briefcase.uploads.reserve`, and `briefcase.uploads.commit`. Other endpoints keep
their five-minute lifetime. This changes OBO proof validity, not upload reservation
or transfer-capability expiry; proofs remain single-use.

Provider review guidance:

Describe why your application needs to invite organization members or make entries public. Briefcase limits each operation to the calling app directory and the represented member permissions.

## Planned archive link

- [Briefcase archive — planned, not uploaded](https://briefcase.teamofsilicons.com/org/tos/apps/tos%3Ehoneycomb/public/tos--briefcase-1.1.0.tar.gz)
- Briefcase organization: `tos`
- Exact storage path: `apps/tos>honeycomb/public/tos--briefcase-1.1.0.tar.gz`
- [Direct public download — available after upload and link sharing](https://backend.briefcase.teamofsilicons.com/api/v1/public/tos/apps/tos%3Ehoneycomb/public/tos--briefcase-1.1.0.tar.gz?view=attachment)

This matches Honeycomb's existing upload folder and filename. The URL can remain
unchanged when the archive is uploaded at that exact path. It does not currently
resolve to a verified release. Public download additionally requires effective
public link access; the folder name `public` alone does not grant access.

The future link is saved in the local release plan alongside the validated archive
hash and size. The operator bootstrap records this path directly instead of sending
the normal registration/upload request. It does not assert that an upload occurred.

## Release archive

- File: `/Users/codanium/Documents/silicon/silicon-briefcase/dist/briefcase-1.1.0.tar.gz`
- Version: `1.1.0`
- Bytes: 20715240
- SHA-256: `a2a1fce14171d3b03f0c8a5942f17c69079b15ebad9b783d680e0ce05b85ccb0`
- Honeycomb validation: passed for Linux, macOS and Windows on x86_64 and aarch64.

This final rebuild includes merged upstream manuals. Its hash and size replaced the
original planned metadata before the first upload, with a separate operator audit
record. The original bootstrap history is preserved; see the
[registration status](BRIEFCASE-REGISTRATION-STATUS.md#final-archive-metadata-correction).

Fresh credentials have been issued and authenticated against production IAM. They are now configured in the deployed Briefcase service. The archive has not been uploaded, and a public catalog entry does not make its download available.
