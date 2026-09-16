## What is Honeycomb?
Honeycomb brings your application catalog, portable CLI releases, configuration reviews, and shared testing workflows into one place. Browse applications in the **library**, manage your organization's applications in the **console**, or use the same workflows from the Rust CLI and client SDK.

<div class="cards">
<a class="card" href="/quickstart/"><span>01 · GET STARTED</span><strong>Install your first application</strong><p>Set up the CLI, sign in, and discover packages.</p></a>
<a class="card" href="/upload-an-app/"><span>02 · PUBLISH</span><strong>Bring your app to Honeycomb</strong><p>Build six native targets, pack an archive, and upload a release.</p></a>
<a class="card" href="/rust-client/"><span>03 · INTEGRATE</span><strong>Build with the Rust client</strong><p>Use typed catalog calls and explicit, retryable mutations.</p></a>
<a class="card" href="/testing-environments/"><span>04 · TEST</span><strong>Understand isolated environments</strong><p>Learn the lifecycle, dependency imports, and current integration limits.</p></a>
</div>

## Your workspace
| Surface | What you do there |
| --- | --- |
| [Application library](https://honeycomb.teamofsilicons.com) | Browse public packages; sign in to discover private applications you can access. |
| [Developer console](https://console.honeycomb.teamofsilicons.com) | Register applications, upload releases, edit configuration, and review requests. |
| `honeycomb` CLI | Package, install, update, and administer applications from a terminal. |
| `silicon-honeycomb-client` | Integrate Honeycomb into a Rust application without taking on the CLI's local state. |
| [Backend contract discovery](https://backend.honeycomb.teamofsilicons.com/api/contracts) | Discover the HTTP API contract supported by the server. |

## The important distinction
Registering an application creates its identity and configuration. Uploading a release attaches an immutable, installable package. Publication is a separate approval workflow that makes an application public after the required checks complete. A successful upload does not automatically publish it.

IAM continues to authenticate users and enforce accepted permissions. Honeycomb manages application workflows; Briefcase stores package bytes. [Read the architecture](/architecture/).

## Version and availability
These guides describe CLI/client **0.1.0**, package format **1**, and HTTP API **v1**, checked against the repository on **16 September 2026**. Some shared testing, publication, bundle, and adoption workflows still depend on unfinished service contracts. The [availability page](/availability/) distinguishes shipped interfaces from completed production acceptance.
