> Integration status: these interfaces exist and have local contract tests. Full ecosystem production acceptance remains incomplete. Read [availability](/availability/) before depending on cross-service lifecycle behavior.

## What an environment isolates
Honeycomb coordinates one shared testing environment across participating services. IAM handles the isolated identity/token/OBO work; each data service owns its test records. Testing credentials must never reach production through a fallback.

```sh
honeycomb environments create my-org 'Integration testing' --description 'Shared testing for our application.'
honeycomb environments list
honeycomb environments get ENVIRONMENT_ID
honeycomb environments key ENVIRONMENT_ID
```
Key retrieval is authorized and audited. It saves the root key in the CLI's selected home and can print credential material; keep the output private. The key is exactly 32 alphanumeric characters.

## Select the context
```sh
honeycomb --test ENVIRONMENT_ID search
```
The CLI accepts a saved ID or the raw key. Prefer saved IDs so the root key is not placed in shell history. An environment that is disabled, unready, invalid, or unauthorized must fail instead of using production.

## Import an existing application
```sh
honeycomb environments import ENVIRONMENT_ID 'my-org>my-app' --revision 1 --release 1.0.0
honeycomb environments get ENVIRONMENT_ID
```
Imports recursively include external-scope dependencies, preserve owning organizations, and pin accepted configurations and selected releases. Production access is required for private dependencies. The selected release can be omitted to use the workflow's current selection rules.

Use `--refresh` explicitly to update existing dependency pins. Supply the environment's current revision; importing should not silently rewrite unrelated pinned work.

## Coordinate lifecycle changes
```sh
honeycomb environments action ENVIRONMENT_ID rotate-key --revision 2
honeycomb environments action ENVIRONMENT_ID clean --revision 3
honeycomb environments action ENVIRONMENT_ID delete --revision 4
honeycomb environments action ENVIRONMENT_ID restore --revision 5
honeycomb environments action ENVIRONMENT_ID retry --revision 6
```
These revision numbers are illustrative; read current state before each action. Rotation replaces testing authority, clean removes test data, delete starts recoverable deletion, restore reverses recoverable deletion, and retry resumes an incomplete operation. A generation change invalidates stale work. Completion requires participant receipts, not merely dispatching requests.

A `purge` action also exists and is irreversible. Use it only for an explicitly intended permanent removal under the environment's policy. Recoverable deletion has a 30-day recovery window after every participant confirms disabled access.

## App-owned testing and bundles
The product design assigns application/test lifecycle and bundle definitions to Honeycomb. Current app-owned testing credentials, legacy environment adoption, bundle UI/API, and some protected service coordination are not complete. Do not invent bundle commands or use production credentials to bypass missing test contracts.
