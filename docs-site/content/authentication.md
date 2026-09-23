## Sign in through IAM
Honeycomb never needs your IAM password. The CLI discovers IAM through `honeycomb iam`; the websites send you through IAM and return to their registered callback. IAM issues a one-use short-lived token, which Honeycomb exchanges for an application session.
```sh
honeycomb iam --json
honeycomb login '<IAM short-lived token>'
honeycomb login status --json
honeycomb logout
```
CLI sessions are stored privately and separated by backend origin and testing context. The client library leaves persistence to its caller. Websites use their own persistent server-side sessions.

## Organization access
IAM grants only the organizations you explicitly share. An app does not automatically gain access when you join another organization. A current **organization owner or admin** can create and manage Honeycomb applications for that organization. Membership alone is insufficient for administration.

If **Create application** is unavailable, inspect the console's explanation, verify your organization role in IAM, and sign in again after permission changes. An old token does not gain a newly approved scope retroactively.

## Honeycomb's own scopes
These are the integration requirements of `honeycomb`, not permissions every application must blindly request.
| Scope | Purpose |
| --- | --- |
| `self.identity.read` | Identify the signed-in Carbon or Silicon. |
| `self.profile.read` | Display profile information. |
| `self.membership.read` | Disclose shared organization membership and role for access checks. |
| `self.tags.read` | Disclose assigned tags needed by Briefcase's delegated authorization checks. |
| `obo:briefcase:briefcase.uploads.reserve` | Reserve package/logo uploads. |
| `obo:briefcase:briefcase.uploads.commit` | Commit uploaded bytes. |
| `obo:briefcase:briefcase.files.read` | Read private package data. |
| `obo:briefcase:briefcase.link_access.update` | Reconcile archive link access during publication and create public logo links. |

Provider approval, effective application scopes, token scopes, selected organization membership, and current user consent must agree. Merely listing a scope in requested configuration does not make it effective. [Scopes and OBO](/scopes-and-obo/) explains how your own application declares access.

## Credentials have different purposes
| Credential | Intended use |
| --- | --- |
| IAM one-use login token | Exchange once to start a Honeycomb session. |
| User access/refresh token | Act as that user within their current authorization. |
| Application secret | Identify an application to IAM; store server-side. |
| Webhook signing secret | Verify incoming webhook authenticity. |
| IAM management service credential | Protected Honeycomb-to-IAM control-plane access. |
| Environment root key | Select and authorize an isolated testing context. |
| IAM step-up assertion | Fresh, action-bound proof for sensitive credential/webhook changes. |

These credentials are not interchangeable. Do not put application secrets or environment keys in frontend bundles, repositories, package archives, or documentation.
