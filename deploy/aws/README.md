# Honeycomb AWS host

Use AWS profile `silicon-production`, account `234951665042`, region `us-east-2`.
The initial `us-east-1` attempt hit its standard EC2 vCPU quota. Its unused address
and repositories were removed; its duplicate runtime secret is scheduled for
recoverable deletion. IAM continues in its existing region and infrastructure.

`production.json` creates a dedicated ARM64 `t4g.small` host, encrypted 30 GiB gp3
volume, static public IP, two immutable ECR repositories, a Secrets Manager secret,
and an SSM instance role. Public ingress is limited to TCP 80/443. SSH is closed;
IMDSv2 is required. The instance and disk are retained to protect persisted data.
This is one stateful host; it has no automatic failover.

```sh
aws cloudformation deploy --profile silicon-production --region us-east-2 \
  --stack-name silicon-honeycomb-production --template-file deploy/aws/production.json \
  --parameter-overrides VpcId=vpc-01d16f1948c3c34aa SubnetId=subnet-0647142f6221fade8 \
  --capabilities CAPABILITY_IAM --tags Service=silicon-honeycomb
```

Build Linux ARM64 images from `deploy/Dockerfile.backend` and `deploy/Dockerfile.web`,
push them to the stack's ECR repositories under a new immutable tag, and populate
`silicon-honeycomb/production/runtime` with protected `backend`, `library`, and
`console` environment maps. Preserve existing encryption/session keys during
updates. The operator handoff lives outside Git at
`/Users/codanium/.config/silicon/honeycomb/production-runtime.json` (0600).

```sh
python3 deploy/aws/deploy.py --image-tag <published-image-tag>
```

The tool resolves immutable image digests, automatically provisions the shared
Briefcase testing credential, and dispatches `start.sh` through SSM. Testing does
not require an app owner to generate or copy a service token. The deployment tool
reuses an existing credential from either secret store, creates one only when both
are absent, and refuses conflicting existing credentials. It preserves unrelated
runtime settings and never prints the credential. The deploying AWS identity needs
read/write access to both service secrets; service runtime roles remain separate.
Use `--briefcase-region` and `--briefcase-secret` for a different managed deployment.
Provisioning secrets does not reload a running Briefcase service: its deployment
must load the shared secret too. Ordinary environment creation never rotates or
re-provisions this service credential.

This automatic setup covers service authentication. Complete shared testing still
requires the IAM lifecycle contract changes tracked in
[`docs/IAM-CONTRACT-REVIEW.md`](../../docs/IAM-CONTRACT-REVIEW.md); the current
unavailable IAM adapter must not report environments ready prematurely.
Inspect the returned command ID with `aws ssm get-command-invocation` until it
succeeds. It reads secrets on the host, recreates only Honeycomb containers, and
preserves `/var/lib/silicon-honeycomb` data. The proxy uses a pinned Caddy image and
obtains HTTPS certificates. The static SolidJS frontend assets are hosted on
Vercel; API/session traffic is proxied to this host.

Before an upgrade, take a consistent backup of SQLite databases (SQLite backup
API or stop writers before copying) and retain the encryption keys separately in
Secrets Manager. An ordinary live filesystem copy of a WAL database is not a
reliable backup. Restore a matching database/key set together. Automatic backup
and failover policies have not been configured.

Namecheap DNS:

- `backend.honeycomb` A → the stack's PublicIp output.
- `honeycomb` A → `76.76.21.21` (Vercel's verified project recommendation).
- `console.honeycomb` A → `76.76.21.21`.

Verify `/health`, `/api/contracts`, public catalog access and both frontend
`/api/config` and `/api/session` routes after deploying. IAM grants, consent,
notifications and deferred Briefcase acceptance remain separate checks.

## Adopting an existing legacy IAM application

An application created before Honeycomb can have IAM configuration revision zero.
Deploy revision-zero reconciliation support before importing it. The privileged
`scripts/adopt_legacy_app.py` operator tool reads IAM's current accepted record,
checks its immutable ID and organization, and imports an active **private** catalog
record. It does not mutate IAM, obtain its app/webhook secrets, or publish a release.
It refuses to overwrite an existing Honeycomb app. This is an operator migration,
not a public endpoint or a replacement for actor-authorized app registration.

Run on the backend host, using the existing protected environment file and local
SQLite database; first omit `--apply` to review its safe projection:

```sh
python3 adopt_legacy_app.py --app 'tos>briefcase' \
  --expected-identity 01a070db-89b4-7542-83f1-4fad5cbce625 \
  --metadata briefcase-catalog.json \
  --database /var/lib/silicon-honeycomb/backend/honeycomb.db \
  --env-file /etc/silicon-honeycomb/backend.env \
  --actor 'operator:user-requested-briefcase-adoption' \
  --backup-dir /var/lib/silicon-honeycomb/backups/legacy-adoption --apply
```

The tool verifies a consistent SQLite backup before acquiring a write transaction,
re-reads IAM to reject concurrent changes, and records an accepted `iam.adopt`
operation and audit entry. Worker reconciliation continues from IAM revision zero;
new Honeycomb configuration updates start at revision one. Existing app and webhook
secrets are preserved in IAM; a later configuration edit must supply the webhook
URL and signing secret. The existing app is private until normal release and
publication requirements are fulfilled. Verify its live CLI result and accepted
reconciliation after the import; keep the receipt and backup path in the operator log.
