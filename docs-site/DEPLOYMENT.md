# Documentation deployment — 2026-09-16

- Source: `20f4a1e` (public main), including generated references and typed SDK examples.
- Project: `silicon-honeycomb-docs`, Vercel team `saketdev12-5675s-projects`.
- Production: https://docs.honeycomb.teamofsilicons.com
- Deployment: https://silicon-honeycomb-docs-kk5pm5pvn-saketdev12-5675s-projects.vercel.app
- Public default alias: https://silicon-honeycomb-docs.vercel.app
- Namecheap: added only `A docs.honeycomb → 76.76.21.21`, automatic TTL (1800 seconds), through the authenticated browser because the CLI's current public IP was rejected.
- Both authoritative nameservers, Google DNS, and Quad9 return the new address.
  Cloudflare and the local resolver still hold negative responses; their previous
  NXDOMAIN cache can take up to about an hour to expire. No nameservers, existing
  records, or local resolver settings were changed.
- All 26 pages pass production HTTPS requests with certificate verification, using
  the confirmed destination address to isolate the local DNS cache. The public
  Vercel alias passes normal browser navigation and live search-to-guide navigation.
- 12 desktop/mobile documentation browser tests pass, including all article routes,
  overflow, search, error recovery, copy buttons, downloads, mobile menu, and static
  readability without JavaScript. Static checks validate 1489 links/assets.
- The downloadable six-target manifest passes real CLI validation, packaging, and
  archive validation with disposable structural fixture payloads. This does not
  substitute for executing native binaries on each platform.
- Rust SDK example passes cargo check and strict Clippy. Generated reference checks
  cover 66 command help screens and 55 registered API methods.
- GitHub Actions run `35044993399`: documentation job passed; wider existing Rust
  and website jobs were still running at this deployment verification.
- Library deployment: https://silicon-honeycomb-library-6mptwkqvx-saketdev12-5675s-projects.vercel.app
- Console deployment: https://silicon-honeycomb-console-6e913sgu8-saketdev12-5675s-projects.vercel.app
- Both products now link to Documentation. Four focused existing desktop/mobile
  website navigation/layout checks passed. No backend runtime change was needed.

## IAM acceptance update

The browser popup no longer blocks automation. IAM reports `self.tags.read`
effective at revision 4. Fresh user consent was completed for the already shared
`tos` organization. The released 0.1.0 CLI exchanged a fresh one-use token against
production and a separate `login status` request confirmed an authenticated Carbon
with the current `tos` owner role. The test used isolated private local storage;
no credential is recorded here.

The live management check still reports missing Briefcase external grants for
reserve, commit, read, and link-access update. Private registration/storage,
publication, adoption, and the remaining shared-testing service contracts still
need production acceptance. This documentation rollout does not mark those done.
