import { createResource, onCleanup, For, Show } from "solid-js";
import { ArrowUpRight, RefreshCw } from "lucide-solid";
import { type AppRecord } from "./api";
import { loadSentRequests, publicationStatus } from "./sent-requests";
export { publicationStatus } from "./sent-requests";

export default function SentRequests(props: {
  revision: number;
  open: (app: AppRecord) => void;
  discuss: (item: { id: string; provider: string }) => void;
}) {
  const [items, { refetch }] = createResource(() => props.revision, () => loadSentRequests());
  const timer = setInterval(() => {
    if (!items.loading && items()?.some(item => !["published", "denied"].includes(item.state))) void refetch();
  }, 15000);
  onCleanup(() => clearInterval(timer));
  return <section aria-label="Sent requests">
    <div class="page-heading">
      <div><span class="eyebrow">YOUR APPLICATIONS</span><h1>Sent requests.</h1><p>Publication and scope requests for apps you manage, including completed requests.</p></div>
      <button class="button outline" disabled={items.loading} onClick={() => void refetch()}><RefreshCw size={16} />Refresh sent requests</button>
    </div>
    <Show when={items.loading}><p role="status">Loading sent requests…</p></Show>
    <Show when={items.error}><p class="field-error" role="alert">Could not load all sent requests. {String(items.error?.message || "Please try again.")}</p></Show>
    <Show when={!items.error && (!items.loading || items())}>
      <Show when={items()?.length} fallback={<p class="muted">No requests sent yet. Open an application’s Releases &amp; publication tab to request a public release or scope review.</p>}>
        <For each={items()}>{item => <article class="sent-request">
          <div class="sent-request-heading"><div><h2>{item.app.name}</h2><p class="app-id">{item.app.app_id} · Revision {item.revision}</p></div><span class="badge">{publicationStatus(item.state)}</span></div>
          <Show when={item.revision !== item.app.revision}><p class="muted">An earlier application revision. Open the application to see its current configuration.</p></Show>
          <Show when={item.state === "awaiting_activation" || item.state === "activating"}><p class="muted">All approvals passed. Honeycomb is completing publication with IAM and release storage.</p></Show>
          <Show when={item.error || item.activation?.error}><p class="field-error" role="alert">{item.error || item.activation?.error}</p></Show>
          <For each={item.gates}>{gate => <p class="sent-request-gate"><span>{gate.provider} · {publicationStatus(gate.state)}</span><button class="text-button" onClick={() => props.discuss({ id: item.id, provider: gate.provider })}>Open discussion</button></p>}</For>
          <button class="button outline" onClick={() => props.open(item.app)}>Open publication details<ArrowUpRight size={16} /></button>
        </article>}</For>
      </Show>
    </Show>
  </section>;
}
