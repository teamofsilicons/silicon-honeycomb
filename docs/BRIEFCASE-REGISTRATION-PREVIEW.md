# Briefcase new-app registration preview

**Status: prepared; not submitted. User confirmation is required before registration.**

This creates a new IAM identity and fresh application credentials through the ordinary authenticated Honeycomb `apps create` flow. No previous identity or credential is adopted.

## Application

- Organization: `tos`
- Application ID: `tos>briefcase`
- Display name: Silicon Briefcase
- Initial visibility: private to `tos`
- Testing idle limit: 30 days
- Website: https://briefcase.teamofsilicons.com
- Documentation: https://docs.briefcase.teamofsilicons.com
- Backend origin: https://backend.briefcase.teamofsilicons.com
- Webhook: https://backend.briefcase.teamofsilicons.com/webhook/
- Webhook categories: `full`
- New webhook signing secret: prepared in a protected local file; omitted here.

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
| `briefcase.files.create` | `/api/v1/obo/files` | false | 300 seconds |
| `briefcase.files.read` | `/api/v1/obo/files/read` | false | 300 seconds |
| `briefcase.folders.create` | `/api/v1/obo/folders/create` | false | 300 seconds |
| `briefcase.invitations.create` | `/api/v1/obo/invitations` | true | 300 seconds |
| `briefcase.link_access.update` | `/api/v1/obo/link-access` | true | 300 seconds |
| `briefcase.uploads.cancel` | `/api/v1/obo/uploads/cancel` | false | 300 seconds |
| `briefcase.uploads.commit` | `/api/v1/obo/uploads/commit` | false | 300 seconds |
| `briefcase.uploads.reserve` | `/api/v1/obo/uploads/reserve` | false | 300 seconds |
| `briefcase.uploads.status` | `/api/v1/obo/uploads/status` | false | 300 seconds |

The `briefcase.files.create` metadata schema requires `content_type`, `name`, and `path`, each a string. Other endpoints have empty metadata definitions. All endpoints are enabled.

Provider review guidance:

Describe why your application needs to invite organization members or make entries public. Briefcase limits each operation to the calling app directory and the represented member permissions.

## Release archive

- File: `/Users/codanium/Documents/silicon/silicon-briefcase/dist/briefcase-1.1.0.tar.gz`
- Version: `1.1.0`
- Bytes: 20711083
- SHA-256: `ccee3f85e257e61faa826f65dfff7b42d7964471cd3ea127d69b0f161281019e`
- Honeycomb validation: passed for Linux, macOS and Windows on x86_64 and aarch64.

Registration, archive upload, service credential configuration, and publication are separate steps. No new application secret has been issued yet; normal registration returns it once. Fresh credentials must subsequently be configured in Briefcase before its login/storage can work. The archive has not been uploaded.
