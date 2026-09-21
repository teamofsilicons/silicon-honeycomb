# Honeycomb library and console

One Solid frontend serves two hosts with separate server-side sessions:

- `honeycomb.teamofsilicons.com`: public catalog, authenticated private discovery,
  details, reviews and installation instructions.
- `console.honeycomb.teamofsilicons.com`: IAM login before organization app creation,
  shared drafts, releases, publication discussions and testing environments.

The visual reference is the current `silicon-interface-frontend`: paper surfaces,
`#1736B8`, Source Serif 4 headings, IBM Plex Sans and Mono, thin rules and spacious
layouts. Identity, authorization and product behavior belong to Honeycomb.

The console requires a release channel for both the first archive and subsequent
uploads. Release history defaults to production and can show development
independently. Development releases can be promoted by entering a production
version; uncertain uploads and promotions retain their idempotency key when retried.
The library defaults to official production install commands and offers expandable
experimental installation instructions.
Release requests and downloads use HTTP v2 with matching negotiation headers;
authentication, application configuration, and publication continue to use v1.
The website session proxy forwards both versions without moving tokens to the browser.

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

## Vercel production assets

Set `HONEYCOMB_SITE=library` or `console`, then run `npm run build:vercel`.
The build creates static Build Output API artifacts in `.vercel/output`, ready
for `vercel deploy --prebuilt --prod` in the matching project. The two Vercel
projects are `silicon-honeycomb-library` and `silicon-honeycomb-console`.

Vercel proxies `/api/*` and `/auth/*` to `/web/<site>/*` on the backend hostname.
The persistent Node session services run beside the Rust backend on AWS, retaining
encrypted SQLite sessions and distinct keys for each website. No credentials,
session databases or server functions are uploaded to Vercel. `deploy/Caddyfile`
routes these two prefixes to the correct service. IAM callbacks remain on each
frontend's `/auth/callback`; cookies are host-only, HttpOnly, Secure and SameSite=Lax.

## Repeatable browser tests

Review requests is the incoming queue for eligible reviewers. Sent requests shows
publication history for all applications the signed-in user manages, including
completed and denied requests. Submitting a publication request authorizes
publication of that revision after every required approval; there is no separate
activation confirmation. IAM activation and archive sharing must complete before
the catalog exposes the release publicly. Failed integration steps are retried.

The backend encrypts an authorizing app manager's short-lived IAM access token to finish this
workflow, rechecking permissions on every attempt. An expired or revoked token
leaves the request approved and the application private. An application manager
must sign in and open Sent requests (or the application's publication details) to
renew authorization; publication then resumes without another confirmation.
Saved authorization is removed on completion, denial, revision replacement, or
after 24 hours. A reviewer without app-management authority cannot authorize
publication. Once activation starts, its authorizing manager must resume it;
review history always retains the original requester.

```sh
npm ci
npm run test:unit
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
Release-channel browser checks cover independent histories, required upload
selection, retry identity, production-version validation and promotion, and
experimental install instructions on desktop and mobile.

Real IAM activation and shared lifecycle completion require the integration in
`../docs/IAM-HANDOFF.md`. The console displays pending operations until accepted.
