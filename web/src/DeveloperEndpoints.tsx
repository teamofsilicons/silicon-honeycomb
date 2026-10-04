import { createMemo, createSignal, createUniqueId, For, onCleanup, Show } from "solid-js";
import { Check, ChevronDown, Copy, Server } from "lucide-solid";
import type { AppRecord } from "./api";
import { StatusBadge } from "./ui/Arc";
import "./developer-endpoints.css";

type Definition = Record<string, unknown>;
type Kind = "OBO" | "ATA";

function object(value: unknown): Definition | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Definition : undefined;
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function definitions(value: unknown): Definition[] {
  return Array.isArray(value) ? value.flatMap(item => object(item) ? [item as Definition] : []) : [];
}

function origin(value: unknown): string | undefined {
  try {
    const url = new URL(text(value));
    if (url.protocol === "https:" && !url.username && !url.password &&
      url.pathname === "/" && !url.search && !url.hash) return url.origin;
  } catch { /* Missing or invalid public backend origin. */ }
}

function endpointUrl(base: string | undefined, path: unknown): string | undefined {
  const route = text(path);
  if (!base || !route.startsWith("/") || route.startsWith("//") ||
    /[\\?#\s\u0000-\u001f]/.test(route) || route.split("/").some(part => part === "." || part === "..")) return;
  // Preserve route parameters as published, rather than URL-encoding their braces.
  return base + route;
}

function CopyButton(props: { value: string; label: string }) {
  const [copied, setCopied] = createSignal(false);
  const [error, setError] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  return <span class="developer-copy-control">
    <button type="button" class="developer-copy" aria-label={props.label} title={props.label}
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(props.value);
          setCopied(true);
          setError(false);
          clearTimeout(timer);
          timer = setTimeout(() => setCopied(false), 2000);
        } catch {
          setError(true);
        }
      }}>
      <Show when={copied()} fallback={<Copy size={14} aria-hidden="true" />}>
        <Check size={14} aria-hidden="true" />
      </Show>
      <span>{copied() ? "Copied" : "Copy"}</span>
    </button>
    <span class="developer-copy-status" classList={{ "developer-copy-error": error() }} role="status">
      {error() ? "Copy unavailable. Select the text to copy it." : copied() ? "Copied to clipboard." : ""}
    </span>
  </span>;
}

function CodeValue(props: { value: string; label: string }) {
  return <div class="developer-code-value">
    <code>{props.value}</code>
    <CopyButton value={props.value} label={`Copy ${props.label}`} />
  </div>;
}

function EndpointCard(props: { endpoint: Definition; kind: Kind; appId: string; base?: string }) {
  const id = () => text(props.endpoint.endpoint_id);
  const name = () => text(props.endpoint.name) || id() || "Unnamed endpoint";
  const path = () => text(props.endpoint.path);
  const reference = () => `[${props.appId}:${props.kind.toLowerCase()}:${id()}]`;
  const url = () => endpointUrl(props.base, props.endpoint.path);
  const warnings = () => Array.isArray(props.endpoint.additional_warnings)
    ? props.endpoint.additional_warnings.filter((warning): warning is string => typeof warning === "string") : [];
  const dependencies = () => definitions(props.endpoint.downstream);
  const json = () => JSON.stringify(props.endpoint, null, 2);
  const enabled = () => props.endpoint.enabled !== false;
  const ttl = () => props.endpoint.ttl_seconds;
  return <details class="developer-endpoint">
    <summary>
      <div class="developer-endpoint-summary">
        <div class="developer-endpoint-title">
          <h4>{name()}</h4>
          <div class="developer-endpoint-badges">
            <StatusBadge tone={enabled() ? "success" : "neutral"}>{enabled() ? "Enabled" : "Disabled"}</StatusBadge>
            <StatusBadge tone={props.endpoint.critical === true ? "warning" : "neutral"}>
              {props.endpoint.critical === true ? "Critical" : "Standard"}
            </StatusBadge>
          </div>
        </div>
        <div class="developer-endpoint-identifiers">
          <code aria-label="Endpoint ID">{id() || "ID not provided"}</code>
          <span aria-hidden="true">·</span>
          <code aria-label="Endpoint path">{path() || "Path not provided"}</code>
        </div>
      </div>
      <ChevronDown size={17} class="developer-expand-icon" aria-hidden="true" />
    </summary>
    <div class="developer-endpoint-body">
      <p class="developer-endpoint-description">{text(props.endpoint.description) || "No description provided."}</p>
      <dl class="developer-endpoint-fields">
        <div><dt>Endpoint ID</dt><dd><CodeValue value={id() || "Not provided"} label="endpoint ID" /></dd></div>
        <Show when={id()}><div><dt>Global reference</dt><dd><CodeValue value={reference()} label="global endpoint reference" /></dd></div></Show>
        <div><dt>Path</dt><dd><code>{path() || "Not provided"}</code></dd></div>
        <Show when={url()}>{value => <div><dt>Endpoint URL</dt><dd><CodeValue value={value()} label="endpoint URL" /></dd></div>}</Show>
        <Show when={props.kind === "OBO"}>
          <div><dt>Token lifetime</dt><dd>{typeof ttl() === "number" ? `${ttl()} seconds` : "300 seconds (default)"}</dd></div>
        </Show>
        <div><dt>User note</dt><dd>{text(props.endpoint.note_to_user) || "None declared"}</dd></div>
        <div><dt>Warnings</dt><dd>
          <Show when={warnings().length} fallback={<span class="developer-muted">None declared</span>}>
            <ul class="developer-warning-list"><For each={warnings()}>{warning => <li><code>{warning}</code></li>}</For></ul>
          </Show>
        </dd></div>
        <div><dt>Downstream dependencies</dt><dd>
          <Show when={dependencies().length} fallback={<span class="developer-muted">None declared</span>}>
            <ul class="developer-dependencies"><For each={dependencies()}>{dependency => <li>
              <span><span class="developer-dependency-label">Application</span><code>{text(dependency.audience) || "Not provided"}</code></span>
              <span><span class="developer-dependency-label">{props.kind} endpoint</span><code>{text(dependency.endpoint_id) || "Not provided"}</code></span>
            </li>}</For></ul>
          </Show>
        </dd></div>
        <div><dt>Metadata</dt><dd>
          <Show when={props.endpoint.metadata !== undefined && props.endpoint.metadata !== null}
            fallback={<span class="developer-muted">None declared</span>}>
            <pre class="developer-json"><code>{JSON.stringify(props.endpoint.metadata, null, 2)}</code></pre>
          </Show>
        </dd></div>
      </dl>
      <details class="developer-definition">
        <summary>Full endpoint definition <span>JSON</span></summary>
        <div class="developer-definition-toolbar"><CopyButton value={json()} label={`Copy ${id() || props.kind} endpoint definition JSON`} /></div>
        <pre class="developer-json"><code>{json()}</code></pre>
      </details>
    </div>
  </details>;
}

function EndpointSection(props: { kind: Kind; endpoints: Definition[]; appId: string; base?: string }) {
  const headingId = createUniqueId();
  return <section class="developer-endpoint-section" aria-labelledby={headingId}>
    <div class="developer-section-heading">
      <h3 id={headingId}>{props.kind} endpoints <span>{props.endpoints.length}</span></h3>
      <p>{props.kind === "OBO"
        ? "On behalf of a user. These endpoints use explicit user delegation and the user’s permitted access."
        : "Application to application. These endpoints use an application’s own authority, without acting as a user."}</p>
    </div>
    <Show when={props.endpoints.length} fallback={<div class="developer-endpoints-empty">
      <strong>No {props.kind} endpoints published</strong>
      <p>The accepted configuration does not declare any {props.kind} endpoints.</p>
    </div>}>
      <div class="developer-endpoint-list"><For each={props.endpoints}>{endpoint =>
        <EndpointCard endpoint={endpoint} kind={props.kind} appId={props.appId} base={props.base} />
      }</For></div>
    </Show>
  </section>;
}

/** Read-only public discovery. Never fall back to the proposed app.config. */
export default function DeveloperEndpoints(props: { app: AppRecord }) {
  const effective = createMemo(() => {
    const config = object(props.app.effective_config);
    return props.app.effective_revision > 0 && config && Object.keys(config).length ? config : undefined;
  });
  const obo = createMemo(() => definitions(effective()?.obo_endpoints));
  const ata = createMemo(() => definitions(effective()?.ata_endpoints));
  const backend = createMemo(() => origin(effective()?.base_url));
  return <div class="developer-endpoints">
    <div class="developer-introduction">
      <div>
        <h2>Build with {props.app.name}</h2>
        <p>Published delegation endpoints from the current accepted configuration. Endpoint availability does not grant access.</p>
      </div>
      <Show when={props.app.effective_revision > 0}>
        <span class="developer-revision">Revision {props.app.effective_revision}</span>
      </Show>
    </div>
    <div class="developer-backend">
      <div class="developer-backend-label"><Server size={16} aria-hidden="true" /><span>Backend origin</span></div>
      <Show when={backend()} fallback={<p>No backend origin published.</p>}>
        {value => <CodeValue value={value()} label="backend origin" />}
      </Show>
    </div>
    <Show when={effective()} fallback={<p class="developer-config-unavailable">The accepted endpoint configuration is not available yet.</p>}>
      <div class="developer-endpoint-counts" aria-label="Published endpoint counts">
        <span><strong>{obo().length}</strong> OBO endpoints</span>
        <span><strong>{ata().length}</strong> ATA endpoints</span>
      </div>
      <EndpointSection kind="OBO" endpoints={obo()} appId={props.app.app_id} base={backend()} />
      <EndpointSection kind="ATA" endpoints={ata()} appId={props.app.app_id} base={backend()} />
    </Show>
  </div>;
}
