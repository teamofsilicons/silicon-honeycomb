# Configured service identities rollout — 2026-09-16

Backend source: `7448a7dac252a83521174dc9ab77e7ab3998cf1c`, built from its Git archive.
Image: `234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:8fe683df8d3c70a75a01715cf5b021e428588f5cc6b61710c861ab97e9c444c6`.

The coordinator now uses configured identity and storage application IDs. External
lifecycle participants use explicit identity, HTTPS origin and secret references.
The deployment helper derives the existing Briefcase participant from its runtime
configuration and preserves shared service authentication. No user setup token,
new product workflow or automatic trust based on catalog metadata was added.
This image also includes the publication discussion audience/retry fixes.

## Validation

- Full locked workspace test suite passed, including 48 server API tests and
  configured-identity lifecycle ordering/cleanup regressions.
- Strict server Clippy and formatting passed; all ten deployment credential tests passed.
- SSM command `434e03c3-5e26-4ab1-b57b-6d3daf125d3e` succeeded on
  `i-06986627793021fb2` in `us-east-2` using `silicon-production`.
- Consistent database/config backup:
  `/var/lib/silicon-honeycomb/backups/configured-identities-20260916T041108Z`.
- Previous image retained for rollback:
  `sha256:2234baa2e5e367edfed3947b8b7a0f3bb2aa87b8544c56df48a7565b034e80ff`.
- Library, console and proxy container IDs were preserved. Existing encryption,
  application and IAM management credentials were checked unchanged.
- Public health, API contracts, Briefcase catalog and both frontend session
  endpoints returned HTTP 200 after deployment. Session probes were anonymous;
  these checks do not establish user consent or authenticated storage access.

IAM contract completion, actual Briefcase archive upload/publication and fresh
CLI release delivery remain separate work. Local CLI managed-update fixes are
committed but were not delivered by this backend-only rollout.
