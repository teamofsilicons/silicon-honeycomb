import { createMemo, createSignal, Index, Show, For } from "solid-js";
import {importOboDefinition, type DelegationEndpoint} from "./delegation-model";
const warnings = ["uses_credits", "incurs_cost", "stores_data", "shares_data", "deletes_data", "external_service"];
function parse(value: string): DelegationEndpoint[] | undefined {
  try {const items: unknown = JSON.parse(value); return Array.isArray(items) && items.every(item => !!item && typeof item === "object" && !Array.isArray(item)) ? items as DelegationEndpoint[] : undefined;} catch {return;}
}
export default function DelegationEndpoints(props: {kind: "obo" | "ata"; appId: string; value: string; change: (value: string) => void; oboSource?: string}) {
  const endpoints = createMemo(() => parse(props.value)), sources = createMemo(() => parse(props.oboSource || "[]") || []);
  const [notice, setNotice] = createSignal("");
  const save = (values: DelegationEndpoint[]) => props.change(JSON.stringify(values, null, 2));
  function update(index: number, field: string, value: unknown) {const next = [...(endpoints() || [])]; next[index] = {...next[index], [field]: value}; save(next);}
  function jsonField(index: number, field: string, event: InputEvent & {currentTarget: HTMLTextAreaElement}) {
    try {const value: unknown = JSON.parse(event.currentTarget.value); if (field === "downstream" && !Array.isArray(value)) throw new Error(); event.currentTarget.setCustomValidity(""); update(index, field, value);}
    catch {event.currentTarget.setCustomValidity(field === "downstream" ? "Enter a JSON array of dependencies." : "Enter valid JSON metadata.");}
  }
  return <section class="delegation-editor">
    <div class="delegation-heading"><h3>{props.kind.toUpperCase()} endpoints</h3><Show when={endpoints()}><button class="button outline small" type="button" onClick={() => save([...(endpoints() || []), {endpoint_id: "", name: "", description: "", path: "/", critical: false, metadata: {}, downstream: [], additional_warnings: [], enabled: true, ...(props.kind === "obo" ? {ttl_seconds: 300} : {})}])}>Add endpoint</button></Show></div>
    <p class="muted">{props.kind === "obo" ? "Actions other applications may perform on behalf of a user, approved separately from sign-in." : "Actions other applications may perform using their own verification. ATA never grants user access."}</p>
    <Show when={props.kind === "ata" && sources().length}><label>Import an OBO definition<select value="" onChange={event => {const source = sources()[Number(event.currentTarget.value)]; if (!source) return; const imported = importOboDefinition(source); if (endpoints()?.some(item => item.endpoint_id === imported.endpoint_id)) {setNotice("An ATA endpoint with this ID already exists. Edit it below or choose another endpoint."); return;} save([...(endpoints() || []), imported]); setNotice("Definition imported. Add ATA dependencies explicitly; OBO permissions and dependencies were not imported."); event.currentTarget.value = "";}}><option value="" disabled>Choose an OBO endpoint</option><For each={sources()}>{(source, index) => <option value={String(index())}>{source.name || source.endpoint_id}</option>}</For></select></label></Show>
    <Show when={notice()}><p class="inline-notice" role="status">{notice()}</p></Show>
    <Show when={endpoints()} fallback={<p class="field-error" role="alert">Correct the invalid endpoint JSON below to continue.</p>}>
      <Index each={endpoints()}>{(item, index) => <details class="delegation-definition" open>
        <summary><strong>{item().name || item().endpoint_id || "New endpoint"}</strong><code>[{props.appId || "app"}:{props.kind}:{item().endpoint_id || "id"}]</code></summary>
        <div class="delegation-fields">
          <div class="form-grid"><label>Endpoint ID<input required pattern="[a-z][a-z0-9_.-]*" value={item().endpoint_id || ""} onInput={event => update(index, "endpoint_id", event.currentTarget.value)} placeholder="read_private" /></label><label>Name<input required maxLength={200} value={item().name || ""} onInput={event => update(index, "name", event.currentTarget.value)} placeholder="Read private files" /></label></div>
          <label>Endpoint path<input required value={item().path || ""} onInput={event => update(index, "path", event.currentTarget.value)} placeholder="/api/v1/files" /></label>
          <label>Description<textarea required rows="3" value={item().description || ""} onInput={event => update(index, "description", event.currentTarget.value)} /></label>
          <div class="form-grid"><label>Sensitivity<select value={item().critical ? "critical" : "standard"} onChange={event => update(index, "critical", event.currentTarget.value === "critical")}><option value="standard">Non-critical</option><option value="critical">Critical</option></select></label><label>Availability<select value={item().enabled === false ? "disabled" : "enabled"} onChange={event => update(index, "enabled", event.currentTarget.value === "enabled")}><option value="enabled">Enabled</option><option value="disabled">Disabled</option></select></label></div>
          <label>Note to user <span class="optional">Optional</span><textarea rows="2" value={item().note_to_user || ""} onInput={event => update(index, "note_to_user", event.currentTarget.value || null)} /></label>
          <fieldset><legend>Additional warnings</legend><div class="delegation-warnings"><For each={warnings}>{warning => <label class="checkboxes"><input type="checkbox" checked={item().additional_warnings?.includes(warning) || false} onChange={event => update(index, "additional_warnings", event.currentTarget.checked ? [...new Set([...(item().additional_warnings || []), warning])] : (item().additional_warnings || []).filter(value => value !== warning))} />{warning.replaceAll("_", " ")}</label>}</For></div></fieldset>
          <label>Dependency {props.kind.toUpperCase()} endpoints<textarea class="json-input" rows="3" value={JSON.stringify(item().downstream || [], null, 2)} onInput={event => jsonField(index, "downstream", event)} /><small>{`Example: [{"audience":"briefcase","endpoint_id":"files.upload"}]`}. Add only {props.kind.toUpperCase()} dependencies.</small></label>
          <label>Metadata<textarea class="json-input" rows="3" value={JSON.stringify(item().metadata ?? {}, null, 2)} onInput={event => jsonField(index, "metadata", event)} /></label>
          <button type="button" class="text-link" onClick={() => save((endpoints() || []).filter((_, position) => position !== index))}>Remove endpoint</button>
        </div>
      </details>}</Index>
      <Show when={!endpoints()?.length}><p class="muted">No {props.kind.toUpperCase()} endpoints defined.</p></Show>
    </Show>
    <details><summary>Edit endpoint JSON</summary><label class="delegation-json">{props.kind.toUpperCase()} definitions<textarea class="json-input" rows="8" value={props.value} onInput={event => {props.change(event.currentTarget.value); event.currentTarget.setCustomValidity(parse(event.currentTarget.value) ? "" : "Enter a JSON array of endpoints.");}} /></label></details>
  </section>;
}
