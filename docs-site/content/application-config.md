Register and configure production applications through Honeycomb. Honeycomb creates and maintains the internal IAM application identity needed for authentication. Direct production app registration in IAM is retired; an existing IAM record alone is not a Honeycomb listing or approval request.

## A complete creation input
Save the following as a protected JSON file. Replace the example organization, receiver, and signing secret before submitting. The description intentionally meets the minimum word count.
```json
{
  "org_id": "my-org",
  "app_id": "my-app",
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
| `org_id` | Existing organization selected for this session; caller must be a current owner/admin. Organization IDs use 3–50 lowercase letters, digits, underscores, or hyphens. |
| `app_id` | Permanent, globally unique application ID. Use 1–80 lowercase letters, digits, underscores, or hyphens, beginning with a letter. |
| `name` | Nonempty display name, at most 100 bytes under the current validator. |
| `description` | 50–1,000 whitespace-separated words. |
| `webhook_url` | HTTPS destination without embedded credentials. |
| `webhook_secret` | At least 32 characters for creation; protect it like a credential. |

The resulting ID is the bare `app_id`; `org_id` separately identifies its owner. Updating configuration does not rename the application's identity.

## Optional fields
| Field | Default / purpose |
| --- | --- |
| `visibility` | New production apps default to `public`: automatically request approval once configuration and the first valid release are ready. Set `private` to opt out. On updates, omission preserves the existing choice; older apps retain their existing visibility preference. Effective public access still requires every approval. |
| `logo_url` | Optional HTTPS logo. Uploaded logo links are public. |
| `website_url`, `docs_url` | Optional HTTPS links. |
| `base_url` | Backend origin for exposed OBO or ATA endpoints; no path, query, or trailing slash. |
| `webhook_scope` | Categories from `membership`, `updates`, `trust`, `full`. |
| `app_scope.iam` | Requested IAM scope names. |
| `app_scope.external` | Provider application and endpoint pairs. |
| `obo_endpoints` | Endpoints your application exposes for delegated access. |
| `obo_endpoints[].downstream` | Optional direct dependencies as `{ "audience": "briefcase", "endpoint_id": "briefcase.files.read" }` pairs. At most 16 unique calls per endpoint; the audience must be another application. IAM validates the complete dependency graph and requires separate user OBO consent. |
| `ata_endpoints` | Endpoints exposed for application-only delegation. ATA dependencies remain ATA and do not inherit user OBO grants. |
| `obo_review_message` | Context for permission reviewers. |
| `testing_idle_days` | 30 by default; range 1–36500. |

The console, HTTP API, and current CLI creation/update input support `visibility`. Use `honeycomb apps create --help` and the current configuration reference when upgrading older CLI installations.

Unknown input fields are rejected. Requested and effective configuration are displayed separately because IAM must accept changes before they apply at runtime.

## Update safely
```sh
honeycomb apps get 'my-app' --json
honeycomb --idempotency-key my-app-config-0002 apps update 'my-app' application.json --revision 2
```
Use the actual current revision and a complete configuration document. On updates, an empty or omitted signing secret retains the existing stored secret when available. Existing legacy apps without stored configuration need a supported adoption workflow; do not recreate their identity to work around this.

## Shared drafts
```sh
honeycomb drafts list my-org
honeycomb drafts save my-org draft-handle draft.json --revision 0
```
A new draft starts at revision 0; later saves use the latest draft revision. Drafts let another current organization admin continue the work. They are not active IAM applications, and secrets/files are not persisted as draft fields.
