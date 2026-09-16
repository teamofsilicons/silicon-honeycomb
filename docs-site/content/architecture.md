## Request flow
```text
Library / Console ── website session service ──┐
                                               ├── Honeycomb backend
CLI / Rust client ────────────────────────────┘        │
                                            ┌─────────┴────────┐
                                            IAM            Briefcase
                                       identity + policy   archive bytes
```
The SolidJS websites render the library and console. Their session service handles IAM redirects, refresh, and secure session persistence. The Rust CLI and stateless client call the Honeycomb HTTP API directly.

## Service boundaries
| Component | Owns |
| --- | --- |
| Honeycomb | Application workflows, catalog, immutable releases, review orchestration, drafts, durable operations, shared testing lifecycle, and the product's bundle definitions. |
| IAM | Runtime authentication and authorization, consent, identities, app secrets, effective scopes/configuration, OBO issuance/validation, and accepted webhook state. |
| Briefcase | Authorized archive/logo byte storage and link-access operations. |
| Participating services | Their own isolated test data and lifecycle receipts. |

IAM runtime uses its accepted local state. It must not call Honeycomb for every login or permission check, and IAM bootstrap must not recursively require Honeycomb.

## Durable control-plane work
Mutations combine current acting-user authority, explicit idempotency keys, revision preconditions, and a durable operation record. A remote service can accept work before a response is lost, so retries must recover the same result. Signed IAM notifications and explicit reconciliation update accepted state without pretending requested settings are already active.

## Storage and credentials
SQLite holds application and workflow metadata on persistent volumes. Package bytes go to Briefcase. Backend encryption keys protect stored signing material; website session keys protect persisted sessions. Back up databases consistently and preserve their associated encryption keys separately.

## Production layout
Vercel serves library/console assets and the documentation site. AWS hosts the Rust backend and the two persistent website session services behind Caddy. Namecheap hosts DNS. Each website has its own session configuration and callback URL. This deployment uses a single backend host and persistent local SQLite; it is not a horizontally scaled or highly available design.

## Design versus completion
Ownership statements describe architecture. They do not claim every bundle/adoption/lifecycle API is implemented.
