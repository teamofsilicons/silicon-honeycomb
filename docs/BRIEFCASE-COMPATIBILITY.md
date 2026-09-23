# Briefcase compatibility review — 2026-09-16

Reviewed Briefcase source `abb3fb7` against Honeycomb `2cbb2e3` and deployed IAM
`ed9abe2`. Briefcase's live `/api/v1/version` advertises contract/build 1.1.0;
that response does not identify its source commit. This is a source/contract
review, not successful live delegated upload acceptance.

## Already aligned

Briefcase implements the exact reserve, capability transfer, commit, delegated
file-read and critical link-access routes used by Honeycomb's storage adapter.
Request bodies, reservation states, entry IDs and public-link response fields
match. App-folder initialization covers `apps/honeycomb/public`. It also
supports isolated requests selected by the IAM test application secret.

## Required for publishing and installing Briefcase

1. Add a release `honeycomb.yaml` for `briefcase`, mapping the `briefcase`
   command to real binaries for Linux, Windows and macOS on x86_64 and aarch64.
   The repository has no such manifest or six-platform release workflow; its
   current workflow only runs Linux Rust checks plus web/docs builds. Package
   all six payloads as one versioned archive and run Honeycomb validation.
2. Make the CLI updater recognize Honeycomb-managed installation. Its current
   updater derives a Cargo root from the running `bin` directory and uses
   `cargo install --force`. Managed installs must defer version selection and
   file replacement to Honeycomb; direct Cargo installs can retain their updater.
3. Adopt/link the existing `briefcase` IAM identity into Honeycomb, preserving
   its ID and secret. Do not attempt ordinary new-app creation over that identity.
   Honeycomb's legacy adoption flow remains a separate integration gap.

## Required IAM/storage configuration

- Honeycomb needs consented/effective `self.identity.read`,
  `self.membership.read`, **`self.tags.read`**, and the four external scopes:
  `briefcase.uploads.reserve`, `briefcase.uploads.commit`, `briefcase.files.read`,
  `briefcase.link_access.update` on `briefcase`.
- Briefcase must also have approved identity, membership and tag disclosure,
  with the corresponding OBO endpoints registered. Link access is critical and
  requires provider approval for public Honeycomb plus user consent.
- The tags requirement follows Briefcase
  `src/infrastructure/iam/official.rs::authorization` and IAM migration 0093;
  missing tags cannot safely be converted into an empty tag list.
- Honeycomb now passes the resource's owning organization for delegated reads
  and link publication as well as upload. The archive storage interface requires
  that organization, including reads of older entry-ID-only references. HTTP
  contract tests verify explicit selection for a caller owned by a different
  organization, exact signed request bytes, and production/test isolation. Live
  multi-organization acceptance still depends on the missing scopes and consent.

## Required for full shared testing compatibility

Briefcase still offers its own environment creation/lifecycle operations backed
by IAM. Honeycomb now owns the shared environment lifecycle. Add a protected
participant contract for Honeycomb-driven prepare/activate/clean/disable/
restore/purge, with stable operation identity, environment generation and durable
completion/cleanup receipts. Keep Briefcase responsible for isolated file data,
storage limits and provider cleanup. Its existing app-secret data-plane routing
is useful and should be preserved. Both the Briefcase participant endpoint and
Honeycomb transport need implementation; no end-to-end shared readiness is proven.

## Documentation corrections

Briefcase's OBO documentation still requires same-owner-organization delegation
and `obo.issue`. IAM migration 0078 replaced those restrictions with selected
membership and effective external endpoint scopes. Correct these descriptions
and align the testing guide with Honeycomb lifecycle ownership. Do not grant an
obsolete permission merely to satisfy stale documentation.
