#!/usr/bin/env python3
"""Deploy immutable images to the dedicated Honeycomb host through SSM."""
import argparse
import base64
import json
from pathlib import Path
import shlex
import subprocess

from testing_credentials import ensure_testing_credentials

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--profile", default="silicon-production")
parser.add_argument("--region", default="us-east-2")
parser.add_argument("--stack", default="silicon-honeycomb-production")
parser.add_argument("--image-tag", required=True)
parser.add_argument("--briefcase-region", default="us-east-1")
parser.add_argument("--briefcase-secret", default="silicon-briefcase/production")
args = parser.parse_args()


def aws(*command):
    return json.loads(subprocess.check_output(
        ["aws", "--profile", args.profile, "--region", args.region, *command, "--output", "json"], text=True
    ))


stack = aws("cloudformation", "describe-stacks", "--stack-name", args.stack)["Stacks"][0]
if stack["StackStatus"] not in {"CREATE_COMPLETE", "UPDATE_COMPLETE"}:
    raise SystemExit("The infrastructure stack must be complete before deploying.")
outputs = {item["OutputKey"]: item["OutputValue"] for item in stack["Outputs"]}


def immutable_image(key):
    uri = outputs[key]
    detail = aws("ecr", "describe-images", "--repository-name", uri.split("/", 1)[1],
                 "--image-ids", f"imageTag={args.image_tag}")["imageDetails"][0]
    return uri + "@" + detail["imageDigest"]


root = Path(__file__).resolve().parent
commands = ["set -eu", "install -d -m 700 /etc/silicon-honeycomb"]
for source, target in [(root / "start.sh", "start.sh"), (root.parent / "Caddyfile", "Caddyfile")]:
    encoded = base64.b64encode(source.read_bytes()).decode()
    commands.append(f"printf %s {shlex.quote(encoded)} | base64 -d > /etc/silicon-honeycomb/{target}")
values = {
    "AWS_REGION": args.region,
    "RUNTIME_SECRET_ARN": outputs["RuntimeSecretArn"],
    "BACKEND_IMAGE": immutable_image("BackendRepository"),
    "WEB_IMAGE": immutable_image("WebRepository"),
    "CADDY_IMAGE": "caddy@sha256:13ba145cba2f3e28fa801994876e4c086d1b95d5aa2a520a734765ffb6b12017",
}
commands.append(" ".join(f"{key}={shlex.quote(value)}" for key, value in values.items())
                + " bash /etc/silicon-honeycomb/start.sh")
testing_credentials = ensure_testing_credentials(
    args.profile, args.region, outputs["RuntimeSecretArn"],
    args.briefcase_region, args.briefcase_secret,
)
result = aws("ssm", "send-command", "--instance-ids", outputs["InstanceId"],
             "--document-name", "AWS-RunShellScript", "--comment", f"Honeycomb deployment {args.image_tag}",
             "--parameters", json.dumps({"commands": commands, "executionTimeout": ["900"]}))
print(json.dumps({"command_id": result["Command"]["CommandId"], "instance_id": outputs["InstanceId"],
                  "public_ip": outputs["PublicIp"], "images": values,
                  "testing_credentials": testing_credentials}, indent=2))
