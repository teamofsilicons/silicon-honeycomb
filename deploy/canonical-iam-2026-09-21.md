# Canonical IAM compatibility deployment, 2026-09-21

Source: `16cd14b79c3c50b1a07de495f5dd094b5ff32233`.

This release combines the deployed server baseline, canonical IAM actor handling,
and the existing 0.2.4 CLI release source. It preserves publication behavior and
the current console/library services.

The backend on `i-06986627793021fb2` in `us-east-2` runs:

```text
234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:a7bc8272b2346bf690c566b08bd1d4eacc5cb774d4823c1a5659dc8fb1567b98
```

The SQLite conversion was rehearsed on an online copy, then repeated after
stopping the backend and saving a quiesced backup. It converted 185 actor
references using exact testing-environment mappings. Row counts, foreign keys,
and integrity checks passed; a second import changed zero references.

SSM deployment `538f9fc3-8fb9-4afb-8070-6b272ab8bdc2` preserved the current
container environment, network, mounts, and supporting containers. The backup is
`/var/lib/silicon-honeycomb/backups/iam3-1789982863822686763`; its original
database SHA-256 is
`b3637a01d7fe6d817a589160dc0eca9dbbffa3c2d2fcfc7a731f6e2adc164e6d`.
The previous backend container is stopped with its restart policy disabled.

Validation included 124 server tests, three real SQLite tests, strict Clippy,
69 server API tests after merging the current release branch, and formatting.
The existing authenticated `tos>iam` application read passed. Publishing the
IAM CLI 3.0.1 through this backend also completed with checksum validation and a
fresh anonymous install, exercising retained application ownership.

This records consumer readiness before the IAM identity switch. An old-image-only
rollback after canonical data conversion would be unsafe.
