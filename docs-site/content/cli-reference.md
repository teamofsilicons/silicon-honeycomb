This reference is generated from the actual CLI command parser. Every command supports `--help`. Global flags can be supplied with subcommands. Examples with credentials print sensitive results only when that workflow explicitly returns them.

## honeycomb

```text
Honeycomb is the application library for Carbons and Silicons.

Start: honeycomb search briefcase
Authenticate: honeycomb login <slt>
Publish: honeycomb validate → honeycomb pack → honeycomb apps create → honeycomb releases upload

Repository: https://github.com/teamofsilicons/silicon-honeycomb
Library: https://honeycomb.teamofsilicons.com
Rust client: https://crates.io/crates/silicon-honeycomb-client

Every command supports --help. Quote app identifiers: 'briefcase'.

Usage: honeycomb [OPTIONS] <COMMAND>

Commands:
  license       Print Honeycomb's MIT license notice without contacting a service
  iam           Discover the configured IAM app identity and login URL
  login         Exchange an IAM short-lived token, or use `login status` to check live authentication
  logout        Revoke the IAM session and refresh family, then clear the local session
  search        Search public apps and private apps visible to your current memberships
  validate      Validate honeycomb.yaml and all six required targets, or a .tar.gz archive
  pack          Validate and build an immutable .tar.gz. Creates a template if honeycomb.yaml is absent
  install       Install the latest or selected app release for this platform. Collision errors suggest --alias
  update        Update an installed app to its latest release, preserving aliases
  uninstall     Uninstall only files and launchers owned by this package
  installed     Show packages installed in the selected home and testing environment
  review        Save a rating and review for an application you can access
  report        Report a reproducible bug; optionally link a pull request with its fix
  star          Star an application; repeat safely. Use --remove to remove the star
  apps          Create, inspect and configure organization-owned applications
  releases      Upload and inspect immutable CLI releases. Run pack before upload
  publication   Request publication and participate in application review discussions
  bundles       Manage bundled sign-in applications through Honeycomb and IAM
  drafts        Save shared drafts with revision checks, allowing another admin to continue
  operations    Inspect pending operations or retry after repairing an integration
  environments  Create and manage isolated ecosystem testing environments
  config        Configure home, backend and update/telemetry preferences
  daemon        Run the shared CLI/app update worker at second 01 of every minute
  self-update   Check for and install the latest verified Honeycomb CLI binary
  service       Register the per-user CLI/app update worker
  help          Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost

          [env: HONEYCOMB_API_URL=]

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key

      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands

      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## honeycomb license

```text
Print Honeycomb's MIT license notice without contacting a service

Usage: honeycomb license [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb iam

```text
Discover the configured IAM app identity and login URL

Usage: honeycomb iam [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb login

```text
Exchange an IAM short-lived token, or use `login status` to check live authentication

Usage: honeycomb login [OPTIONS] <SLT_OR_STATUS>

Arguments:
  <SLT_OR_STATUS>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb logout

```text
Revoke the IAM session and refresh family, then clear the local session

Usage: honeycomb logout [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb search

```text
Search public apps and private apps visible to your current memberships

Usage: honeycomb search [OPTIONS] [QUERY]

Arguments:
  [QUERY]  [default: ""]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --page <PAGE>
          [default: 1]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb validate

```text
Validate honeycomb.yaml and all six required targets, or a .tar.gz archive

Usage: honeycomb validate [OPTIONS] [PATH]

Arguments:
  [PATH]  [default: .]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb pack

```text
Validate and build an immutable .tar.gz. Creates a template if honeycomb.yaml is absent

Usage: honeycomb pack [OPTIONS] [PATH]

Arguments:
  [PATH]  [default: .]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
  -o, --output <OUTPUT>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb install

```text
Install the latest or selected app release for this platform. Collision errors suggest --alias

Usage: honeycomb install [OPTIONS] <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --version <VERSION>

      --alias <ALIAS>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --rewrite-existing
          Answer yes in advance to rewriting an older command this package requires
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
      --switch-channel
          Confirm switching an existing installation between production and dev releases
  -h, --help
          Print help
```

## honeycomb update

```text
Update an installed app to its latest release, preserving aliases

Usage: honeycomb update [OPTIONS] <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb uninstall

```text
Uninstall only files and launchers owned by this package

Usage: honeycomb uninstall [OPTIONS] <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb installed

```text
Show packages installed in the selected home and testing environment

Usage: honeycomb installed [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb review

```text
Save a rating and review for an application you can access

Usage: honeycomb review [OPTIONS] --rating <RATING> --review <REVIEW> <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --rating <RATING>

      --review <REVIEW>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb report

```text
Report a reproducible bug; optionally link a pull request with its fix

Usage: honeycomb report [OPTIONS] <MESSAGE>

Arguments:
  <MESSAGE>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --pr <PR>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb star

```text
Star an application; repeat safely. Use --remove to remove the star

Usage: honeycomb star [OPTIONS] <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --remove

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps

```text
Create, inspect and configure organization-owned applications

Usage: honeycomb apps [OPTIONS] <COMMAND>

Commands:
  organization   List every app in an organization you belong to, including private apps
  upload-logo    Upload a publicly viewable logo to Briefcase. Set the returned logo_url in application.json
  webhook        Manage pending webhook destinations and signing credentials through IAM
  reconcile      Refresh accepted IAM state and retry notification reconciliation
  rotate-secret  Replace an application's secret through IAM. Save the one-time result securely
  list
  get
  create         Submit application.json containing org_id, app_id, details, webhook and scopes
  update         Update using a complete application.json and the current revision from apps get
  help           Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps organization

```text
List every app in an organization you belong to, including private apps

Usage: honeycomb apps organization [OPTIONS] <ORG_ID>

Arguments:
  <ORG_ID>

Options:
      --after <AFTER>

      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --limit <LIMIT>
          [default: 100]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps upload-logo

```text
Upload a publicly viewable logo to Briefcase. Set the returned logo_url in application.json

Usage: honeycomb apps upload-logo [OPTIONS] <ORG_ID> <FILE>

Arguments:
  <ORG_ID>
  <FILE>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps webhook

```text
Manage pending webhook destinations and signing credentials through IAM

Usage: honeycomb apps webhook [OPTIONS] <COMMAND>

Commands:
  status         Read the current pending endpoint ID and IAM revision before making a change
  approve        Approve the exact pending destination. Obtain application.webhook.approve step-up from IAM
  rotate-secret  Install a signing secret from a protected file. Update your receiver to verify it
  retry          Retry a saved change by operation ID; omit the secret and original endpoint
  help           Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps webhook status

```text
Read the current pending endpoint ID and IAM revision before making a change

Usage: honeycomb apps webhook status [OPTIONS] <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps webhook approve

```text
Approve the exact pending destination. Obtain application.webhook.approve step-up from IAM

Usage: honeycomb apps webhook approve [OPTIONS] --endpoint <ENDPOINT> --revision <REVISION> <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --endpoint <ENDPOINT>

      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --step-up-file <STEP_UP_FILE>

      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps webhook rotate-secret

```text
Install a signing secret from a protected file. Update your receiver to verify it

Usage: honeycomb apps webhook rotate-secret [OPTIONS] --secret-file <SECRET_FILE> --revision <REVISION> <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --secret-file <SECRET_FILE>

      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --step-up-file <STEP_UP_FILE>

      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps webhook retry

```text
Retry a saved change by operation ID; omit the secret and original endpoint

Usage: honeycomb apps webhook retry [OPTIONS] <OPERATION_ID>

Arguments:
  <OPERATION_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --step-up-file <STEP_UP_FILE>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps reconcile

```text
Refresh accepted IAM state and retry notification reconciliation

Usage: honeycomb apps reconcile [OPTIONS] <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps rotate-secret

```text
Replace an application's secret through IAM. Save the one-time result securely

Usage: honeycomb apps rotate-secret [OPTIONS] --revision <REVISION> <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --revision <REVISION>

      --step-up-file <STEP_UP_FILE>
          Read fresh IAM step-up evidence from a file, avoiding shell-history exposure
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps list

```text
Usage: honeycomb apps list [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --page <PAGE>
          [default: 1]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps get

```text
Usage: honeycomb apps get [OPTIONS] <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps create

```text
Submit application.json containing org_id, app_id, details, webhook and scopes

Usage: honeycomb apps create [OPTIONS] <FILE>

Arguments:
  <FILE>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb apps update

```text
Update using a complete application.json and the current revision from apps get

Usage: honeycomb apps update [OPTIONS] --revision <REVISION> <APP_ID> <FILE>

Arguments:
  <APP_ID>
  <FILE>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb releases

```text
Upload and inspect immutable CLI releases. Run pack before upload

Usage: honeycomb releases [OPTIONS] <COMMAND>

Commands:
  list
  upload
  promote  Promote a dev archive to a new production release with an explicit version
  help     Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb releases list

```text
Usage: honeycomb releases list [OPTIONS] <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --channel <CHANNEL>
          [default: prod]
      --include-private
          Include private pending releases (requires application management access)
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb releases upload

```text
Usage: honeycomb releases upload [OPTIONS] --channel <CHANNEL> --revision <REVISION> <APP_ID> <ARCHIVE>

Arguments:
  <APP_ID>
  <ARCHIVE>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --channel <CHANNEL>

      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb releases promote

```text
Promote a dev archive to a new production release with an explicit version

Usage: honeycomb releases promote [OPTIONS] --revision <REVISION> <APP_ID> <DEV_VERSION>

Arguments:
  <APP_ID>
  <DEV_VERSION>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --version <VERSION>
          Production version (x.y.z). Interactive terminals prompt when omitted
      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb publication

```text
Request publication and participate in application review discussions

Usage: honeycomb publication [OPTIONS] <COMMAND>

Commands:
  activate      Activate an approved public revision and reconcile archive access
  inbox         Requests you can review as a provider administrator or authorized validator
  review
  retry-plan
  decide
  review-reply
  get
  request
  reply
  help          Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb publication activate

```text
Activate an approved public revision and reconcile archive access

Usage: honeycomb publication activate [OPTIONS] --revision <REVISION> <REQUEST_ID>

Arguments:
  <REQUEST_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb publication inbox

```text
Requests you can review as a provider administrator or authorized validator

Usage: honeycomb publication inbox [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb publication review

```text
Usage: honeycomb publication review [OPTIONS] <REQUEST_ID> <PROVIDER>

Arguments:
  <REQUEST_ID>
  <PROVIDER>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb publication retry-plan

```text
Usage: honeycomb publication retry-plan [OPTIONS] --revision <REVISION> <REQUEST_ID>

Arguments:
  <REQUEST_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb publication decide

```text
Usage: honeycomb publication decide [OPTIONS] --revision <REVISION> <REQUEST_ID> <PROVIDER> <DECISION>

Arguments:
  <REQUEST_ID>
  <PROVIDER>
  <DECISION>    [possible values: approve, deny]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --reason <REASON>
          [default: ""]
      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb publication review-reply

```text
Usage: honeycomb publication review-reply [OPTIONS] --message <MESSAGE> <REQUEST_ID> <PROVIDER>

Arguments:
  <REQUEST_ID>
  <PROVIDER>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --message <MESSAGE>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb publication get

```text
Usage: honeycomb publication get [OPTIONS] <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb publication request

```text
Usage: honeycomb publication request [OPTIONS] --message <MESSAGE> --revision <REVISION> <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --message <MESSAGE>

      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb publication reply

```text
Usage: honeycomb publication reply [OPTIONS] --message <MESSAGE> <APP_ID>

Arguments:
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --message <MESSAGE>

      --provider <PROVIDER>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb bundles

```text
Manage bundled sign-in applications through Honeycomb and IAM

Usage: honeycomb bundles [OPTIONS] <COMMAND>

Commands:
  list
  get
  configure  Configure members; use revision zero only when the bundle does not exist
  help       Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb bundles list

```text
Usage: honeycomb bundles list [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb bundles get

```text
Usage: honeycomb bundles get [OPTIONS] <BUNDLE_ID>

Arguments:
  <BUNDLE_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb bundles configure

```text
Configure members; use revision zero only when the bundle does not exist

Usage: honeycomb bundles configure [OPTIONS] --revision <REVISION> <BUNDLE_ID> <FILE>

Arguments:
  <BUNDLE_ID>
  <FILE>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb drafts

```text
Save shared drafts with revision checks, allowing another admin to continue

Usage: honeycomb drafts [OPTIONS] <COMMAND>

Commands:
  list
  save
  help  Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb drafts list

```text
Usage: honeycomb drafts list [OPTIONS] <ORG_ID>

Arguments:
  <ORG_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb drafts save

```text
Usage: honeycomb drafts save [OPTIONS] <ORG_ID> <ID> <FILE>

Arguments:
  <ORG_ID>
  <ID>
  <FILE>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --revision <REVISION>
          [default: 0]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb operations

```text
Inspect pending operations or retry after repairing an integration

Usage: honeycomb operations [OPTIONS] <COMMAND>

Commands:
  recover-secret  Recover a creation/rotation secret within IAM's short replay window
  get
  retry
  help            Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb operations recover-secret

```text
Recover a creation/rotation secret within IAM's short replay window

Usage: honeycomb operations recover-secret [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb operations get

```text
Usage: honeycomb operations get [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb operations retry

```text
Usage: honeycomb operations retry [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb environments

```text
Create and manage isolated ecosystem testing environments

Usage: honeycomb environments [OPTIONS] <COMMAND>

Commands:
  retention      Inspect activity and retention deadlines without extending the idle timer
  set-retention  Set the shared environment idle period. App-specific longer retention still applies
  activity       Report actual use of an application in a ready environment; supports --test root authority
  list
  get
  create
  import         Import an application's external-scope dependencies and pin accepted configurations
  key            Retrieve and save an environment root key. This action is audited by the backend
  action         Coordinate rotation, clean, delete or restore with every participating service
  help           Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb environments retention

```text
Inspect activity and retention deadlines without extending the idle timer

Usage: honeycomb environments retention [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb environments set-retention

```text
Set the shared environment idle period. App-specific longer retention still applies

Usage: honeycomb environments set-retention [OPTIONS] --days <DAYS> --revision <REVISION> <ID>

Arguments:
  <ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --days <DAYS>

      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb environments activity

```text
Report actual use of an application in a ready environment; supports --test root authority

Usage: honeycomb environments activity [OPTIONS] --generation <GENERATION> --key-version <KEY_VERSION> <ID> <APP_ID>

Arguments:
  <ID>
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --generation <GENERATION>

      --key-version <KEY_VERSION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb environments list

```text
Usage: honeycomb environments list [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb environments get

```text
Usage: honeycomb environments get [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb environments create

```text
Usage: honeycomb environments create [OPTIONS] <ORG_ID> <NAME>

Arguments:
  <ORG_ID>
  <NAME>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --description <DESCRIPTION>
          [default: ""]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb environments import

```text
Import an application's external-scope dependencies and pin accepted configurations

Usage: honeycomb environments import [OPTIONS] --revision <REVISION> <ID> <APP_ID>

Arguments:
  <ID>
  <APP_ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --revision <REVISION>

      --release <RELEASE>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --refresh
          Explicitly refresh the imported dependency graph from accepted production configurations
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb environments key

```text
Retrieve and save an environment root key. This action is audited by the backend

Usage: honeycomb environments key [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb environments action

```text
Coordinate rotation, clean, delete or restore with every participating service

Usage: honeycomb environments action [OPTIONS] --revision <REVISION> <ID> <ACTION>

Arguments:
  <ID>
  <ACTION>  [possible values: rotate-key, clean, delete, restore, retry, purge]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --revision <REVISION>

      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb config

```text
Configure home, backend and update/telemetry preferences

Usage: honeycomb config [OPTIONS] <COMMAND>

Commands:
  home  Use an existing directory as home. Data lives below <home>/.honeycomb/dir
  show
  env   Print shell setup for this home and backend. Use eval "$(honeycomb config env)"
  set   Set api, auto_update or telemetry. Boolean values are true/false
  help  Print this message or the help of the given subcommand(s)

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb config home

```text
Use an existing directory as home. Data lives below <home>/.honeycomb/dir

Usage: honeycomb config home [OPTIONS] <LOCATION>

Arguments:
  <LOCATION>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb config show

```text
Usage: honeycomb config show [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb config env

```text
Print shell setup for this home and backend. Use eval "$(honeycomb config env)"

Usage: honeycomb config env [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb config set

```text
Set api, auto_update or telemetry. Boolean values are true/false

Usage: honeycomb config set [OPTIONS] <NAME> <VALUE>

Arguments:
  <NAME>   [possible values: api, auto_update, telemetry]
  <VALUE>

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb daemon

```text
Run the shared CLI/app update worker at second 01 of every minute

Usage: honeycomb daemon [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --once
          Run one scheduled check and exit, honoring auto_update
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb self-update

```text
Check for and install the latest verified Honeycomb CLI binary

Usage: honeycomb self-update [OPTIONS]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```

## honeycomb service

```text
Register the per-user CLI/app update worker

Usage: honeycomb service [OPTIONS] <ACTION>

Arguments:
  <ACTION>  [possible values: install]

Options:
      --api <API>
          Honeycomb backend origin. HTTPS required outside localhost [env: HONEYCOMB_API_URL=]
      --test <TEST>
          Use a saved testing environment ID or its 32-character root key
      --json
          Emit structured JSON. Secrets appear only in explicit credential-returning commands
      --idempotency-key <IDEMPOTENCY_KEY>
          Reuse this key to safely retry a mutation after an uncertain response
  -h, --help
          Print help
```
