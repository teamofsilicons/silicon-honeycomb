import { createSignal, onMount, Show } from "solid-js";
import { Check, ArrowLeft, RefreshCw } from "lucide-solid";
import { request } from "./api";
export default function StorageCallback() {
  const params = new URLSearchParams(window.location.search);
  const id = params.get("authorization_id") || "",
    code = params.get("code") || "",
    state = params.get("state") || "";
  const denied = params.get("error") === "access_denied";
  const [status, setStatus] = createSignal<"loading" | "complete" | "error">(
      "loading",
    ),
    [error, setError] = createSignal("");
  const key = crypto.randomUUID();
  // Keep the short-lived code out of copied URLs and browser history after arrival.
  window.history.replaceState({}, "", "/storage-authorization");
  const notify = (error?: string) =>
    window.opener?.postMessage(
      {
        type: "honeycomb-storage-authorized",
        authorization_id: id,
        state,
        ...(error ? { error } : {}),
      },
      window.location.origin,
    );
  async function complete() {
    setStatus("loading");
    setError("");
    if (!id || !state || (!code && !denied)) {
      setStatus("error");
      setError(
        "This permission return link is incomplete. Return to Honeycomb and start again.",
      );
      return;
    }
    if (denied) {
      notify("access_denied");
      setStatus("error");
      setError(
        "Storage permission was declined. Your files have not been uploaded.",
      );
      return;
    }
    try {
      const result = await request<{ redirect_url?: string | null }>(
        `/api/v1/storage-authorizations/${encodeURIComponent(id)}/complete`,
        {
          method: "POST",
          headers: { "Idempotency-Key": key },
          body: JSON.stringify({ code, state }),
        },
      );
      setStatus("complete");
      notify();
      if (result.redirect_url) {
        const target = new URL(result.redirect_url);
        if (target.origin !== window.location.origin) throw new Error("The saved return destination is invalid.");
        window.location.replace(target.href);
      }
    } catch (error) {
      setError((error as Error).message);
      setStatus("error");
    }
  }
  onMount(() => void complete());
  return (
    <main class="storage-callback">
      <a class="wordmark" href="/">
        <img src="/brand/honeycomb.svg" alt="" />
        honeycomb
      </a>
      <section>
        <span class="eyebrow">STORAGE PERMISSIONS</span>
        <Show when={status() === "loading"}>
          <RefreshCw size={28} class="spinning" />
          <h1>Completing authorization.</h1>
          <p role="status">
            Connecting the permission you approved to your workspace.
          </p>
        </Show>
        <Show when={status() === "complete"}>
          <Check size={30} />
          <h1>You’re ready to continue.</h1>
          <p role="status">
            Storage access is connected. You can return to your application.
          </p>
          <button class="button primary" onClick={() => window.opener ? window.close() : window.location.assign("/")}>
            Return to Honeycomb
            <ArrowLeft size={16} />
          </button>
        </Show>
        <Show when={status() === "error"}>
          <h1>Let’s try that again.</h1>
          <p class="field-error" role="alert">
            {error()}
          </p>
          <Show when={!!code && !!state && !denied}>
            <button class="button primary" onClick={() => void complete()}>
              Retry authorization
            </button>
          </Show>
          <a class="text-link" href="/">
            Return to Honeycomb
          </a>
        </Show>
      </section>
    </main>
  );
}
