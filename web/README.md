# Honeycomb library and console

One Solid frontend serves two hosts with separate server-side sessions:

- `honeycomb.teamofsilicons.com`: public catalog, authenticated private discovery,
  details, reviews and installation instructions.
- `console.honeycomb.teamofsilicons.com`: IAM login before organization app creation,
  shared drafts, releases, publication discussions and testing environments.

The visual reference is the current `silicon-interface-frontend`: paper surfaces,
`#1736B8`, Source Serif 4 headings, IBM Plex Sans and Mono, thin rules and spacious
layouts. Identity, authorization and product behavior belong to Honeycomb.

## Local development

Requires Node 24+ and Rust from the workspace. Run `npm ci` here. For the production
IAM adapter, start the Rust backend using the root `.env.example` configuration,
then copy this directory's `.env.example` to `.env` and configure origins. Start
one process per website with `HONEYCOMB_SITE=library` or `console` and distinct
ports/session databases. Run `npm run dev` in each process.

`npm run build` type-checks and creates `dist` and `dist-server`. `npm start` serves
the production build. Use separate 32-byte hexadecimal `WEB_SESSION_KEY` values
and persistent SQLite session files for each host. Terminate HTTPS at the reverse
proxy and preserve the configured host. Callback URLs are `/auth/callback` on each
host; register both with IAM. Access and refresh tokens stay encrypted server-side.

## Repeatable browser tests

```sh
npm ci
npx playwright install chromium
npm run test:e2e
```

The runner launches an isolated Rust backend fixture and both website processes
on ports 19180, 19173 and 19174, then stops them. It never uses production accounts.
IAM identity/management and Briefcase storage are explicit test doubles in the
Rust example, while HTTP, authorization filtering, database operations, frontend,
session cookies, drafts and installation instructions use the real implementation.
The fixture is not included in the production server.

Desktop/mobile journeys cover public and private search, hosted login redirect,
app creation, publication gating, draft recovery, sign-out, CSRF/state rejection
and responsive layout. Traces and screenshots are retained on failure.

Real IAM activation and shared lifecycle completion require the integration in
`../docs/IAM-HANDOFF.md`. The console displays pending operations until accepted.
