import { createSignal, onMount, For, Show } from "solid-js";
import { request } from "./api";
type Bundle = { bundle_id: string; app_name: string; app_logo?: string; app_ids: string[]; iam_revision: number };
export default function Bundles(props: { organizations: string[] }) {
  const [items, setItems] = createSignal<Bundle[]>([]);
  const [loaded, setLoaded] = createSignal(false);
  const [id, setId] = createSignal("");
  const [name, setName] = createSignal("");
  const [logo, setLogo] = createSignal("");
  const [members, setMembers] = createSignal("");
  const [revision, setRevision] = createSignal(0);
  const [editing, setEditing] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  let pending: { fingerprint: string; key: string } | undefined;
  async function load() { setLoaded(false); setItems((await request<{items: Bundle[]}>("/api/v1/bundles")).items); setLoaded(true); }
  onMount(() => { void load().catch(e => setError(e.message)); });
  function select(bundle?: Bundle) {
    setId(bundle?.bundle_id || `${props.organizations[0] || ""}>`);
    setName(bundle?.app_name || ""); setLogo(bundle?.app_logo || "");
    setMembers(bundle?.app_ids.join("\n") || ""); setRevision(bundle?.iam_revision || 0);
    setEditing(Boolean(bundle)); setError(""); setNotice(""); pending = undefined;
  }
  async function save(event: SubmitEvent) {
    event.preventDefault(); setBusy(true); setError(""); setNotice("");
    const body = JSON.stringify({ app_name: name().trim(), app_logo: logo().trim() || null, app_ids: members().split(/[\s,]+/).filter(Boolean) });
    const fingerprint = JSON.stringify([id(), revision(), body]);
    if (!pending || pending.fingerprint !== fingerprint) pending = { fingerprint, key: crypto.randomUUID() };
    try {
      const result = await request(`/api/v1/bundles/${encodeURIComponent(id())}`, { method: "PUT", headers: { "If-Match": String(revision()), "Idempotency-Key": pending.key }, body });
      setRevision(result.iam_revision); setEditing(true); pending = undefined;
      setNotice("Bundle accepted by IAM. Its applications retain their own permissions and consent."); await load();
    } catch (e) { setError((e as Error).message); } finally { setBusy(false); }
  }
  return <section>
    <div class="page-heading"><div><span class="eyebrow">BUNDLED APPLICATIONS</span><h1>One sign-in. Your applications.</h1><p>Manage bundle membership for your organization. Each application keeps its own consent and credentials.</p></div><button class="button primary" disabled={busy() || !props.organizations.length} onClick={() => select()}>Create bundle</button></div>
    <Show when={error()}><p class="banner error" role="alert">{error()}</p></Show>
    <Show when={notice()}><p class="banner notice" role="status">{notice()}</p></Show>
    <For each={items()}>{bundle => <button class="review-request-row" disabled={busy()} onClick={() => select(bundle)}><div><h2>{bundle.app_name || bundle.bundle_id}</h2><p class="app-id">{bundle.bundle_id}</p><p>{bundle.app_ids.length} applications</p></div><span>Edit bundle</span></button>}</For>
    <Show when={!loaded() && !error()}><p role="status">Loading bundles…</p></Show>
    <Show when={loaded() && !items().length}><p class="muted">No bundles are available to manage. Creation requires an active, trusted organization enabled by IAM for bundled applications.</p></Show>
    <Show when={id()}><form class="application-form" onSubmit={save}>
      <h2>{editing() ? "Edit bundle" : "Create bundle"}</h2>
      <label>Bundle application ID<input required disabled={busy() || editing()} value={id()} onInput={e => setId(e.currentTarget.value)} placeholder="tos>interface" /></label>
      <label>Name<input required maxlength={200} disabled={busy()} value={name()} onInput={e => setName(e.currentTarget.value)} /></label>
      <label>Logo URL (optional)<input type="url" disabled={busy()} value={logo()} onInput={e => setLogo(e.currentTarget.value)} /></label>
      <label>Application IDs<textarea required rows={8} disabled={busy()} value={members()} onInput={e => setMembers(e.currentTarget.value)} placeholder={"iam\nstarter"} /><small>One per line. Select 1–100 applications owned by the same organization.</small></label>
      <p class="muted">Only a current organization owner or admin may save changes. IAM checks organization eligibility and member availability before accepting them.</p>
      <button class="button primary" disabled={busy()}>{busy() ? "Saving bundle…" : "Save bundle"}</button>
    </form></Show>
  </section>;
}
