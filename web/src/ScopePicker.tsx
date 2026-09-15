import { createEffect, createMemo, createResource, createSignal, For, Show } from "solid-js";
import { request } from "./api";

type Permission = { scope: string; description: string; critical: boolean; eligible: boolean | null };
type Catalog = { items: Permission[]; providers: { app_id: string; name: string }[] };
export default function ScopePicker(props: { org: string; value: string; change: (value: string) => void }) {
  const org = createMemo(() => props.org);
  const [provider, setProvider] = createSignal("");
  createEffect(() => { org(); setProvider(""); });
  const [catalog, { refetch }] = createResource(
    () => org() ? `/api/v1/organizations/${encodeURIComponent(org())}/scope-catalog${provider() ? `?provider=${encodeURIComponent(provider())}` : ""}` : false,
    (url) => request<Catalog>(url),
  );
  const selected = createMemo(() => {
    try {
      const scopes = JSON.parse(props.value);
      return Array.isArray(scopes.iam) && Array.isArray(scopes.external) ? scopes : null;
    } catch { return null; }
  });
  const external = (scope: string) => {
    const prefix = `obo:${provider()}:`;
    return provider() && scope.startsWith(prefix) ? { app_id: provider(), endpoint_id: scope.slice(prefix.length) } : null;
  };
  const checked = (scope: string) => {
    const item = external(scope);
    return item ? selected()?.external.some((value: any) => value.app_id === item.app_id && value.endpoint_id === item.endpoint_id) : selected()?.iam.includes(scope);
  };
  const toggle = (scope: string, enabled: boolean) => {
    if (!selected()) return;
    const next = structuredClone(selected()), item = external(scope);
    if (item) {
      next.external = next.external.filter((value: any) => value.app_id !== item.app_id || value.endpoint_id !== item.endpoint_id);
      if (enabled) next.external.push(item);
    } else {
      next.iam = next.iam.filter((value: string) => value !== scope);
      if (enabled) next.iam.push(scope);
    }
    props.change(JSON.stringify(next, null, 2));
  };
  return <section class="scope-picker" aria-label="Permission catalog">
    <h3>Choose permissions</h3>
    <p class="muted">Choose the information and actions your application needs. Critical permissions require review before public use.</p>
    <label>Permission provider<select value={provider()} onChange={(event) => setProvider(event.currentTarget.value)}>
      <option value="">Silicon IAM</option>
      <For each={catalog.error ? [] : catalog.latest?.providers || []}>{(app) => <option value={app.app_id}>{app.name || app.app_id}</option>}</For>
    </select></label>
    <Show when={catalog.loading}><p role="status" class="muted">Loading available permissions…</p></Show>
    <Show when={catalog.error}><p class="field-error" role="alert">{catalog.error?.message || "Permissions could not be loaded."}</p><button type="button" class="text-link" onClick={() => void refetch()}>Retry permission discovery</button></Show>
    <Show when={!catalog.loading && !catalog.error}>
      <Show when={!selected()}><p class="field-error">Correct the application scopes JSON below before using the permission picker.</p></Show>
      <div class="scope-options"><For each={catalog()?.items || []}>{(permission) => <label class="scope-option">
        <input type="checkbox" aria-label={permission.scope} checked={!!checked(permission.scope)} disabled={!selected() || (permission.eligible !== true && !checked(permission.scope))} onChange={(event) => toggle(permission.scope, event.currentTarget.checked)} />
        <span><strong class="mono">{permission.scope}</strong><small>{permission.description}</small><Show when={permission.critical}><small>Review required for public apps</small></Show><Show when={permission.eligible !== true}><small>Unavailable for this organization</small></Show></span>
      </label>}</For></div>
      <Show when={catalog()?.items.length === 0}><p class="muted">No permissions are available from this provider.</p></Show>
    </Show>
  </section>;
}
