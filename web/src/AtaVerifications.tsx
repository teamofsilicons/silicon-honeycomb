import { createMemo, createSignal, For, Index, onMount, Show } from "solid-js";
import { ArrowRight, Check, ChevronDown, Copy, KeyRound, Plus, RefreshCw, Search, ShieldCheck, X } from "lucide-solid";
import { endpoint, request } from "./api";
import { SegmentedControl, StatusBadge, type StatusTone } from "./ui/Arc";
import { ataDependencies, ataStatus, type AtaEndpointRef as EndpointRef, type AtaGraphNode as GraphNode } from "./ata-model";
import "./ata-verifications.css";

type Preview = {app_id: string; graph_version: string; app_ids: string[]; endpoints: EndpointRef[]; graph: GraphNode[]; requires_expansion: boolean};
type Verification = {id: string; app_id: string; app_ids: string[]; endpoints: EndpointRef[]; graph: GraphNode[]; expires_at: string | null; access_token_validity: number; signing_principal: {id: string; kind: string; display_name?: string}; created_at: string; revoked_at?: string | null; active?: boolean; refresh_token?: string};
type Application = {app_id: string; org_id: string; name: string; can_create: boolean};
type Workspace = {items: Verification[]; applications: Application[]; failures: {app_id: string; message: string}[]; signing_principal: {id: string; kind?: string}; organizations: string[]};
const statusTone = (item: Verification): StatusTone => ataStatus(item) === "Active" ? "success" : ataStatus(item) === "Unavailable" ? "warning" : "neutral";
const date = (value: string) => new Date(value).toLocaleDateString(undefined, {month: "short", day: "numeric", year: "numeric"});
const duration = (seconds: number) => seconds % 3600 === 0 ? `${seconds / 3600} ${seconds === 3600 ? "hour" : "hours"}` : `${seconds / 60} ${seconds === 60 ? "minute" : "minutes"}`;

function Graph(props: {nodes: GraphNode[]}) {
  return <ul class="ata-graph"><For each={props.nodes}>{node => <li>
    <article><div class="ata-action-heading"><strong>{node.name || node.endpoint_id}</strong><StatusBadge tone={node.critical ? "warning" : "neutral"}>{node.critical ? "Critical" : "Non-critical"}</StatusBadge></div>
      <code>{node.ata_id || `[${node.app_id}:ata:${node.endpoint_id}]`}</code><p>{node.description}</p>
      <Show when={node.note_to_user}><p class="ata-endpoint-note">{node.note_to_user}</p></Show>
      <Show when={node.additional_warnings?.length}><div class="ata-warning-list"><For each={node.additional_warnings}>{warning => <StatusBadge tone="warning">{warning.replaceAll("_", " ")}</StatusBadge>}</For></div></Show>
    </article>
    <Show when={node.downstream?.length}><div class="ata-dependencies"><span>Also requires</span><ul><For each={ataDependencies(node, props.nodes)}>{dependency => <li>{dependency.name} <code>{dependency.ata_id}</code></li>}</For></ul></div></Show>
  </li>}</For></ul>;
}

export default function AtaVerifications() {
  const [workspace, setWorkspace] = createSignal<Workspace>();
  const [loading, setLoading] = createSignal(true), [busy, setBusy] = createSignal(false);
  const [loadError, setLoadError] = createSignal(""), [error, setError] = createSignal("");
  const [creating, setCreating] = createSignal(false), [origin, setOrigin] = createSignal("");
  const [recipients, setRecipients] = createSignal("");
  const [endpoints, setEndpoints] = createSignal<EndpointRef[]>([{audience: "", endpoint_id: ""}]);
  const [never, setNever] = createSignal(true), [hours, setHours] = createSignal(24), [minutes, setMinutes] = createSignal(30);
  const [preview, setPreview] = createSignal<Preview>(), [accepted, setAccepted] = createSignal(false), [created, setCreated] = createSignal<Verification>();
  const [revealed, setRevealed] = createSignal(false), [copied, setCopied] = createSignal(false), [revoking, setRevoking] = createSignal<string>();
  const [search, setSearch] = createSignal(""), [status, setStatus] = createSignal("all"), [appFilter, setAppFilter] = createSignal("");
  const retryKeys = new Map<string, string>();
  let originInput: HTMLSelectElement | undefined;
  const items = () => workspace()?.items || [];
  const applications = () => workspace()?.applications || [];
  const availableApps = () => applications().filter(app => app.can_create);
  const activeCount = () => items().filter(item => ataStatus(item) === "Active").length;
  const applicationName = (id: string) => applications().find(app => app.app_id === id)?.name || id;
  const visible = createMemo(() => items().filter(item => {
    const query = search().trim().toLowerCase();
    return (!appFilter() || item.app_id === appFilter())
      && (status() === "all" || (ataStatus(item) === "Active") === (status() === "active"))
      && (!query || [item.app_id, applicationName(item.app_id), ...item.app_ids, ...(item.graph || []).map(node => `${node.name} ${node.endpoint_id}`)].join(" ").toLowerCase().includes(query));
  }));
  const base = (app = origin()) => endpoint(app) + "/ata-verifications";
  function resetPreview() {setPreview(); setAccepted(false); setError("");}
  function payload() {
    const selected = endpoints().map(item => ({audience: item.audience.trim(), endpoint_id: item.endpoint_id.trim()}));
    return {app_ids: [...new Set(recipients().split(/[\s,]+/).filter(Boolean))], endpoints: selected, expires_after: never() ? null : hours() * 3600, access_token_validity: minutes() * 60};
  }
  async function mutate<T>(path: string, value: unknown): Promise<T> {
    const body = JSON.stringify(value), signature = path + body;
    const key = retryKeys.get(signature) || crypto.randomUUID(); retryKeys.set(signature, key);
    const result = await request<T>(path, {method: "POST", headers: {"Idempotency-Key": key}, body});
    if (path.endsWith("/preview")) retryKeys.delete(signature);
    return result;
  }
  async function load() {
    setLoading(true); setLoadError("");
    try {
      const result = await request<Workspace>("/api/v1/ata-verifications");
      if (!Array.isArray(result.items) || !Array.isArray(result.applications) || !Array.isArray(result.failures) || !Array.isArray(result.organizations) || !result.signing_principal?.id) throw new Error("The verification workspace could not be loaded. Please retry.");
      setWorkspace(result);
    } catch (cause) {setLoadError((cause as Error).message);} finally {setLoading(false);}
  }
  onMount(() => void load());
  function start() {
    retryKeys.clear(); resetPreview(); setCreated(); setRevoking(); setCreating(true);
    setOrigin(availableApps().length === 1 ? availableApps()[0].app_id : "");
    setRecipients(""); setEndpoints([{audience: "", endpoint_id: ""}]); setNever(true); setHours(24); setMinutes(30);
    queueMicrotask(() => originInput?.focus());
  }
  async function review(event: SubmitEvent) {
    event.preventDefault(); if (busy()) return; setBusy(true); setError("");
    try {
      const result = await mutate<Preview>(base() + "/preview", payload());
      if (result.app_id !== origin() || !result.graph_version || !Array.isArray(result.app_ids) || !Array.isArray(result.endpoints) || !Array.isArray(result.graph)) throw new Error("The complete verification could not be loaded. Please retry.");
      setPreview(result); setAccepted(false);
    } catch (cause) {setError((cause as Error).message);} finally {setBusy(false);}
  }
  async function create() {
    if (!preview() || !accepted() || busy()) return; setBusy(true); setError("");
    try {
      const result = await mutate<Verification>(base(), {...payload(), app_ids: preview()!.app_ids, endpoints: preview()!.endpoints, graph_version: preview()!.graph_version});
      if (!result.refresh_token || result.app_id !== origin() || result.signing_principal?.id !== workspace()?.signing_principal.id) throw new Error("The credential could not be confirmed. Retry the same request to recover its result.");
      setCreated(result); const {refresh_token: _secret, ...metadata} = result;
      setWorkspace(current => current && ({...current, items: [metadata, ...current.items.filter(item => item.id !== result.id)]}));
      setCreating(false); setPreview(); setRevealed(false); setCopied(false); setSearch(""); setStatus("all"); setAppFilter("");
    } catch (cause) {setError((cause as Error).message);} finally {setBusy(false);}
  }
  async function revoke(item: Verification) {
    if (busy()) return; setBusy(true); setError("");
    try {
      const value = await mutate<Verification>(base(item.app_id) + `/${encodeURIComponent(item.id)}/revoke`, {});
      if (value.id !== item.id || value.app_id !== item.app_id || !value.revoked_at) throw new Error("The revocation could not be confirmed. Retry the same action.");
      setWorkspace(current => current && ({...current, items: current.items.map(existing => existing.id === item.id ? value : existing)}));
      if (created()?.id === item.id) setCreated(); setRevoking();
    } catch (cause) {setError((cause as Error).message);} finally {setBusy(false);}
  }
  return <section class="ata-page">
    <div class="page-heading"><div><h1>App to App verifications</h1><p>Manage the verifications you create for other applications, in one place.</p></div><button type="button" class="button primary" disabled={loading() || busy() || creating() || !availableApps().length} onClick={start}><Plus size={16} />New verification</button></div>
    <Show when={loadError()}><div class="ata-notice error" role="alert"><div><strong>Could not refresh your verifications</strong><p>{loadError()}</p><Show when={workspace()}><p>The previous results are still shown below.</p></Show></div><button class="button outline small" type="button" disabled={loading()} onClick={() => void load()}>Retry</button></div></Show>
    <Show when={workspace()}>{data => <>
      <div class="ata-workspace-summary"><div class="ata-summary-icon"><ShieldCheck size={20} /></div><div><strong>{activeCount()} active {activeCount() === 1 ? "verification" : "verifications"}</strong><p>Signed by <span>{data().signing_principal.id}</span><Show when={data().organizations.length}> · {data().organizations.join(", ")}</Show></p></div><button type="button" class="button outline small" disabled={loading() || busy()} onClick={() => void load()}><RefreshCw size={14} />{loading() ? "Refreshing…" : "Refresh"}</button></div>
      <Show when={data().failures.length}><div class="ata-notice warning" role="status"><div><strong>Some applications could not be checked</strong><p>This list is incomplete. Retry to check for additional verifications.</p><ul><For each={data().failures}>{failure => <li><strong>{applicationName(failure.app_id)}</strong>: {failure.message}</li>}</For></ul></div><button class="button outline small" type="button" disabled={loading()} onClick={() => void load()}>Retry</button></div></Show>
      <Show when={error()}><p class="field-error ata-action-error" role="alert">{error()}</p></Show>
      <Show when={created()?.refresh_token}><section class="ata-secret" aria-label="New verification credential"><div class="ata-section-title"><span class="ata-step complete"><Check size={16} /></span><div><h2>Your verification is ready</h2><p>Save this refresh token in {applicationName(created()!.app_id)}’s backend. It is only available now.</p></div></div><div class="ata-secret-value"><input readonly type={revealed() ? "text" : "password"} value={created()!.refresh_token} aria-label="ATA refresh token" autocomplete="off" spellcheck={false} /><button type="button" class="button outline" onClick={() => setRevealed(!revealed())}>{revealed() ? "Hide" : "Reveal"}</button><button type="button" class="button outline" onClick={async () => {try {await navigator.clipboard.writeText(created()!.refresh_token!); setCopied(true);} catch {setError("Copy failed. Reveal and copy the token manually.");}}}>{copied() ? <Check size={15} /> : <Copy size={15} />}{copied() ? "Copied" : "Copy"}</button></div><button type="button" class="text-link" onClick={() => setCreated()}>I have saved it</button></section></Show>
      <Show when={creating()}><form class="ata-create" onSubmit={review} aria-label="New App to App verification">
        <div class="ata-form-heading"><div><h2>New verification</h2><p>Choose which app is delegating and the actions other apps can perform for it.</p></div><button type="button" class="icon-button" aria-label="Cancel new verification" disabled={busy()} onClick={() => {setCreating(false); resetPreview();}}><X size={18} /></button></div>
        <fieldset disabled={busy() || !!preview()} hidden={!!preview()}>
          <section class="ata-form-section"><div class="ata-section-title"><span class="ata-step">1</span><div><h3>Choose your application</h3><p>The verification will act on behalf of this application.</p></div></div><div class="ata-form-fields"><label>Application<select required ref={originInput} value={origin()} onChange={event => {setOrigin(event.currentTarget.value); resetPreview();}}><option value="" disabled>Select an application</option><For each={availableApps()}>{app => <option value={app.app_id}>{app.name} ({app.app_id}) · {app.org_id}</option>}</For></select></label><div class="ata-signer"><span>Signed by</span><strong>{data().signing_principal.id}</strong><small>Your current account is recorded automatically.</small></div></div></section>
          <section class="ata-form-section"><div class="ata-section-title"><span class="ata-step">2</span><div><h3>Choose actions in other applications</h3><p>Use each provider’s application ID and ATA endpoint ID. Dependencies are included in the next step for your review.</p></div></div><div class="ata-form-fields"><label class="ata-recipient-field">Applications allowed to use this verification<input required value={recipients()} placeholder="waveform, briefcase" autocomplete="off" spellcheck={false} onInput={event => {setRecipients(event.currentTarget.value); resetPreview();}} /><small>Application IDs, separated by commas. Review will show the final list, including dependencies.</small></label><Index each={endpoints()}>{(item, index) => <div class="ata-endpoint-row"><label>Provider application<input required value={item().audience} onInput={event => {setEndpoints(current => current.map((value, position) => position === index ? {...value, audience: event.currentTarget.value} : value)); resetPreview();}} placeholder="waveform" autocomplete="off" spellcheck={false} /></label><label>ATA endpoint<input required value={item().endpoint_id} onInput={event => {setEndpoints(current => current.map((value, position) => position === index ? {...value, endpoint_id: event.currentTarget.value} : value)); resetPreview();}} placeholder="text_to_speech" autocomplete="off" spellcheck={false} /></label><button class="icon-button" type="button" aria-label={`Remove endpoint ${index + 1}`} disabled={endpoints().length === 1} onClick={() => {setEndpoints(current => current.filter((_, position) => position !== index)); resetPreview();}}><X size={16} /></button></div>}</Index><button type="button" class="button outline small" disabled={endpoints().length >= 64} onClick={() => {setEndpoints(current => [...current, {audience: "", endpoint_id: ""}]); resetPreview();}}><Plus size={14} />Add endpoint</button></div></section>
          <section class="ata-form-section"><div class="ata-section-title"><span class="ata-step">3</span><div><h3>Set the lifetime</h3><p>You can revoke a verification at any time.</p></div></div><div class="ata-form-fields ata-lifetime"><label>Verification expires<select value={never() ? "never" : "custom"} onChange={event => {setNever(event.currentTarget.value === "never"); resetPreview();}}><option value="never">Never</option><option value="custom">After a set time</option></select></label><Show when={!never()}><label>Hours until expiry<input type="number" required min="1" step="1" value={hours()} onInput={event => {setHours(Number(event.currentTarget.value)); resetPreview();}} /></label></Show><label>Access token validity<input required type="number" min="1" max="1440" step="1" value={minutes()} onInput={event => {setMinutes(Number(event.currentTarget.value)); resetPreview();}} /><small>Minutes · between 1 minute and 24 hours</small></label></div></section>
        </fieldset>
        <Show when={!preview()}><div class="ata-form-actions"><button type="button" class="button outline" disabled={busy()} onClick={() => {setCreating(false); resetPreview();}}>Cancel</button><button class="button primary" disabled={busy()}>{busy() ? "Loading dependencies…" : "Review verification"}<ArrowRight size={15} /></button></div></Show>
        <Show when={preview()}>{value => <section class="ata-review"><div class="ata-section-title"><span class="ata-step"><ShieldCheck size={16} /></span><div><h3>Review the complete verification</h3><p>{value().requires_expansion ? "The selected apps and actions require changes. Review the final applications and complete endpoint list below before approving." : "These applications and actions will be included."}</p></div></div><div class="ata-review-route"><strong>{applicationName(origin())}</strong><ArrowRight size={16} aria-hidden="true" /><div><For each={value().app_ids}>{app => <StatusBadge tone="info">{app}</StatusBadge>}</For></div></div><Show when={payload().app_ids.some(app => !value().app_ids.includes(app))}><p class="ata-unavailable">These entered apps are not included in the endpoint chain: {payload().app_ids.filter(app => !value().app_ids.includes(app)).join(", ")}. They will not receive access. Edit this verification to choose their endpoints if they should be included.</p></Show><dl class="ata-facts"><div><dt>Verification expires</dt><dd>{never() ? "Never" : `After ${hours()} ${hours() === 1 ? "hour" : "hours"}`}</dd></div><div><dt>Access token validity</dt><dd>{duration(minutes() * 60)}</dd></div><div><dt>Signed by</dt><dd>{data().signing_principal.id}</dd></div></dl><Graph nodes={value().graph} /><label class="ata-approval"><input type="checkbox" checked={accepted()} onChange={event => setAccepted(event.currentTarget.checked)} /><span>I approve every application and endpoint shown, including their dependencies.</span></label><div class="ata-form-actions"><button type="button" class="button outline" disabled={busy()} onClick={resetPreview}>Edit verification</button><button type="button" class="button primary" disabled={busy() || !accepted()} onClick={() => void create()}>{busy() ? "Creating…" : "Create verification"}</button></div></section>}</Show>
      </form></Show>
      <Show when={items().length}><div class="ata-toolbar"><SegmentedControl label="Verification status" value={status()} onChange={setStatus} options={[{value:"all",label:`All (${items().length})`},{value:"active",label:"Active"},{value:"inactive",label:"Inactive"}]} /><div class="ata-list-controls"><label class="ata-search"><Search size={16} /><input type="search" aria-label="Search verifications" placeholder="Search apps or endpoints" value={search()} onInput={event => setSearch(event.currentTarget.value)} /></label><select aria-label="Filter by originating application" value={appFilter()} onChange={event => setAppFilter(event.currentTarget.value)}><option value="">All applications</option><For each={applications().filter(app => items().some(item => item.app_id === app.app_id))}>{app => <option value={app.app_id}>{app.name}</option>}</For></select></div></div></Show>
      <Show when={items().length} fallback={<div class="ata-empty"><span><KeyRound size={26} /></span><h2>{data().failures.length ? "No verifications loaded yet" : "Your app connections start here"}</h2><p>{availableApps().length ? "Create a verification to let other applications perform selected actions for one of your apps." : "You need an application registered with IAM and an owner or admin role to create a verification."}</p><Show when={availableApps().length && !creating()}><button type="button" class="button outline" onClick={start}><Plus size={15} />Create your first verification</button></Show></div>}>
        <Show when={visible().length} fallback={<div class="ata-empty compact"><h2>No matching verifications</h2><p>Try another application, status, or search.</p><button type="button" class="text-link" onClick={() => {setSearch(""); setStatus("all"); setAppFilter("");}}>Clear filters</button></div>}><div class="ata-list"><For each={visible()}>{item => <article class="ata-verification">
          <div class="ata-verification-heading"><div class="ata-record-icon"><KeyRound size={19} /></div><div><h2>{applicationName(item.app_id)}<ArrowRight size={16} aria-label="authorizes" />{item.app_ids.join(", ")}</h2><p>{item.app_id} · Created {date(item.created_at)}</p></div><StatusBadge tone={statusTone(item)}>{ataStatus(item)}</StatusBadge></div>
          <Show when={ataStatus(item) === "Unavailable"}><p class="ata-unavailable">An application or endpoint has changed. This verification can no longer be used; create a replacement with the current endpoints.</p></Show>
          <dl class="ata-facts"><div><dt>Signed by</dt><dd>{item.signing_principal.display_name || item.signing_principal.id}</dd></div><div><dt>Verification expires</dt><dd>{item.expires_at ? new Date(item.expires_at).toLocaleString(undefined, {month:"short", day:"numeric", year:"numeric", hour:"numeric", minute:"2-digit"}) : "Never"}</dd></div><div><dt>Access token validity</dt><dd>{duration(item.access_token_validity)}</dd></div></dl>
          <div class="ata-record-footer"><details><summary>{item.endpoints.length} approved {item.endpoints.length === 1 ? "endpoint" : "endpoints"}<ChevronDown size={14} /></summary><Graph nodes={item.graph || []} /></details><Show when={!item.revoked_at && revoking() !== item.id}><button type="button" class="ata-revoke-link" disabled={busy() || creating()} onClick={() => {setRevoking(item.id); setError("");}}>Revoke</button></Show></div>
          <Show when={revoking() === item.id}><div class="ata-revoke-confirm"><div><strong>Revoke this verification?</strong><p>{item.app_ids.join(", ")} will no longer be able to use it for {applicationName(item.app_id)}.</p></div><div><button class="button outline small" type="button" disabled={busy()} onClick={() => setRevoking()}>Keep verification</button><button class="button danger small" type="button" disabled={busy()} onClick={() => void revoke(item)}>{busy() ? "Revoking…" : "Revoke verification"}</button></div></div></Show>
        </article>}</For></div></Show>
      </Show>
      <p class="ata-workspace-note">Showing verifications signed by your current account for applications you can manage in your current account context. Configure ATA endpoints in each application’s settings.</p>
    </>}</Show>
    <Show when={loading() && !workspace()}><div class="ata-empty" role="status"><span><KeyRound size={26} /></span><h2>Loading your verifications…</h2></div></Show>
  </section>;
}
