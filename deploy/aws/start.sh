#!/bin/bash
# Runs on the dedicated host through SSM, with immutable image digests supplied by the operator.
set -euo pipefail
umask 077
: "${AWS_REGION:?}" "${RUNTIME_SECRET_ARN:?}" "${BACKEND_IMAGE:?}" "${WEB_IMAGE:?}" "${CADDY_IMAGE:?}"
install -d -m 700 /etc/silicon-honeycomb
aws secretsmanager get-secret-value --region "$AWS_REGION" --secret-id "$RUNTIME_SECRET_ARN" --query SecretString --output text > /etc/silicon-honeycomb/runtime.json
python3 - <<'PY'
import json, os
from pathlib import Path
root=Path('/etc/silicon-honeycomb')
settings=json.loads((root/'runtime.json').read_text())
for service in ['backend','library','console']:
    values=settings[service]
    assert all('\n' not in value and '\r' not in value for value in values.values())
    path=root/(service+'.env')
    path.write_text(''.join(f'{key}={value}\n' for key,value in values.items()))
    os.chmod(path,0o600)
(root/'runtime.json').unlink()
PY
registry="${BACKEND_IMAGE%%/*}"
aws ecr get-login-password --region "$AWS_REGION" | docker login --username AWS --password-stdin "$registry"
docker pull "$BACKEND_IMAGE"
docker pull "$WEB_IMAGE"
docker pull "$CADDY_IMAGE"
docker network inspect honeycomb >/dev/null 2>&1 || docker network create honeycomb
install -d -o 10001 -g 10001 /var/lib/silicon-honeycomb/backend
install -d -o 1000 -g 1000 /var/lib/silicon-honeycomb/library /var/lib/silicon-honeycomb/console
install -d /var/lib/silicon-honeycomb/caddy-data /var/lib/silicon-honeycomb/caddy-config
# Stop only this service's containers. Persistent data and encrypted session files remain.
for service in proxy backend library console; do
  if docker container inspect "honeycomb-$service" >/dev/null 2>&1; then docker rm -f "honeycomb-$service"; fi
done
docker run -d --name honeycomb-backend --network honeycomb --network-alias backend --restart unless-stopped --log-opt max-size=10m --log-opt max-file=3 --env-file /etc/silicon-honeycomb/backend.env -v /var/lib/silicon-honeycomb/backend:/data "$BACKEND_IMAGE"
for service in library console; do
  docker run -d --name "honeycomb-$service" --network honeycomb --network-alias "$service" --restart unless-stopped --log-opt max-size=10m --log-opt max-file=3 --env-file "/etc/silicon-honeycomb/$service.env" -v "/var/lib/silicon-honeycomb/$service:/data" "$WEB_IMAGE"
done
docker run -d --name honeycomb-proxy --network honeycomb --restart unless-stopped --log-opt max-size=10m --log-opt max-file=3 -p 80:80 -p 443:443 -v /etc/silicon-honeycomb/Caddyfile:/etc/caddy/Caddyfile:ro -v /var/lib/silicon-honeycomb/caddy-data:/data -v /var/lib/silicon-honeycomb/caddy-config:/config "$CADDY_IMAGE"
docker logout "$registry"
