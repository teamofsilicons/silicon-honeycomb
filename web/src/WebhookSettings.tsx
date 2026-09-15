import { createResource, createSignal, For, Show } from "solid-js";
import { AppRecord, endpoint, request } from "./api";

export default function WebhookSettings(props: { app: AppRecord; refresh: () => Promise<void> }) {
  const [proof, setProof] = createSignal("");
  const [secret, setSecret] = createSignal("");
  const [confirmation, setConfirmation] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal("");
  const [error, setError] = createSignal("");
  const [state, { refetch }] = createResource(() => `${props.app.app_id}:${props.app.iam_revision}`, async () => {
    const [webhook, operations] = await Promise.all([
      request(endpoint(props.app.app_id) + "/webhook"),
      request(endpoint(props.app.app_id) + "/operations"),
    ]);
    return { webhook, operations: operations.items };
  });
  const pending = () => state()?.operations.some((op: any) => op.state === "pending");
  const ready = () => !state.loading && !state.error && !busy() && !pending() && state()?.webhook.iam_revision === props.app.iam_revision && props.app.effective_revision === props.app.revision;
  async function change(action: "approve" | "rotate", operation?: string) {
    setBusy(true); setMessage(""); setError("");
    const body = operation ? { step_up_assertion: proof() || null } : {
      action, step_up_assertion: proof() || null,
      ...(action === "approve" ? { pending_endpoint_id: state()?.webhook.pending_endpoint_id } : { webhook_secret: secret() }),
    };
    try {
      const result = await request(operation ? `/api/v1/operations/${operation}/webhook-retry` : endpoint(props.app.app_id) + "/webhook", {
        method: "POST", headers: { "If-Match": String(props.app.revision) }, body: JSON.stringify(body),
      });
      setMessage(result.state === "accepted" ? (action === "approve" ? "Webhook destination approved." : "Webhook signing secret rotated. Update your receiver to verify the new secret.") : "Change saved and pending IAM acceptance. Retry the saved operation below with fresh verification if required.");
      await props.refresh(); await refetch();
    } catch (e) { setError((e as Error).message); await refetch(); }
    finally { setBusy(false); setSecret(""); setProof(""); setConfirmation(""); }
  }
  return <section aria-label="Webhook management" class="application-form">
    <div class="section-divider" />
    <h3>Webhook management</h3>
    <p class="muted">Approve a pending destination or replace its signing secret after verifying your identity with IAM.</p>
    <Show when={state.loading}><p role="status">Loading IAM webhook state…</p></Show>
    <Show when={state.error}><p role="alert" class="field-error">{state.error?.message}</p><button class="button outline" onClick={() => void refetch()}>Retry webhook status</button></Show>
    <Show when={!state.loading && !state.error && state()}>
      <p class="muted">Requested destination: {props.app.config.webhook_url}</p>
      <Show when={state()?.webhook.application_id}><p class="app-id">IAM resource: {state()?.webhook.application_id}</p></Show>
      <p class="muted">Obtain a fresh IAM step-up assertion for this resource: application.webhook.approve for approval, or application.webhook_secret.rotate for rotation.</p>
      <label>IAM webhook step-up assertion<input type="password" autocomplete="off" value={proof()} onInput={e => setProof(e.currentTarget.value)} /></label>
      <Show when={state()?.webhook.pending_endpoint_id} fallback={<p class="muted">IAM has no destination awaiting approval.</p>}>
        <p class="app-id">Pending endpoint: {state()?.webhook.pending_endpoint_id}</p>
        <button type="button" class="button outline" disabled={!ready() || !proof()} onClick={() => void change("approve")}>Approve webhook destination</button>
      </Show>
      <label>New webhook signing secret<input type="password" autocomplete="new-password" minlength={32} maxlength={4096} value={secret()} onInput={e => setSecret(e.currentTarget.value)} /></label>
      <label>Confirm application ID for webhook rotation<input value={confirmation()} onInput={e => setConfirmation(e.currentTarget.value)} /></label>
      <button type="button" class="button outline" disabled={!ready() || !proof() || secret().length < 32 || confirmation() !== props.app.app_id} onClick={() => void change("rotate")}>Rotate webhook signing secret</button>
      <For each={state()?.operations.filter((op: any) => op.kind.startsWith("webhook."))}>{(op: any) => <article class="review-thread">
        <strong>{op.kind === "webhook.approve" ? "Webhook approval" : "Webhook signing-secret rotation"}</strong><p>{op.state}</p><p class="app-id">{op.id}</p>
        <Show when={op.error}><p class="muted">{op.error === "step_up_required" ? "IAM requires fresh identity verification for this action." : op.error}</p></Show>
        <Show when={op.state === "pending"}><button type="button" class="button outline" disabled={busy() || !proof()} onClick={() => void change(op.kind === "webhook.approve" ? "approve" : "rotate", op.id)}>Retry webhook change</button></Show>
      </article>}</For>
    </Show>
    <Show when={message()}><p role="status">{message()}</p></Show>
    <Show when={error()}><p role="alert" class="field-error">{error()}</p></Show>
  </section>;
}
