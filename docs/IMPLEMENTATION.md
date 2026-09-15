# Implementation tracker

Source of requirements: human-owned `../UNDERSTANDING.md` (never edited by agents).

## Delivery order

1. Backend: live IAM authentication, organization authorization, safe packages,
   drafts and releases, durable configuration operations, publication reviews,
   private catalog filtering, metrics, testing lifecycle coordination.
2. Stateless Rust client and stateful CLI: SLT login, packaging, app administration,
   package installation/update/removal, testing selection, updater and shell installer.
3. Library and authenticated console on the requested domains.
4. Backend, CLI, installer and browser end-to-end verification.

## Integration findings

- Latest published `silicon-iam-client` verified with `cargo info`: 1.9.0.
- IAM supports application SLT exchange and live authorization snapshots.
- Current IAM source has no Honeycomb management service integration. Application
  administration there requires a direct IAM principal credential, which must not
  be substituted for Honeycomb's application token. The new integration requires
  service identity AND live acting-user authority, revisions and idempotency.
- Briefcase OBO supports recoverable uploads, reads, and critical
  `briefcase.link_access.update` for anyone-with-link sharing.
- Ecosystem testing lifecycle migration requires coordination with IAM and each
  application's protected lifecycle implementation. Never report all services
  ready solely because Honeycomb's local record exists.

Track external limitations explicitly. Test doubles are restricted to test code;
production has no fabricated-login or automatic-approval switch.
