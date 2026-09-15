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

The tool resolves immutable image digests and dispatches `start.sh` through SSM.
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
