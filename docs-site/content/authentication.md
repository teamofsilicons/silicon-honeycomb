## Sign in through IAM
Honeycomb never needs your IAM password. The CLI discovers IAM through `honeycomb iam`; the websites send you through IAM and return to their registered callback. Choose one Carbon or Silicon account and one organization. IAM issues a one-use short-lived token, which Honeycomb exchanges for an application session. The sign-in consent screen covers critical IAM permissions; it does not grant access to another application through OBO.
```sh
honeycomb iam --json
honeycomb login '<IAM short-lived token>'
honeycomb login status --json
honeycomb logout
```
CLI sessions are stored privately and separated by backend origin and testing context. The client library leaves persistence to its caller. Websites use their own persistent server-side sessions.

## Organization access
A Honeycomb login is scoped to the selected account and organization. Configuring more accounts in IAM gives you more choices at sign-in; it does not combine their organizations into one Honeycomb session. Sign in with the appropriate account and organization when changing context. A current **organization owner or admin** can create and manage Honeycomb applications for that organization. Membership alone is insufficient for administration.

If **Create application** is unavailable, inspect the console's explanation, verify your organization role in IAM, and sign in again after permission changes. An old token does not gain a newly approved scope retroactively.

## Honeycomb's own scopes
These are the IAM integration requirements of `honeycomb`, not permissions every application must blindly request.
| Scope | Purpose |
| --- | --- |
| `self.identity.read` | Identify the signed-in Carbon or Silicon. |
| `self.profile.read` | Display profile information. |
| `self.membership.read` | Disclose the selected organization membership and role for access checks. |
| `self.tags.read` | Disclose assigned tags needed by Briefcase's delegated authorization checks. |

## Briefcase storage permission
Signing in does not authorize storage. Honeycomb requests separate user OBO consent when an operation needs Briefcase. Its registered storage access includes:

| External scope | Purpose |
| --- | --- |
| `obo:briefcase:briefcase.uploads.reserve` | Reserve package/logo uploads. |
| `obo:briefcase:briefcase.uploads.commit` | Commit uploaded bytes. |
| `obo:briefcase:briefcase.files.read` | Read private package data. |
| `obo:briefcase:briefcase.link_access.update` | Reconcile archive link access during publication and create public logo links. |

In the console, choose **Review storage permissions** when prompted. For the CLI:
```sh
honeycomb --idempotency-key my-storage-request-001 apps storage start my-org
honeycomb apps storage status AUTHORIZATION_ID
honeycomb apps storage complete AUTHORIZATION_ID --code-file /secure/path/consent-code --state RETURNED_STATE
```
Open the returned `consent_url`, review the provider endpoints and selected account/organization, and approve or reject the request in IAM. Use the `authorization_id` and `state` from the start response; put only the approved one-time code in the code file. After completion, retry the original upload with its original idempotency key, revision, channel, and file. `status --wait` can wait for a request completed in another browser or CLI; it does not approve the request for you.

Honeycomb stores the resulting storage access/refresh credentials separately from its login session. Provider approval, effective application scopes, token scopes, selected organization membership, and current user consent must agree. Merely listing a scope in requested configuration does not make it effective. [Scopes and OBO](/scopes-and-obo/) explains how your own application declares access.

## Credentials have different purposes
| Credential | Intended use |
| --- | --- |
| IAM one-use login token | Exchange once to start a Honeycomb session. |
| Honeycomb login access/refresh token | Maintain the selected account and organization session. |
| OBO access/refresh token | Perform separately approved provider actions with the account and organization selected in that consent. |
| Application secret | Identify an application to IAM; store server-side. |
| Webhook signing secret | Verify incoming webhook authenticity. |
| IAM management service credential | Protected Honeycomb-to-IAM control-plane access. |
| Environment root key | Select and authorize an isolated testing context. |
| IAM step-up assertion | Fresh, action-bound proof for sensitive credential/webhook changes. |

These credentials are not interchangeable. Do not put application secrets or environment keys in frontend bundles, repositories, package archives, or documentation.

## Browser popups and return destinations

Show separate **Continue as Carbon** and **Continue as Silicon** buttons. Honeycomb opens IAM in a popup with `identity_kind=carbon|silicon` and `display=popup`, binds the chosen type and return page to signed login state, exchanges the SLT on its server, and checks the returned identity before setting the application session. The popup sends only a completion signal to the originating Honeycomb window; it never sends access or refresh tokens through browser messages.

OBO approval also opens in a popup when a feature needs storage. `POST /api/v1/storage-authorizations` accepts `org_id`, an optional `return_url` pointing to an allowed Honeycomb origin's `/storage-authorization` callback, and an optional `redirect_url` for the page to visit after successful completion. The redirect must use the same Honeycomb origin, requires `return_url`, and is bound to the original request and idempotency key. It cannot be changed on the callback. The callback completes the exchange before redirecting. Without `redirect_url`, it shows the **Return to Honeycomb** page; popup callers can close that window and resume their feature.

The CLI supports `apps storage start ORG --return-url URL --redirect-url URL`. Omit both URLs for the manual CLI consent-code flow. Applications integrating directly with IAM should use their registered `redirect_uri` callback, bind any later return destination to their own request state, and use exact-origin, exact-window completion checks.
