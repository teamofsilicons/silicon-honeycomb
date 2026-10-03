import { createSignal, onCleanup, Show } from "solid-js";
import { CheckCheck, LoaderCircle } from "lucide-solid";
import { request } from "./api";
import { markRequestsRead, type ReadBatch } from "./request-read";

export default function MarkAllRead(props: {
  view: "sent" | "received";
  disabled: boolean;
  load: (read: typeof request) => Promise<ReadBatch>;
  refresh: () => Promise<unknown>;
}) {
  const [busy, setBusy] = createSignal(false);
  const [progress, setProgress] = createSignal("");
  const [message, setMessage] = createSignal("");
  const [failed, setFailed] = createSignal(false);
  let active = true;
  const controller = new AbortController();
  onCleanup(() => { active = false; controller.abort(); });
  const read: typeof request = (path, options = {}) => {
    if (!active) return Promise.reject(new Error("Request list closed"));
    return request(path, { ...options, signal: controller.signal });
  };
  async function markAll() {
    if (busy() || props.disabled) return;
    setBusy(true); setMessage(""); setFailed(false); setProgress("Marking as read…");
    try {
      const batch = await props.load(read);
      if (!active) return;
      const result = await markRequestsRead(batch.items, props.view,
        (done, total) => { if (active) setProgress(`Marking ${done} of ${total}…`); }, read);
      if (!active) return;
      setFailed(result.failed > 0 || (batch.partial === true && batch.items.length === 0));
      const outcome = result.failed
        ? `Marked ${result.marked} as read. ${result.failed} could not be updated. Try again.`
        : result.marked ? `Marked ${result.marked} request${result.marked === 1 ? "" : "s"} as read.`
        : batch.partial ? "Could not check all requests. Refresh and try again."
        : "No unread requests to update.";
      setMessage(outcome + (batch.partial && result.marked ? " Some requests could not be checked." : ""));
      await props.refresh();
    } catch {
      if (active) { setFailed(true); setMessage("Could not finish marking requests as read. Refresh and try again."); }
    } finally { if (active) setBusy(false); }
  }
  return <div class="requests-read-action">
    <button class="button outline" disabled={props.disabled || busy()} onClick={() => void markAll()} aria-busy={busy()}>
      <Show when={busy()} fallback={<CheckCheck size={16} />}><LoaderCircle size={16} class="spinning" /></Show>
      {busy() ? progress() : "Mark all as read"}
    </button>
    <Show when={message()}><p class="requests-read-feedback" classList={{ "field-error": failed() }} role={failed() ? "alert" : "status"}>{message()}</p></Show>
  </div>;
}
