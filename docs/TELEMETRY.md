# Telemetry

## Configure the backend

The dedicated Space Station table is
[tos / siliconhoneycomb](https://spacestation.teamofsilicons.com/o/tos/tables/siliconhoneycomb).
Provide `HONEYCOMB_SPACE_STATION_TABLE_KEY` through the backend's secret store.
Use a persistent, private directory for `HONEYCOMB_TELEMETRY_HOME` (the backend
container defaults to `/data/telemetry`). Never put the key in browser bundles,
CLI configuration, Git, or release artifacts.

The local provisioning key is stored outside this repository at
`~/.config/silicon-honeycomb/telemetry.env`, with directory mode 0700 and file mode
0600. Deployment still needs the key provisioned into its own secret store.

Diagnostics are enabled by default. Without a configured key the exporter stays
inactive. `HONEYCOMB_TELEMETRY=false` disables the backend recorder entirely.
The destination defaults to `https://backend.spacestation.teamofsilicons.com`;
`HONEYCOMB_TELEMETRY_URL` supports HTTPS origins or literal loopback HTTP for tests.

To send one explicit synthetic operator check after injecting the key into the
environment, run on the backend's Linux/macOS host:

```sh
cargo run --locked -p silicon-honeycomb-server --example telemetry_check
```

This contacts the real Space Station destination, waits up to 15 seconds for its
acknowledgment, and prints a check ID. Reopen the table to see the new snapshot.
This is separate from the test suite, which never sends records to the live table.

## Change your preference

- CLI: `honeycomb config set telemetry false`. `HONEYCOMB_TELEMETRY=false` also
  overrides the saved preference, including scheduled daemon checks.
- Library and console: open **Telemetry settings** in the top bar and turn off
  **Share usage and diagnostics**. Preferences apply to each website in that browser.
- Rust client: call `client.with_telemetry(false)` and use the returned client.

An opt-out accompanies normal API requests. The web server also carries the cookie
preference through IAM login, refresh and sign-out. The IAM runtime SDK's independent
telemetry is disabled so it cannot bypass Honeycomb's preference.

## Event contract and delivery

Backend HTTP events contain route templates, HTTP method/status, elapsed time and
a generated request ID. CLI/daemon events contain fixed command families, elapsed
time and success. Authenticated website analytics contain page families and API
timings. Events include service, version, source, step, progress and timestamp.
Space Station adds its SDK metadata on the backend host.

The authenticated `/api/v1/telemetry` gateway accepts only named fields and fixed
source/event/action values, with a 4 KiB body limit. It rejects arbitrary details.
No application names, IDs, search strings, request/response bodies, local paths,
credentials or free-form error messages are included. No ingest key is distributed
to clients. Anonymous browser activity does not emit client events; backend request
metrics still follow its telemetry header.

The backend uses official `space-station` 0.1.1 with its bounded queue, durable
spool and WebSocket acknowledgment protocol. Export is best effort; request handling
does not wait for ingestion. CLI diagnostics have a 300 ms request limit and website
diagnostics have a one-second limit and at most eight pending requests. Backend
export currently requires Unix; Rust clients and the CLI remain cross-platform.

Testing-context diagnostics never use the production table key. When the recorder
is enabled they stay in local SQLite, capped at 1,000 events per environment. Writes
require the captured generation to remain ready; clean/purge removes those events.
Backend HTTP metrics skip testing-key requests, avoiding accidental production export.

## Verification

On 2026-09-16, the official SDK acknowledged synthetic check
`cc1d23fe-4af3-4ee2-af4a-1a78d60a7f51`, and the record was verified in the table UI.
Automated coverage checks schema rejection, opt-out propagation, test isolation,
generation fencing, bounded retention, cleanup, client wire behavior and persistent
website preferences on desktop/mobile. Live deployment remains separate work.
