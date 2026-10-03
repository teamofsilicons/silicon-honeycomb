An application becomes part of the Team of Silicons ecosystem when a Carbon or a Silicon can use it with a clear identity, the right organization, and a predictable set of permissions. A polished website helps, but the deeper work is making the application behave consistently: from a terminal, through an API, during a delegated action, and inside an isolated test environment.

Consider **Fieldnotes**, a fictional application owned by the organization `demo`. It starts with a simple task: record and retrieve project notes. Later, it adds spoken summaries and stores generated audio through other applications. Following that small product gives us a practical way to think about identity, consent, integrations, and distribution without turning the guide into an endpoint catalog.

Use the linked references for exact request shapes. The examples describe integration responsibilities; support for a particular provider and feature should be established with the matching application versions and a real end-to-end test.

## Start with a complete command-line experience

Design the first useful Fieldnotes workflow in the CLI. A Carbon should be able to run it interactively; a Silicon should be able to discover the same capability, supply its inputs, and understand the result programmatically. Build both paths around the same application operations and authorization rules.

That means more than adding an install command. Account setup, organization selection, normal work, feature authorization, settings, and recovery all need a command-line path. If the website can export a notebook but the CLI cannot, the application has given its automated users a smaller product. Treat the website as another interface to the capabilities already available through the CLI and API.

Make those capabilities discoverable. Each command’s help should explain its purpose, required inputs, useful examples, and related commands. Structured JSON should provide stable fields for success and failure. A missing permission should identify the action needed to continue; a network interruption should leave enough information to retry. Keep secrets out of status output and ordinary logs.

The [IAM application builder guide](https://docs.iam.teamofsilicons.com/building/) describes discovery and login conventions. In particular, an application's `iam` command should work before login, so someone can find the correct IAM entry point without already having a session.

## Let IAM authenticate the account

Fieldnotes needs a signed-in application user. It does not need the user’s IAM password, Silicon token key, email verification code, or direct IAM refresh token. Its login integration should receive only the application-bound short-lived token, or SLT, produced by IAM.

For a browser flow, send the user to IAM with Fieldnotes’ application ID and the callback that your backend actually implements. IAM handles account authentication and selection. The callback receives an SLT; your backend exchanges it using Fieldnotes’ own application credential and establishes an application session. Bind the callback to the login attempt that started it, and remove the token from the address bar after handling it. Keep the application secret on the backend.

Offer **Continue as Carbon** and **Continue as Silicon** as separate buttons. Open a small popup from the click, passing `identity_kind=carbon` or `identity_kind=silicon` and `display=popup` to IAM. Save the selected type with the login state on your backend. Before creating the application session, verify that the exchanged credential belongs to that type using IAM's authenticated response or introspection. IAM presents the matching accounts, and selecting an organization continues as that account and organization immediately.

Prefer a backend callback that establishes the session before the popup sends a completion message to the exact application origin. If your single-page app handles the callback, it can instead pass the one-use SLT and matching state to its opener for immediate exchange through your backend. The opener must check the message origin, popup window and a fresh request nonce; never pass access tokens, refresh tokens, or direct IAM credentials through the message. Close the popup and resume the page that started login only after the exchange succeeds. Preserve a validated return destination with the original request when another page should open after completion. Handle closed or blocked popups clearly and retain a standalone completion page when no destination was supplied.

A terminal flow reaches the same handoff. The official IAM client can obtain an SLT for an already authenticated Carbon or Silicon, and the application CLI submits that token to its own backend. The application should not recreate IAM’s authentication ceremony to make a headless flow possible.

After the exchange, read the current authorization rather than waiting for a webhook to initialize access. Store and refresh the application session securely, serialize refreshes for the same credential family, and retain a stable retry identity when a response is uncertain. A temporary transport failure is different from an explicit credential rejection; the former should not randomly sign the user out. Follow the [application login contract](https://docs.iam.teamofsilicons.com/client/login/) and [Honeycomb authentication guide](/authentication/) for the supported interfaces.

## Treat Carbon and Silicon as first-class users

A Carbon and a Silicon are different principal types, but both can be application users, organization members, and authorized actors. Model that distinction explicitly. Preserve the verified principal identity and membership context instead of assuming every account has a human email address or that every action begins in a browser.

In Fieldnotes, permission to edit a shared notebook should come from the actor’s current authority over that notebook. Do not add an unexplained Carbon-only restriction to an action simply because its first implementation used a web form. Equally, a Silicon login does not grant permission to read everything in an organization. Roles, resource access, declared scopes, and applicable policy still govern the action.

Only use identity and profile fields IAM actually discloses. An absent role, email, or directory field is not an invitation to infer it or copy it from a more privileged session. Request what the feature needs, handle unavailable fields honestly, and keep display preferences separate from authorization decisions. The [IAM permissions guide](https://docs.iam.teamofsilicons.com/iam-scopes/) describes those disclosure boundaries.

## Keep each session attached to one account and organization

Each application login selects one account and one organization. Fieldnotes might have an Alice session for `demo`, another Alice session for a personal organization, and a Silicon researcher session for `demo`. These are separate application sessions, even when the interface presents them together in an account switcher.

If your app supports several accounts or organizations at once, it owns that session management. Store each credential family under its verified account, organization, application, and testing context. Show the active account and organization before a consequential action. Switching the visible organization must select the corresponding session, not change an `org_id` field while reusing a token from somewhere else.

Carry this separation into caches, background jobs, and pending authorization screens. An export started in Alice’s `demo` session should not finish under the researcher’s session because someone switched accounts while it was running. Clear or fence pending UI state when its context changes, and let immutable job ownership determine where a retry belongs.

Ordinary IAM login asks for consent when critical IAM permissions require it. It does not collect every future OBO permission. That distinction lets Fieldnotes open quickly and explain an integration when someone actually chooses to use it.

## Ask for delegated access when the feature needs it

OBO means **on behalf of**. It lets one application call another for a particular user under explicitly approved authority. For Fieldnotes, selecting “Create a spoken summary” could require a speech provider; saving the resulting audio could require a storage provider.

Declare the required endpoints and their dependencies in application configuration, using the providers’ registered catalogs. Then, when the feature needs them, start a separate OBO authorization request. IAM shows the requesting application, the requested actions, the providers involved, and the declared dependency chain. Notes and warnings should explain consequences such as credit use or storage changes where relevant.

On the website, open that review in a popup from the feature's button, with a registered application callback and request-bound state. Complete the one-use code exchange before reporting success to the original window. Check the completion message's origin, source window, and attempt identifier, just as you do for login. Keep credentials out of browser messages and preserve the initiating account, organization, and testing context throughout the review.

If the feature supplied a validated return page when it started the request, return there after the exchange succeeds. Otherwise, show a clear completion page with a way back to the application. Closing, blocking, or declining the popup must leave the feature usable. If saving an approved result fails temporarily, retain a recoverable pending attempt and retry with the same operation identity rather than asking for consent again. CLI callers can use the returned review URL and manual code flow without requiring a browser popup.

If Fieldnotes exposes its own delegated actions, add their definitions in Honeycomb too. A local ID such as `notes.read` becomes `[fieldnotes:obo:notes.read]`. Supply a clear name, endpoint path, description, critical/non-critical classification, and metadata. Add an optional note, explicit downstream dependencies, and applicable warnings from the predefined list, such as credit use. Register the backend origin and implement the handler’s verification and resource checks; a catalog entry describes the action but does not implement it.

This is more useful than a vague “connect everything” prompt at sign-in. A person can see why speech and storage are being requested at that moment. They can also decline and continue using unrelated Fieldnotes features. After approval, redeem the one-use authorization code on the application backend and securely store the dedicated OBO credentials. Do not relabel the ordinary login token as delegated authority.

Consent is durable: ordinary logout does not erase the grant. Users can manage and revoke it in IAM. The credentials remain subject to expiry, refresh rotation, current membership, security changes, and the approved graph. A new downstream dependency needs a fresh review; an existing approval must not silently grow to cover it.

Build an explicit recovery path for decline, expired approval, and revocation. Return the user to the feature with a useful status and a deliberate retry. Approval should not accidentally trigger a second paid generation or duplicate an upload. The [delegated access guide](/scopes-and-obo/) and [IAM OBO client contract](https://docs.iam.teamofsilicons.com/client/obo/) explain the authorization, exchange, refresh, and revocation lifecycle.

## Preserve authority throughout an OBO chain

The special property of an approved OBO chain is that its access token can be verified at every authorized endpoint in that root’s graph. Fieldnotes can call the speech provider, which can forward the same token to a declared storage dependency. Each receiver authenticates itself to IAM and verifies its own endpoint and request path before acting. Unrelated root permissions have their own grants; this is not a universal token for every integration.

Only the originating application retains the refresh credential. A downstream service receives the access token it needs for the approved work. Verification is repeatable and does not consume that token, so the receiver must still implement operation-level idempotency and resource authorization. A valid endpoint token does not establish ownership of every file ID supplied in the request body.

The user may select an account and organization for each provider. Perhaps the summary starts in a work account but the approved destination is a different storage context. Use the provider-specific context returned by verification; do not substitute the originating login’s organization or whichever account is currently visible in the UI. Pin the chosen destination before starting an operation whose outcome may need to be recovered.

Different-account flows also depend on the receiving application’s resource policy and the identity disclosures it is authorized to receive. Do not assume every provider supports every combination automatically. If a provider needs a role or identity field that is not disclosed, stop and explain the unsupported context. Preserve its access checks while resolving the integration contract. Test the actual account-and-organization combination you plan to offer.

## Use ATA for application authority

Some work belongs to an application itself. A scheduled service task may need another application’s endpoint without representing an individual user. That is the purpose of **app-to-app verification**, or ATA.

Configure ATA endpoints inside each application's configuration in Honeycomb. Create and manage your verifications from the console's centralized **App to App** page, choosing the originating application you manage. Review the expanded endpoint dependencies and the applications allowed to use that verification. Honeycomb records the signing Carbon or Silicon; the signer is audit information about who authorized the setup, not a user identity delegated to every runtime request. The [ATA reference](/app-to-app/) covers the management workflow.

ATA definitions have their own IDs, such as `[fieldnotes:ata:notes.read]`. Honeycomb can import an OBO definition to save retyping, but that copies the description of an endpoint, not its user grant or dependency authority. Review the ATA dependencies independently.

The originating backend stores the one-time refresh credential and exchanges it for short-lived access proof. Each receiving application verifies the originating app, proof, and requested endpoint using its own credentials. An unapproved recipient or endpoint must fail just as an invalid proof does. Choose an appropriate verification lifetime and access-token validity, and provide a way to revoke the verification when the integration is retired.

ATA remains application authority through the entire chain. It cannot become OBO later because a downstream feature happens to need a user’s files. That action requires a separately approved user delegation. Likewise, an application identity key merely proves which application is calling; it is not automatically an ATA endpoint grant. See [IAM’s ATA contract](https://docs.iam.teamofsilicons.com/api/applications/#app-to-app-verification-ata) and Honeycomb’s [ATA commands](/cli-reference/#honeycomb-apps-ata).

## Make testing a real isolated environment

A Team of Silicons-ready application needs to participate in the shared testing environment. This is the same application behavior against isolated identities, credentials, grants, and data, not a UI toggle that skips authorization.

Create or import Fieldnotes through the supported environment workflow. Its declared dependencies must participate in that same environment, with the accepted configuration and release pins you intend to test. Keep production and test application credentials separate. Validate the testing context through IAM before selecting local storage or databases; an unvalidated request flag must not choose an arbitrary environment.

Carry the environment through every downstream call, queued job, session, cache entry, and OBO or ATA operation. A missing, disabled, or mismatched test context must fail instead of falling back to production. The environment selector chooses where authorization is evaluated; it does not replace the actor’s permissions.

A package’s development channel is also separate from testing isolation. A development build can still make production calls unless it explicitly selects a valid testing environment. Conversely, testing can exercise an imported production release without touching production records.

Implement lifecycle handling from the start. Cleaning an environment must clear its application data and invalidate stale pending work. Root-key rotation is a different action: do not assume it also resets every test application secret. Deletion must stop access, and restoration must follow the coordinated state. Use environment identity and generation to keep old jobs or credentials from reappearing after cleanup. Completion requires the participating services to confirm their work, not just a successful dispatch.

Before calling this ready, run both useful workflows and boundary failures: a Carbon and a Silicon login, separate OBO approval, repeated provider calls, explicit revocation, a mismatched environment, and a clean while work is pending. Use the [Honeycomb testing guide](/testing-environments/) and [IAM testing integration](https://docs.iam.teamofsilicons.com/client/testing-environments/) for provisioning and lifecycle details.

## Keep authorization current through webhooks

Login gives Fieldnotes an initial authorization snapshot. It does not promise that a membership, role, permission, or application configuration will remain unchanged. Webhooks help the application react when those facts change.

Choose the relevant `webhook_scope` categories: `membership` for joins and removals, `updates` for permitted profile, organization, and authorization changes, and `trust` for authorized trust changes. `full` subscribes across categories within existing authority; it does not disclose everything. Configuration acceptance and shared testing lifecycle coordination have separate service contracts. Handle the required configuration reconciliation and clean/delete/restore instructions through those workflows rather than assuming an ordinary subscription grants lifecycle control.

Provide a real receiver and verify the signature over the exact request bytes before parsing or applying an event. Keep webhook signing material separate from the application secret. Persist the event’s deduplication identity and its local effect atomically, and use resource versions to prevent an older delivery from overwriting newer state. Replayed delivery should be harmless.

Invalidate or refresh affected authorization caches when membership or access changes. Scope subscriptions deliberately: receiving a webhook does not entitle Fieldnotes to undisclosed fields. Treat removal notifications and absent fields according to the contract, without restoring private data from another session’s cache. Webhooks complement current authorization checks; they do not authorize a request by themselves.

Testing deliveries need the same care. Verify the signed outer envelope, validate the testing context, and route only to that environment. Keep embedded testing credentials out of logs and event tables. Exercise duplicate, stale, tampered, and out-of-order deliveries as well as the happy path. The [receiver contract](https://docs.iam.teamofsilicons.com/client/webhooks/) covers implementation; [Honeycomb webhook management](/webhooks/) covers destinations and secret changes.

## Publish an application people can understand

Register Fieldnotes through Honeycomb under its owning organization with a permanent, globally unique handle, a useful description, a recognizable logo, and the required webhook and permission configuration. Its ID is `fieldnotes`; its owner, `demo`, is stored separately. The console supports logo upload, and the CLI provides `apps upload-logo`. Uploaded logos are public assets even when the application is private, so choose suitable imagery.

Prepare real native CLI builds for all six required platforms, place them under a root `honeycomb.yaml`, and validate the resulting archive. Honeycomb packages these builds; it does not compile them for you. Follow the [package format](/package-format/) and [application configuration](/application-config/) references.

Keep configuration revisions distinct from release versions. A revision describes the application configuration used for an operation. A version such as `1.0.0` identifies immutable release bytes within a channel. Upload changed bytes under a new version, and read current state before using a revision precondition. Preserve the same input and idempotency key when retrying an uncertain operation.

Honeycomb creates the internal IAM application identity as part of registration. Use Honeycomb for application configuration and permission reviews throughout the lifecycle; direct production registration in IAM is retired. App review emails and discussions belong in Honeycomb, while the individual user’s login and OBO consent remain in IAM.

For public distribution, a valid production release and accepted configuration begin the approval workflow. Follow **Sent requests** for reviewer questions, status, and new updates; use **Received requests** for decisions you are authorized to make. An uploaded package or individual approval is not the same as completed publication. A permission-expanding update remains private while approval is pending, leaving the previous public release available to installations.

Share `/apps/fieldnotes` for the current public production release and `/apps/fieldnotes/releases/prod/1.0.0` when discussing that exact version. Check the journey from discovery to installation and the first successful command. [Releases](/releases/), [publication](/publication/), and [operation recovery](/operations/) explain the detailed workflow.

## TODO: prove the application is ready

Use this checklist as work to complete and evidence to collect. A configured endpoint or successful build alone does not check an item off.

- [ ] Every user-facing capability has a usable CLI/API path, clear help, structured results, and an understandable recovery action.
- [ ] Carbon and Silicon accounts can complete the intended workflows under equivalent permissions, without collecting IAM credentials inside the application.
- [ ] Application login exchanges only an app-bound SLT; callback handling, secure session storage, refresh, and rejected credentials are exercised.
- [ ] Carbon and Silicon buttons open separate typed login popups; the backend rejects the wrong identity type. Popup completion, cancellation, and validated return destinations work.
- [ ] Feature-specific OBO approval opens in a popup, resumes the initiating context, and recovers from an interrupted exchange without creating duplicate work.
- [ ] Each login remains bound to one account and organization. Multiple sessions, caches, pending approvals, and jobs stay separate when users switch context.
- [ ] IAM scopes and provider endpoints match actual features. Missing or undisclosed authorization fields never become implied permission.
- [ ] OBO starts at the feature that needs it. Approval, decline, code redemption, refresh, logout, recovery, and explicit revocation behave predictably.
- [ ] Every OBO receiver verifies its own endpoint and selected provider context, retains resource checks, and prevents duplicate side effects. Offered cross-account paths are tested end to end.
- [ ] ATA recipients and dependencies are explicit. Expired, revoked, or mismatched proof is rejected, and no ATA path acquires user authority.
- [ ] Shared testing isolates credentials, data, downstream calls, and background work. Invalid context never reaches production; cleanup and lifecycle changes fence stale work.
- [ ] Webhooks verify exact bytes, deduplicate atomically, respect resource versions, and invalidate the right caches in production and testing.
- [ ] Package validation passes for all required targets, and representative native installations complete real work.
- [ ] The listing, logo, release pages, approval discussion, private update behavior, and installation commands make the release understandable to someone new.
