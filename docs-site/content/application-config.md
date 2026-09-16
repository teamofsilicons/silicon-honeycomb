## A complete creation input
Save the following as a protected JSON file. Replace the example organization, receiver, and signing secret before submitting. The description intentionally meets the minimum word count.
```json
{
  "org_id": "my-org",
  "local_app_id": "my-app",
  "name": "My application",
  "description": "My application helps members of our organization prepare, inspect, and share project work from a consistent command line interface. It provides native builds for supported desktop and server platforms, uses IAM for identity and permissions, and keeps organization data separated. This application is maintained by our team and is intended for authorized members working on shared projects.",
  "webhook_url": "https://api.example.com/webhook/",
  "webhook_secret": "REPLACE_WITH_A_PRIVATE_RANDOM_SECRET",
  "webhook_scope": ["membership"],
  "app_scope": {"iam": ["self.identity.read"], "external": []},
  "obo_endpoints": [],
  "testing_idle_days": 30
}
```
Generate a random signing secret with a secure generator such as `openssl rand -hex 32`, store it privately, and configure the same value at your receiver. [Download the example](/examples/application.json).

## Required fields
| Field | Rules |
| --- | --- |
| `org_id` | Existing shared organization; caller must be a current owner/admin. |
| `local_app_id` | Permanent local handle. Handles use 1–64 lowercase letters, digits, or hyphens; begin with a letter or digit. |
| `name` | Nonempty display name, at most 100 bytes under the current validator. |
| `description` | 50–1,000 whitespace-separated words. |
| `webhook_url` | HTTPS destination without embedded credentials. |
| `webhook_secret` | At least 32 characters for creation; protect it like a credential. |

The resulting ID is `org_id>local_app_id`. Updating configuration does not rename the application's identity.

## Optional fields
| Field | Default / purpose |
| --- | --- |
| `logo_url` | Optional HTTPS logo. Uploaded logo links are public. |
| `website_url`, `docs_url` | Optional HTTPS links. |
| `base_url` | Backend origin for exposed OBO endpoints; no path, query, or trailing slash. |
| `webhook_scope` | Categories from `membership`, `updates`, `trust`, `full`. |
| `app_scope.iam` | Requested IAM scope names. |
| `app_scope.external` | Provider application and endpoint pairs. |
| `obo_endpoints` | Endpoints your application exposes for delegated access. |
| `obo_review_message` | Context for permission reviewers. |
| `testing_idle_days` | 30 by default; range 1–36500. |

Unknown input fields are rejected. Requested and effective configuration are displayed separately because IAM must accept changes before they apply at runtime.

## Update safely
```sh
honeycomb apps get 'my-org>my-app' --json
honeycomb --idempotency-key my-app-config-0002 apps update 'my-org>my-app' application.json --revision 2
```
Use the actual current revision and a complete configuration document. On updates, an empty or omitted signing secret retains the existing stored secret when available. Existing legacy apps without stored configuration need a supported adoption workflow; do not recreate their identity to work around this.

## Shared drafts
```sh
honeycomb drafts list my-org
honeycomb drafts save my-org draft-handle draft.json --revision 0
```
A new draft starts at revision 0; later saves use the latest draft revision. Drafts let another current organization admin continue the work. They are not active IAM applications, and secrets/files are not persisted as draft fields.
