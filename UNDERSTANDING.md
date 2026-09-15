h
# This file is only meant to be changed by carbons (humans), if you are an agent DONT EDIT THIS FILE.  


# UNDERSTANIDNG.md - Honeycomb

Honeycomb is our application layer for all the apps in the silicon ecosystem they would be availaible via honeycomb and honeycomb console can be used to upload the said apps to match our set of requiremenets. 


# Login/Signup

Logging in and signing up are handled entirely by Silicon IAm (this is our access and authorization management layer). You would have an app_id and app_secret stored in your env that you can use to request the login and signup from Silicon IAm (read [(https://docs.iam.teamofsilicons.com/)]) you would realise how you would need to login and singup using silicon IAm. For both signing in and signing up into the system would need Silicon IAm authorization, once you have the access token from SIlicon IAm for the user logged in, render the application accordingly. 

Use the oficial and latest silicon client for using IAm at all times and across everywhere. (https://crates.io/crates/silicon-iam-client/)

The webhook endpoint ([backend.honeycomb.teamofsilicons.com/webhook/]) you have would give you information whenever someone logs out, kicked from org, anything changes you would know.


# Apps 

Honeycomb is responsible for managing every application in the silicon ecosystem. Apps are org scoped so apps are owned by organisations. IAM would be used for managing the authentication for all of these apps. 

For creating each applications we have a set of things we need:
1) CLI
2) Webhook (url, secret, scope of updates)
3) OBO Endpoints (can be defined by the user - optional)
4) Base URL (optional, required if there are obo endpoints)
5) App Scope (Scope of the application)
6) App ID (This is the App ID, app ID is always in `org_id>local_app_id`)
7) App Name
8) App Logo URL - optional
9) App Version
10) App Description
11) Website Link - Optional
12) Docs Link - Optional

The end to end process for registering an application is a 3 step process:
`Send Application > Application reviewed and approved > Goes to the apps for scope verification (if any critical scopes) > App Published`

Even without publishing the application it should be possible to use the application but in that case the use of the application would be limited to the organisation who owns the app. So for eg: tos create an app named cat so tos>cat, and that's an app not yet published, so this becomes an tos only application - so login would be restricted to tos only members. Private applications bypass provider approval for critical IAM scopes and critical OBO scopes. They must still declare the requested scopes, obtain user consent and pass normal authorization and OBO proof checks. IAM restricts login and organisation grants to current members of the application's owning organisation. 


### CLI

For each application it's compulsory to have a CLI, for our cli's it needs to support the following list of operating systems:

| OS      | Architecture                 | Package target    | Optional |
| ------- | ---------------------------- | ----------------- | -------- |
| Linux   | Intel/AMD 64-bit             | `linux-x86_64`    | No       |
| Linux   | Intel/AMD 32-bit, i686-class | `linux-i686`      | Yes      |
| Linux   | ARM 64-bit                   | `linux-aarch64`   | No       |
| Linux   | ARMv7 32-bit, hard-float     | `linux-armv7hf`   | Yes      |
| Windows | Intel/AMD 64-bit             | `windows-x86_64`  | No       |
| Windows | Intel/AMD 32-bit             | `windows-i686`    | Yes      |
| Windows | ARM 64-bit                   | `windows-aarch64` | No       |
| macOS   | Intel 64-bit                 | `macos-x86_64`    | No       |
| macOS   | Apple Silicon                | `macos-aarch64`   | No       |


For each cli package they would need to package and give us the `.tar.gz` format. For each archive it must have 

```
briefcase-1.2.0.tar.gz
├── honeycomb.yaml   # Required
├── targets/
```

For each tar it must always have [honeycomb.yaml], refer to [honeycomb.yaml] to understand the structure of the target files and how exactly can it be reffered. 

For each package they should be able to run `honeycomb pack <optional file parameter - by default .>` which would package it all based on the honeycomb.yaml file, if it's not present respond with "This repo doesen't have honeycomb.yaml file, the file has been initiated fill the details and then try running honeycomb package. Read the contents of the file and fill them to continue. Run `honeycomb validate` to validate if it's valid, once validated run `honeycomb pack` again". And when the file is present but couldn't pass the validation state display an error message accordingly while keeping it detailed on how to actually resolve and test it again. Pack would be using validate as an internal task to actually validate.

`honeycomb validate <optional file parameter - by default .>` would similarly validate if everything has been provided and resolved if anything doesen't resolve show them the exact errors show them all the errors at once for the entire validation. Once the validation passes show run `honeycomb pack` to package it and get the .tar.gz file. 

Once the packaging completes it returns a tar.gz file with the targets and the honeycomb.yaml in the folder format defined above. For naming the file by default name it in the format: `appname(slugified)-version.tar.gz` otherwise during pack they can pass an aditional argument for the file name.


### App Registeration

For when a given application is registering themselves onto the system, they would need to provide: 

#### CLI

For cli they will be providing a .tar.gz format honeycomb compatible file, if it's provided via cli use the honeycomb validate command to validate the provided archive. Otherwise there must be backend endpoints to validate the provided archive. 

For storing the provided .tar.gz store it using OBO endpoints of briefcase https://docs.briefcase.teamofsilicons.com/obo/, also if the specific application is public the upload should be on anyone with the link can view. For storing all the archives store it in your app's public folder, but if it's a public app that anyone can use make it anyone with the link can view. 

And for the cli archive you just store this briefcase link. And as soon as a private app goes public make the briefcase link anyone with the link can view. 

CLI is compulsory for publishing the application. 


#### Webhook

For each application they would need to give a webhook url, and secret, and along with that the scope of updates the app expects is set this is for subscribing to the org level updates the app might want to know, this is mainly used by IAM. And the app can also define the scopes of the webhook:

| Category     | Changes requested                                                        |
| ------------ | ------------------------------------------------------------------------ |
| `membership` | Membership creation, removal, and reactivation; Silicon creation/removal |
| `updates`    | Other permitted updates, such as profile and organization changes        |
| `trust`      | Trust-related changes                                                    |
| `full`       | All categories the application is authorized to receive                  |


#### OBO Endpoints

For each application it should be possible to define the endpoints that other applications can use on behalf of an authenticated user. These endpoints are optional. The OBO definitions would be managed through Honeycomb, and only the application's owning organisation's current org_owner or org_admin can configure them.

For each endpoint they need to provide endpoint_id, path, metadata requirements (if any), and critical as true or false. They can also optionally set ttl_seconds to configure how long an OBO proof for that endpoint stays valid. The endpoint_id is the identifier other applications would use when requesting access. Once defined, the same endpoint_id cannot be moved to a different path; for a different path create a new endpoint_id. So say briefcase exposes files.upload, it could look like:

```json
{
  "endpoint_id": "files.upload",
  "path": "/v1/files",
  "metadata": {
    "filename": {"type": "string"},
    "content_type": {"type": "string"}
  },
  "critical": true,
  "ttl_seconds": 300
}
```

By default an OBO proof would be valid for 5 minutes (300 seconds) from issuance or one successful verification, whichever happens first. The provider application's org_owner or org_admin can change ttl_seconds for each endpoint through Honeycomb. If not provided it defaults to 300; if provided it must be a positive integer in seconds. Honeycomb would send this setting to IAM along with the endpoint configuration, and IAM would use the accepted value when issuing a new proof. The calling application cannot override it in an OBO request. Changing the TTL applies to newly issued proofs; existing proofs keep their original expiry, and retrying an exchange does not reset that expiry. Revocation and the other authorization checks still apply even before a proof expires.

The application that wants to use this endpoint would select the provider's app_id and endpoint_id in its app_scope.external through Honeycomb. These applications can belong to different organisations. If the requesting application is public and the endpoint is critical, the provider application's org_owner or org_admin needs to approve the request. Private requesting applications bypass this provider-approval step while remaining restricted to their owning organisation. Honeycomb would provide the request, discussion, approval and denial flow, including the provider's obo_review_message. IAM would still validate and record the approval, and the user must consent to the effective scopes before they can be used. The private-app exemption is not a provider approval and must not carry over when the app becomes public; all required critical-scope approvals must be accepted before public access is enabled.

Whenever the base_url, OBO endpoints, external scopes or approval decisions change, Honeycomb would send the relevant change to IAM through an authenticated Honeycomb management integration. IAM must verify both Honeycomb's authority and the acting user's current authority; a normal Honeycomb login token or an actor_id in the request is not enough on its own. Each change needs an idempotency key and a configuration revision, separate from the app release version. Honeycomb would show it as pending until IAM accepts it. Failed changes can be retried, and an older change must not replace a newer one. IAM would return the effective configuration and notify Honeycomb of later approval or revocation changes. After migration these definitions would be edited through Honeycomb, not independently through IAM.

IAM would keep the accepted OBO configuration so applications can continue discovering endpoints, requesting proofs and verifying them directly with IAM. Disabling an endpoint, withdrawing access or changing it from noncritical to critical must take effect in IAM's exchange and verification checks once accepted; previous consent must not bypass a newly required approval for a public requesting app. Private requesting apps retain the approval exemption, but endpoint availability, declared scopes, consent and resource permissions still apply. Honeycomb's own OBO calls, such as uploading archives to briefcase, follow this same flow using Honeycomb's own app credentials and consented scopes.


#### Base URL 

Base_url is required if the application exposes OBO endpoints. It must be a complete backend origin with the scheme and no trailing "/", for example `https://backend.iam.teamofsilicons.com`. Public applications' base URLs can be discovered by anyone without login. Private applications' base URLs require authentication and permission to access that application; unauthorized requests must not reveal the URL. Honeycomb and IAM must follow the same visibility rule. Discovering a URL does not grant permission to call the application's protected endpoints. 

#### App Scope

App_scope defines what information and actions an application can request on behalf of a user. This would be configured through Honeycomb by the application's current owning-organisation org_owner or org_admin. IAM would still enforce the accepted scopes during login, consent, API access, webhooks and OBO.

App_scope has two parts: iam for IAM permissions, and external for the app_id of another application's OBO endpoint. These external applications can belong to a different organisation. For example:

```json
{
  "app_scope": {
    "iam": ["self.identity.read", "self.profile.read"],
    "external": [
      {"app_id": "tos>briefcase", "endpoint_id": "files.upload"}
    ]
  }
}
```

Honeycomb would fetch the available IAM permissions from IAM's scope catalog and the available external scopes from the accepted OBO endpoint definitions. The catalog must identify the scope, its description, whether it is critical, and who approves it. The tables below describe the intended permissions; Honeycomb must use IAM's current catalog and eligibility rules when showing the choices. App_scope is separate from webhook_scope, which only selects notification categories.

##### Non Critical IAM Scope

| Proposed scope                         | What the app can access                                                    |
| -------------------------------------- | -------------------------------------------------------------------------- |
| `self.identity.read` (default checked) | Carbon ID or Silicon ID, and account type                                  |
| `self.profile.read` (default checked)  | Display name, profile photo, description, timezone                         |
| `self.email.read`                      | The Carbon’s verified email                                                |
| `self.phone.read`                      | The Carbon’s verified phone number                                         |
| `self.organizations.read`              | Selected organizations’ IDs, names, logos, descriptions                    |
| `self.membership.read`                 | The user’s membership status and owner/admin/member role                   |
| `self.capabilities.read`               | The user’s explicitly assigned organization capabilities                   |
| `self.job_role.read`                   | The user’s descriptive job role                                            |
| `self.tags.read`                       | Tags assigned to the user                                                  |
| `self.silicon_access.read`             | First Silicon, extra Silicon assignments, and Silicons the user can access |
| `self.hierarchy.read`                  | A Silicon’s own `reports_to` relationship                                  |
| `self.trust.read`                      | Effective trust involving the user, from the user’s perspective            |

##### Critical IAM Scope

| Proposed scope                             | What the app can access                                                                       |
| ------------------------------------------ | --------------------------------------------------------------------------------------------- |
| `directory.carbons.read`                   | List, search, and look up Carbons within a selected organization                              |
| `directory.silicons.read`                  | List, search, and look up Silicons within a selected organization                             |
| `directory.profiles.read`                  | Other members’ names, photos, descriptions, timezones                                         |
| `directory.memberships.read`               | Other members’ membership status and owner/admin/member roles                                 |
| `directory.capabilities.read`              | Other members’ explicit organization capabilities                                             |
| `directory.job_roles.read`                 | Other members’ descriptive job roles                                                          |
| `directory.tags.read`                      | Other members’ tag assignments, including membership of a tag                                 |
| `directory.silicon_access.read`            | Other Carbons’ first Silicon, extra Silicons, and accessible Silicon assignments              |
| `directory.hierarchy.read`                 | Silicon reporting relationships across the organization                                       |
| `organization.tags.read`                   | The organization’s complete tag catalog                                                       |
| `organization.trust.read`                  | Organization default trust, tag rules, exact Silicon overrides, inter-tag trust matrix        |
| `organization.invitations.read`            | Pending/historical invitations and invitation details                                         |
| `organization.governance.read`             | Role/tag change requests, approval decisions, role/tag history                                |
| `organizations.create`                     | Create an organization on behalf of a Carbon; that Carbon becomes its owner                   |
| `organization.profile.update`              | Change organization name, logo, description                                                   |
| `organization.invitations.create`          | Invite Carbons with the permitted invitation settings                                         |
| `organization.invitations.revoke`          | Revoke pending Carbon invitations                                                             |
| `organization.silicons.create`             | Create a Silicon account in the organization                                                  |
| `organization.silicons.update`             | Update Silicon profile details and reporting relationships                                    |
| `organization.carbons.remove`              | Remove Carbons from the organization                                                          |
| `organization.silicons.remove`             | Remove Silicons from the organization                                                         |
| `organization.tags.create`                 | Create tags                                                                                   |
| `organization.tags.update`                 | Edit tag definitions                                                                          |
| `organization.tags.delete`                 | Delete tags                                                                                   |
| `organization.member_tags.update`          | Assign or remove members’ tags                                                                |
| `organization.job_roles.update`            | Change members’ descriptive job roles                                                         |
| `organization.silicon_access.update`       | Change first Silicon and extra Silicon assignments                                            |
| `organization.trust.update`                | Change trust defaults and rules                                                               |
| `organization.admins.promote`              | Promote a Carbon member to admin                                                              |
| `organization.admins.demote`               | Remove a Carbon’s admin status                                                                |
| `organization.capabilities.update`         | Change an admin’s explicitly assigned capabilities                                            |
| `organization.invitations.read`            | List invitations and fetch their details/status                                               |
| `organization.invitations.create`          | Send Carbon invitations                                                                       |
| `organization.invitations.revoke`          | Revoke pending invitations                                                                    |
| `organization.change_requests.read`        | List and fetch role/tag change requests, including their decisions                            |
| `organization.job_role_changes.request`    | Submit a role-change request                                                                  |
| `organization.tag_changes.request`         | Submit a tag-change request                                                                   |
| `organization.change_requests.decide`      | Approve or reject a request the represented user is eligible to decide                        |
| `organization.job_role_history.read`       | View job-role change history                                                                  |
| `organization.tag_history.read`            | View tag-assignment history                                                                   |
| `organizations.join`                       | Complete invitation verification and join; still require a valid invitation or successful SSO |
| `organization.sso.read` / `.manage`        | View and configure SSO, including join-method settings                                        |
| `organization.silicons.credentials.rotate` | Rotate Silicon credentials separately from editing their profile                              |

The one's described above were the critical and non critical IAM scope, then except this each application should also be able to define other apps they would need OBO from. These could be any application's even outside the organisation. For the selected application they would need to select the scope of the external application - this scope can be selected based on the exposed OBO endpoints of the said application.

In case of IAM some of the scopes are gonna be limited only to the trusted organisations. IAM would remain responsible for deciding which organisations are trusted and which permissions each can request; Honeycomb must respect that eligibility and cannot grant it itself. Those scopes would include:

```
organization.silicons.credentials.rotate
organization.sso.read` / `.manage`
organization.invitations.create
organization.silicons.create
organization.member_tags.update
organization.job_roles.update
organization.silicon_access.update
organization.trust.update
organization.admins.promote
organization.capabilities.update
organization.change_requests.decide
organizations.join
organization.sso.read` / `.manage
```

This eligibility can be configured in IAM for each trusted organisation, including removing permissions from its allowed list. Honeycomb would use that configuration when showing and submitting scope requests. 

##### Scope Permissions

Private apps bypass provider approval for critical IAM and OBO scopes while login and selected organisation grants remain restricted to the owning organisation. IAM applies this exemption from its accepted app status, not a flag supplied by an OBO caller. The exemption only removes provider review; declared scopes, scope eligibility, user consent, resource permissions and OBO proof verification still apply.

If a public app adds a critical scope, or a private app requests public access, Honeycomb would create requests for any critical scopes that still need provider approval. For IAM scopes this goes to IAM's authorized scope reviewers. For another application's OBO scopes this goes to that provider application's current org_owner or org_admin. The requesting app's owner cannot approve their own request unless they independently have the required provider review authority. Honeycomb's publication approval does not approve these scopes.

Honeycomb would show the request and discussion in one place, using the provider's obo_review_message as the first message where configured. Both the requester and reviewer can reply. The reviewer can approve or deny, and denial requires a reason. Each decision is for a specific requesting app_id and set of scopes. IAM validates the reviewer's authority and records the effective approval; Honeycomb displays that accepted result.

The app can remove scopes without a new approval. Adding another critical scope to a public app needs a new request. A private app can use its accepted scopes under the private-app exemption without waiting for provider review. While its request to become public is pending, it remains private. A public app requesting more scopes continues working with its previously approved scopes until the new request is accepted. Public access must not be enabled until all required provider approvals and Honeycomb review pass, and IAM accepts the status change. Honeycomb must show requested app_scope separately from effective_app_scope so pending access is not shown as already granted.

For the initial request, send an acknowledgment email to the requester and notify the provider's reviewers with the app_id and message. Replies and approval or denial decisions should notify the other participants, with a link back to the discussion in Honeycomb. Notification delivery must avoid duplicate emails when the same operation is retried.

Honeycomb would send scope changes and review decisions to IAM through the authenticated management integration, with an idempotency key and configuration revision. IAM validates the acting user's authority, scope eligibility and any required approval before applying the change. Honeycomb keeps it pending until IAM confirms the accepted revision. IAM's approval and revocation notifications update the displayed result, and Honeycomb must be able to reconcile it if a notification is missed. After migration the scope configuration and request interface live in Honeycomb rather than a separate IAM app-management screen.

Whether access uses provider approval or the private-app exemption, the user must consent through IAM to the effective scope set. Public apps allow the user to select permitted organisations; private apps allow only the app's owning organisation. Scopes do not increase the user's own permissions. Removing or revoking access must take effect in IAM's current authorization checks; Honeycomb's saved configuration or a previously issued token is not enough to bypass those checks. These rules also apply to private applications and to Honeycomb's own app_scope.

IAM Default: So when an user request's an IAM approval for a critical scope display the first message as:
```
Define the need of each and every one of the critical scopes, why you wanna use them and what's the exact purpose they are gonna serve your application. Also give a detailed description of what exactly is it that you are building. 
```


#### App ID

For the App ID there would be an app handle so for eg: org tos is trying to create briefcase app, the app_id would be tos>briefcase. As each application is owned by an organisation, the user must only be able to configure the local_app_handle to set the app id. Once the app id is set it can't be changed.

#### App Details

App Name - Each application would have a name to it
App Logo - This would just take in app logo url also give the option to be upload a file - for file upload, use briefcase where the cli package was uploaded. Upload it there itself. 
App Version - For each new release, version update, etc take in the app version it should be possible to create a new version release any time, and when the cli is updated it automatically goes to the next version, user can also specifically mention the app version. 
App Description - Each app would have an app description linked to it, must be minimum 50 words maximum 1000 words. 
Website Link (optional) - the user must also be able to attach a website link, this can also be fetched by anyone who has access to the application. 
Docs Link (optional) - a docs link can also be attached, similarly this can also only be fetched by anyone who has access to the application itself.


#### App Secret

For app_secret, Honeycomb would ask IAM to generate it for the registered app_id. IAM stores the verification hash and returns the secret through the protected creation response. Honeycomb displays it to the authorized org_owner or org_admin to save in their application's backend, without keeping a permanent copy itself. The secret cannot be viewed again later; if lost, the owner must request a rotation through Honeycomb, which asks IAM to generate the replacement. Retrying the same creation or rotation during IAM's short replay window must return the same result rather than generate another secret. App secrets must never be included in CLI packages or honeycomb.yaml.


#### Who can register an app

For app registeration only the org admins and org owners should be able to create the application and submit the said proposal to make it public. So for apps they are gonna be limited to org admins and org owners. 


# Communicating with IAM

IAM itself would also be an application in Honeycomb, owned by its configured platform organisation. It would have its own app_id, app details, base_url, scopes, OBO definitions where exposed, and app releases with the combined CLI archive, just like the other applications. IAM's catalog entry and releases would be managed through Honeycomb, while IAM continues being the identity and authentication service for the entire system. Registering IAM as an app does not give its application credentials unrestricted IAM authority.

For the initial setup, first provision IAM's identity service and create or reuse the authentication records for IAM and Honeycomb. Then start Honeycomb using its own IAM credentials and create and link both application entries in Honeycomb using those same app_ids. This setup must be safe to retry without creating duplicate apps or rotating existing secrets. Initial IAM signup and login must work before Honeycomb's catalog is available, and the initial service deployment must not depend on downloading a release from Honeycomb. Once setup is complete, their app details and subsequent releases are managed through Honeycomb.

Honeycomb would manage the applications, but IAM would still handle their authentication, consent, webhooks and OBO. So whenever an application is registered or its IAM-related configuration changes, Honeycomb must send the relevant details to IAM. This includes app_id, org_id, name and logo used during login, base_url, webhook configuration, app_scope, OBO endpoints with their ttl_seconds, and the application's availability and private/public status. CLI archives, release files, descriptions and website/docs links stay managed by Honeycomb and do not need to be copied to IAM for authentication.

Honeycomb itself would be registered in IAM during platform setup, before Honeycomb needs to serve its first login. Its backend keeps its own app_id and app_secret and uses the normal IAM login and token exchange flow. Managing other applications needs a separately provisioned Honeycomb integration identity. For user-initiated changes, IAM must verify this integration identity together with the authenticated acting user's current organisation or provider-review authority and any required step-up. Honeycomb's normal app login is not enough to administer all applications, and its integration identity must not allow it to approve its own critical scope requests.

When sending a change, Honeycomb first saves the intended configuration and a durable pending operation. Each operation includes an idempotency key, the affected app_id and a configuration revision with the expected IAM revision. These revisions are separate from the app release version. IAM validates the change and returns the accepted revision and effective configuration. Honeycomb only shows the change as active after that confirmation. If the response is lost, retry the same operation with the same key; if IAM rejects it, show the error and keep the previously accepted configuration active. New apps cannot become usable until IAM accepts their required configuration and any approvals required for their visibility. Private apps use the critical-scope approval exemption; public apps require the relevant provider approvals. Security restrictions must also be acknowledged by IAM before Honeycomb reports them as enforced.

IAM would send signed management notifications to Honeycomb for scope decisions and revocations, webhook activation, credential-version changes, application availability changes and other changes to the security state Honeycomb displays. These are separate from Honeycomb's ordinary membership/profile webhook subscription. Verify the signature, ignore repeated event_ids and apply resource revisions in order, fetching the current state when a revision is missed. Notifications must not contain app_secrets, webhook signing secrets or user tokens. Honeycomb must also be able to fetch the current IAM state and reconcile pending operations if notifications are delayed or lost.

The normal IAM webhooks still go directly to each application's configured endpoint. Applications continue asking IAM for login, token checks, base_url discovery and OBO proofs, and they send actual OBO requests directly to the receiving application. Honeycomb is not in the middle of these runtime calls. Production and testing communication must stay in their own environments, with no fallback from test credentials or configuration to production.


# Step by step process for app registeration

For app registeration we follow a 3 step process:

1) App publishing - During the initial app publishing the app doesen't become public and becomes a private app for the organisation, only that org will be able to use the application in this step, login and everything should be supported at this step, this is a full e2e application that can be used as a private app, this won't need provider approval for critical IAM or OBO scopes. Declared scopes, user consent and normal authorization checks still apply. Only current members of the owning organisation can log in, and they can only grant access to that organisation. 

2) Request for Public - The said application would then be able to request a public release request, in this the request would go to the specific scope providers if there are any critical scopes to the application. The request would go to the said forum where the discussions would take place. And the request should always come to honeycomb itself for the final verification. 

3) Verified: Once all the scope holders verify the request the application would be allowed to go public and would officially be released and would start appearing in everyone's honeycomb store and the login restrictions would also be lifted. 

#### Release lifecycle:

For when the application is being filled for publishing keep drafting it so none of the data is lost and another admin or owner is also able to continue as needed. 

As soon as the request for public release has been requested they would need to write a message to each approval as we have defined and once that is done the application would go in Awaiting Approval. For the app owners they would see in an combined window all the application requests they have. And for honeycomb carbon's and silicon's might get a validator permission when they have the permission they would be able to give verification to the apps from honeycomb's side, it should come to honeycomb only after the critical scopes from the apps have been approved. 

Once honeycomb gives the final go, the app would be published for public. 


# Command collision

In case of command collision a command exists already for the installed command, it would ask for setting an alias for the new installation, which would set the alias, it should also be possible to set an alias at the time of installation. 

# honeycomb install

For installing any given application, the user should be able to run `honeycomb install {app-id}` which would install the cli for the application based on my current operating system, once the installation successfully takes place, show them {app-id} successfully installed, run {app-command} --help to get the help. 

# honeycomb uninstall

Similar to `honeycomb install {app-id}`  there would be `honeycomb uninstall {app-id}` which would uninstall the application from my system entirely. While uninstalling also add you can give a review using `honeycomb review {app-id} --rating 4.7 --review "this was a very good application, but need to retire."`

# honeycomb update

There should be `honeycomb update {app-id}` which would check if there any any updates for the said package cli and if there is update the said package otheriwse just return package already upto date. 


# Email

We use postmark as our mail provider. You have an email at [honeycomb@teamofsilicons.com], this email will be used to notify the users about their scope status - whenever any party replies to the scope request thread an email must go to the org owners and org admins, verification status - for when the app is verified or canceled, or stopped, or anyone denies the request it comes here, new version release - whenever a new version of the app is release mail the admins and the owners, congratulating a new version was just released.


# Honeycomb Store

We would also have honeycomb store, for all our public apps on the system anyone would be able to run `honeycomb search <search term>`, for the said search term it will search the entire directory of all the public apps + private apps (if logged in and has access to). There would also be backend endpoints supporting it. For searching it won't be a restricted endpoint and anyone should be able to search. 

### Search

Applications can be searched by app_id, name and description. Search should support partial names and spelling mistakes. Exact app_id and name matches should appear first, with strong name matches ranked above description-only matches. Use weighted full-text search for names and descriptions, together with trigram similarity for misspelled names. Only applications the user is allowed to discover should be included, and results should be paginated (20 results per page). Search relevance is the primary ordering; a higher rating only ranks an app higher when relevance scores are equal. Break any remaining ties by app_id so pagination is stable. A higher rating must not move a weaker match above a stronger match. Also display the rating for each application. 


### Application Metrics

For each application any carbon/silicon should be able to star the application, and also give it a review. For each the application is downloaded it would also add +1 to the installs for the application. 


### Log in honeycomb store

A silicon/carbon should also be able to login to honeycomb, once they login to honeycomb they would be able to start giving reviews, and also be able to install to install apps private to their organisation. 


### Private Apps

Private apps are entirely restricted to their owning organisation and do not appear in public search. Their base URLs and downloads require access to the application. They bypass provider approval for critical IAM and OBO scopes, but still require declared scopes, user consent and normal authorization. IAM must enforce current membership and the owning-organisation restriction during login, token exchange, refresh, introspection and OBO exchange/verification. A user losing that membership loses access even if they still hold a previously issued token. When becoming public, the app stays private until all required critical-scope approvals, Honeycomb review and IAM activation are complete. 



# Testing Environments

Honeycomb would own testing environments for the entire ecosystem, including IAM, Honeycomb and all the other applications. Creating, listing, configuring, importing apps, rotating keys, cleaning, deleting, restoring and automatically retiring environments would all be managed through Honeycomb. IAM would provide the isolated identity and authentication services inside each environment, but would no longer independently manage the testing lifecycle for other apps. Each application remains responsible for its own test data and behavior; Honeycomb coordinates the environment rather than building or deploying another copy of every application.

### Creating a Test Environment

There can be multiple testing environments per organisation. Any current Carbon or Silicon member can create one using a name and optional description. The environment belongs to the organisation, with the initiating user recorded as its creator. The creator, while still a member, and the current org_owner and org_admins can manage it. Honeycomb generates an environment_id and a cryptographically random 32-character alphanumeric testing_key. Anyone holding this key has root authority inside that test environment, but it gives no authority over production or another environment. Honeycomb stores the key encrypted and allows authorized managers to retrieve it with an audit record. Ordinary environment lists must not include the key.

Applications must also be able to create an environment for themselves through Honeycomb using their production app_id and app_secret, which Honeycomb verifies with IAM. The request takes name, optional description and an optional existing testing_key. If the key is supplied, attach to that active environment; if invalid, return an error rather than create a new one. If omitted, create a new environment owned by the application's organisation. The creating application can manage the environment it created. Attaching an app or importing it as a dependency does not give its production credentials ownership or access to the environment's root key.

Honeycomb returns the environment_id, organisation, state and the test credentials the caller is authorized to receive. It must support listing the environments linked to an app, including can_manage, last activity, retention and any pending setup errors. Keep the existing iam_test_key request field as a compatibility alias for testing_key while existing apps migrate; if both are supplied with different values, reject the request.

### Preparing the Environment and Its Applications

Honeycomb first creates the environment record and asks IAM to prepare the matching isolated identity environment using the same environment_id and key version. IAM creates the test identities and application authentication records and generates fresh test app_secrets. Honeycomb coordinates the remaining applications through their authenticated testing lifecycle integration. Every application must support preparing, cleaning, disabling, restoring and purging its environment-specific data and reporting completion. These control operations use service authentication outside the test sessions being erased, so cleanup can finish even after IAM has invalidated those sessions.

For app setup, Honeycomb follows app_scope.external recursively and imports all the required applications into the same environment. So say app A needs B, and B needs C and D, one request for A prepares A, B, C and D. There is no fixed dependency-depth limit; repeated apps are prepared once, and cycles must not create an infinite loop. Keep each app's own organisation identity even when dependencies belong to different organisations. These are testing relationships, not CLI package dependency management. Existing visibility and import permissions still apply; a dependency declaration does not give access to an otherwise inaccessible private app.

Each import records the source app configuration revision and selected release where relevant. Copy the app's identity, base_url, scopes, OBO definitions including ttl_seconds, and webhook configuration into its test record. Do not copy production users, sessions, business data or production app_secrets. Later production configuration changes do not silently alter an existing test setup. Refreshing an import must be explicit and must revalidate its dependencies.

The environment stays provisioning until IAM and all requested applications acknowledge readiness. If a step fails, show which app failed and why and allow retrying that same operation. Do not claim the full environment is ready after only IAM succeeds. Attaching another app must not clean an existing environment or interrupt its already-ready apps.

### Apps Inside a Test Environment

It should be possible to create a test-only application or import an existing production application. A new test-only app_id must not claim an ID already used by a production app. Importing keeps the original app_id and owning organisation. So importing google>drive creates or uses google inside the test environment and keeps the app as google>drive; it cannot become another organisation's app. The environment's root authority can create test owners and administrators for that test organisation without changing production ownership.

Honeycomb would have its own isolated app catalog, releases, review records and store data for the environment. IAM's test identities, scope decisions and consents belong to that same environment. Test uploads must go to Briefcase's isolated test storage. Test publications, downloads, ratings, reviews and installs must not appear in the production store or alter production metrics. Each application must isolate its own files, caches, jobs and business records as well as database rows.

### Using a Test Environment

Use the same application APIs with X-Testing-Environment-Key selecting the test environment. Each service keeps test data separate from its production database and associates test records with environment_id. Access tokens, refresh tokens, SLTs, STKs, app_secrets, sessions and OBO proofs must resolve to the same environment. Requests with invalid, expired, deleted or mismatched test context must fail; they must never fall back to production.

When an application receives a test app_secret in a request, that is a signal to enter the test flow, not proof that the request is authorized. It must validate that app_id and secret with IAM using the environment key, resolve the environment_id, and then use only that environment's data. An app_secret does not identify an end user; ordinary user login and authorization still apply when acting for a Carbon or Silicon. Any recipient test credentials carried through OBO are forwarded only to the matching recipient, never exposed as production credentials.

IAM continues issuing and verifying test login tokens and OBO proofs. Test callers discover the base_url and OBO configuration belonging to their selected environment. Critical-scope rules follow the app's test visibility: private apps use the private-app exemption with owning-organisation restrictions; public test apps use approvals recorded inside that environment. Test approvals never grant production access.

For test email and phone verification use 000000. IAM still applies challenge expiry, attempt limits and session checks. Test mode must not send real verification emails or SMS. Honeycomb's publication and scope-review emails must also be captured in the environment instead of sent to real users. Other applications must keep their test side effects isolated.

### Test Webhooks

Test webhooks keep the separate test envelope, for example:

```json
{
  "test": {
    "testing_key": "<environment key>",
    "metadata": {
      "event_id": "<event id>",
      "environment_id": "<environment id>",
      "generation": 1
    },
    "data": {}
  }
}
```

Verify the signature over the complete raw body, validate the environment key, and process the event only in that environment. The testing_key is root authority and must be redacted from logs, traces and stored event payloads. Deduplicate by event_id and use the resource revision for ordering. Events from an earlier cleaning generation must not recreate erased test data.

IAM can internally retain an imported application's existing webhook signing key so its receiver can verify test deliveries, but must not reveal the production key to Honeycomb or the tester. Replacing a test webhook destination installs a supplied or newly generated test-only signing secret and activates that test destination immediately. It must not change the production destination or signing key.

### Rotate, Clean, Delete and Restore

Honeycomb manages key rotation and sends the new key version to IAM and the linked applications through protected service communication. The old key must stop authorizing new requests. Services must validate the current environment state and key version; a cached old key cannot keep granting access. While rotation is incomplete, affected test access is blocked rather than accepting both versions indefinitely. Rotation completes only when the required services acknowledge it.

Cleaning keeps the environment identity and current key, but permanently clears its test data across Honeycomb, IAM and the linked applications. An authorized manager or holder of the environment root key can request it. Honeycomb marks the environment as cleaning, blocks test access, increments its generation and coordinates cleanup. Old users, app imports, sessions and proofs are invalid after cleaning. Reimport the required applications and obtain fresh test credentials for the next run. Show completion only after all participating services confirm; failed cleanup remains visible and retryable.

Deleting an environment first disables its key and test access everywhere, then starts a 30-day recovery window. Authorized managers can restore it before that deadline; the environment becomes usable only after its services confirm restoration. A restore does not undo a previous clean. After the deadline, Honeycomb coordinates permanent removal of the remaining test data and keys. Keep only the control audit needed to record who performed the operation and whether it completed, without retaining test secrets or erased payloads.

### Inactivity and Retention

By default, retire an application's test instance after 30 days without activity. Its org_owner or org_admin can configure testing_idle_days through Honeycomb. Applications report environment activity and Honeycomb tracks it per app, so retiring an idle app must not remove another app's active data. An instance still needed by an active linked application must not be retired underneath it.

An environment also defaults to soft deletion after 30 idle days, followed by the 30-day recovery window. Active app links and longer configured retention must be taken into account before retiring the shared environment. Cleanup and permanent deletion include Briefcase test files and other application-owned storage, not just IAM identities.

### Reliable Lifecycle and Migration

Every lifecycle mutation requires an idempotency key and the expected environment revision. Honeycomb keeps durable per-service progress, retries failed steps without repeating completed work, and exposes pending, completed and failed states through its backend, client, CLI and console. IAM and the applications send authenticated receipts and lifecycle notifications; Honeycomb can reconcile their current state if a notification is lost. Test key holders remain confined to their environment and cannot use these integrations to administer production.

Existing IAM-managed environments must be imported into Honeycomb with their IDs, organisations, creators, keys, app links, retention and active credentials preserved. After reconciliation, Honeycomb becomes the only lifecycle authority and IAM's previous management entry points must forward to it or be retired compatibly. IAM continues running the test authentication APIs, and each application continues running its own isolated test behavior. This section defines the intended ownership change; it does not imply the integration is already implemented.


# Backend Versioning

For versioning we have Contract Governance/API/service contract lifecycle management. We will have:

1) Contract versioning / API versioning
2) Protocol Negotiation
3) Backward compatibility
4) Consumer-driven contract testing
5) Deprecation and sunset management - if 0 requests for 7 days, sunset that version
6) Compatibility matrix
7) Version policy



---
---
---
---
---
---
---
---
---
---
---
---
---

Only above this line is what the honeycomb backend would hold, below this would be the users of the backend, the client, the frontend, the cli, etc. 

# Rust Package & CLI

The Rust package & cli using that rust package are first hand client with an always running deamon if needed in the background. the UI will be a subset of the cli. make sure everything works via the CLI first, and then we'll make the UI. Everyone should be able to use the CLI/Rust Package (carbons, silicons, org, access keys, api keys, read, write, patch, delete, everything).

The rust package would be stateless whereas the cli would be statefull. CLI built on top of the rust package.

For how this CLI is built, rust as the programming language, but can use anything under the hood that is needed. Maybe rust, or node, or shell, as and when the work comes. That is decided by the implementor based on the work. If something requirs a UI (like graph, live, video, images etc). for that the UI has an endpoint that can be viewed/used/downloaded and the cli gives the link to that.

The primary Interface is the Rust Package. CLI is built using the Rust Package only and doesn't have any feature that the Rust package does not.

if you need a local store for auth or something else, use `{home_dir}/.{appname}/dir`.

The default home dir is `~`. If `SILICON_HOME` is present in the enviorment variables, use that as the home directory by default. 

For both package and the cli write detailed docs on how to use the package and how to use the cli, and also another doc on how to use the package. 

Package and CLI must only expose the client side actions, and not the internal actions performed by the backend. For the CLI follow the standard command line grammar rules, and also include a -h command that shows all the possible commands.

Testing in the test enviorment should also be possible via both cli, and the package. 

Testing enviorment in cli, for testing enviorment in cli i should just be able to `honeycomb --test <test_id> <command>` infront of the same command and it should treat that as a test command. Same for test only commands even they would have the same style just without specifying --test for them would return this action is only possible for test enviorment.  

--- logging in via cli ---

For logging in via the cli or the package for any carbon/silicon you don't ask for their credentials or redirect them anywhere, instead you just request for their short lived token. This short lived token would then be used for the same login logic, the short lived token would be compared and you will get the refresh and auth token. 

For CLI login there should be this exact command: `honeycomb login <slt>`. 
And there should be an command to configure the home directory where the information is stored:  `{home_dir}/.{appname}/dir`. This can be confitgure via `honeycomb config home {location}`. If it's not a directory give an error not a directory. 

The default home dir is `~`.

For both cli and client we would also package in an auto updater, the task of this auto updater is to compare the current version to the latest version in crates for them, and if there's a new verion auto update it to the said new version. By default auto update is on, users can specifically come and opt in to stop auto update. Which would stop auto updating the package. Auto updater check runs every single hour. Updates should be checked when the command is run and should happen every hour, so check for the last update check time and if it's past 1 hour old check for update and update after the command finishes running.

It should also expose these specific commands:
1) `--help` which would give all the help documentation on how to use waveform. So the user should be able to run `honeycomb --help` and get the help docs.
2) `iam --json` the user should be able to run  `honeycomb iam --json` which returns `app_id` alongside other information.
3) `login status --json` the user should be able to run `honeycomb login status --json`, reports successful authentication reports `authenticated: true`, alongside which carbon or silicon is it authenticated as.


# Cli experience

CLI is the primary way to interact with IAM Apps. It should be built for both Carbons & Silicons. Any other interface (like website) will be a subset of the CLI.

The cli should never ask for credentials from either silicon or carbon. it should just ask for short lived tokens that the user can generate from the official iam cli, or from the web where the the user is sent to auth concent screen.

CLIs get SILICON_HOME env variable where it should store all the details. Its home, so you should use that as base, and make their own hidden folders to keep their information.

Specific apps that could benefit from using ISI env variable should do that. eg: dm.

ISI are internal silicons. If silicon is a brain, then isi are parts of the brain. store this inside metadata, or main data if its super useful. ISI may or may not be present. make sure to not rely on it in such a way that things break. consider ISI as useful additional information.

every app cli must support the following commands:

`app iam --json` gives {app_id: "...", ...}

`app login "..."` takes in a short lived auth token generated by silicon interpretter.

`app login status --json` tells if its {authenticated: true, ...}


App Internals:
All apps are suggested to make a rust library which is stateless. then 2 things that uses the rust library: always running daemon, and a cli interface that talks to the daemon.

At the end, on the docs page, there should be one curl + sh command to run to install and get everything setup to start using it. not auth, just technical setup on the system like installing the right set of things.

CLI design should be focused on giving details and helping finding the right command to use. CLI will often have lots of commands and it should be like a tree that can be traversed using --help.

CLI documentation should be bundled inside the cli itself. On each print of the cli documentation using --help or otherwise, it should show what this command is for, how its often used (perhaps in conjunction with other commands if applicable) and then a list of flags etc it takes in.

Follow the CLI grammar. These CLIs can be used by humans, but more often than not, it'll be used by an agent who prefers to know why something broke and so it can figure out ways to fix it. Don't just say something went wrong... tell it exactly what & why.

A good rule of thumb is: these CLIs are being made for someone who understands ins-and-outs of technology. Make like a programming language that gives very specific and helpful errors and outputs compared to a web interface where all errors are hidden until absolutely critical.

All CLIs must have a report bug feature that also optionally takes in a PR ref if the agent did not just find a bug but also patched it. 

honeycomb report `<report-message>` --pr `<pr-link>` and if someone just reports the bug, without the pr, show them a message, you can also put a pr in the repo (`repo-link`). 

Everytime a bug is reported use postmark to mail [saketdev12@gmail.com, shubhastro2@gmails.com, bugs@teamofsilicons.com]

Since all TOS applications are open sourced, any bug can be discovered, replicated, patched and a pr can be raised. Allow all such edge cases be figured out by the agent instead of fixing it ourselves based on a bug report.

Only a bug report submitting is possible, but its encouraged to give a lot more details and also attach a PR if possible.

Give the information of the github repo, online docs, rust package, etc inside the cli itself.

The CLI as i told before is a tree of documentation. Show possible paths, and then let someone go deeper along with documentation.


# Docs

There are two kinds of documentations: informative & instructive.

Always keep instructive documentation up front, easy to use, direct with clear instructions & link to informative documents to know why its done this way. Instructive documents should be the landing point of the product for both carbons & silicons.

It can give carbon the instructions on how to install & use it, or how to ask their silicon to use it.

For silicons, it can be that, but also how to do a lot more with it. Esp. things like building on top of it. Make it very clear what is expected, what is mandatory and how does the system work.

Then the silicon can dig deeper into the informative documentation to know all the possible ways to do it, & why its done the way its done.

While both carbons and silicons can read the documentation, it'll likely be more silicon. So design it for silicons. The more reasons you give, the better a silicon would be at making a judgement call of how to do something.

Since all IAM apps can both be used as is, and also built on top of... its imp to write documentation for both. Usage docs & Development docs.

# Telemetry

All IAM apps use Space Station [https://spacestation.teamofsilicons.com/docs] for telemetry. Telemetry is opted-in by default but can be opted out from settings if the user wants.

Space Station is also a rust package which can be used from within the backend, or daemon, or cli to send telemetry.

Record as many things as you think might be useful to diagnose or follow traces later.

Since space station is just an event store, make sure to include all the source, step, progress, etc information inside each event. some of the system information is automatically added to the metadata so you need not add that.

push context-rich, self-contained events.

Space Station also support web, for web it has 2 possible pathways: analytics & events. Most of the Analytics is self captured and you can define a seperate event store from the web.


# Configurability

We ship highly configurable apps with sensible defaults. Very much like VS Code. flags to toggle / customize behaviors.


# Updates

All CLIs when installed, within their daemon run a update checker hourly. Update the CLI to the newest one if a update is found. Don't rely on user usage to check for updates.