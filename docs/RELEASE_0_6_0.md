# Honeycomb 0.6.0

Honeycomb 0.6.0 introduces dedicated application and release pages, clearer request discussions and publication status, application logo uploads, and personal App to App verifications. The console and library share the Arc-based interface and responsive navigation. The application-building guide is a separate article linked from the documentation.

## Compatibility

Deploy with IAM 5.0.0 and the coordinated reusable OBO token integrations. Application login no longer authorizes delegated storage. Logo and release storage operations request separate Briefcase permission and retain encrypted access/refresh credentials. ATA verification records are centrally listed for the current signer; endpoint definitions remain within applications. Existing authorization checks and per-release private-until-approved visibility remain in force.

Both persistent session gateways must accompany the static console/library deployment. Migrations 28 and 29 add request activity/read markers and encrypted storage-authorization state. Preserve backend encryption keys and gateway session keys and take consistent database backups before upgrading.

## Release validation

The working implementation passed 235 Rust tests, strict Clippy, package verification, CLI/installer integrations, frontend checks and desktop/mobile inspection. Final commit CI, all six native packages, production backup/migration checks, authenticated live acceptance and publication receipts must be recorded by the release operator; this note does not claim deployment completion.
