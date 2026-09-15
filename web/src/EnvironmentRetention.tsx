import { createResource, createSignal, createEffect, For, Show } from "solid-js";
import { request } from "./api";
export default function EnvironmentRetention(props: { environment: any; refresh: () => Promise<void> }) {
  const [days, setDays] = createSignal(30);
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal("");
  const [retention, { refetch }] = createResource(() => `${props.environment.environment_id}:${props.environment.revision}`, () => request(`/api/v1/environments/${props.environment.environment_id}/retention`));
  createEffect(() => { if (!retention.error && retention()) setDays(retention().idle_days); });
  const date = (seconds: number) => new Date(seconds * 1000).toLocaleString();
  return <section aria-label="Environment retention" class="application-form">
    <h3>Activity & retention</h3>
    <Show when={retention.error}><p class="field-error" role="alert">{retention.error?.message}</p><button class="button outline" onClick={()=>void refetch()}>Retry retention status</button></Show>
    <Show when={!retention.loading && !retention.error && retention()}>
      <p class="muted">Last activity: {date(retention().last_activity)}. Environment idle deadline: {date(retention().delete_after)}.</p>
      <p class="muted">Longer application retention and dependencies are included. Reading this page does not extend the idle period.</p>
      <Show when={retention().automatic_cleanup}>{cleanup=><article class="review-thread" aria-label="Automatic cleanup">
        <strong>Automatic cleanup · {cleanup().kind.replace("environment.", "")}</strong>
        <p>{cleanup().state === "accepted" ? "Completed" : "Waiting for participating services"}</p>
        <p class="app-id">{cleanup().operation_id}</p>
        <Show when={cleanup().applications.length}><p>{cleanup().applications.join(", ")}</p></Show>
        <Show when={cleanup().state === "pending"}><p class="muted">Attempts: {cleanup().attempts}. Next retry: {date(cleanup().next_attempt_at)}.</p><Show when={cleanup().error}><p class="muted">{cleanup().error}</p></Show></Show>
      </article>}</Show>
      <form class="application-form" onSubmit={e=>{
        e.preventDefault(); setSaving(true); setError("");
        void request(`/api/v1/environments/${props.environment.environment_id}/retention`, { method:"PUT", headers:{"If-Match":String(props.environment.revision)}, body:JSON.stringify({idle_days:days()}) })
          .then(async()=>{ await props.refresh(); await refetch(); }).catch(e=>setError(e.message)).finally(()=>setSaving(false));
      }}>
        <label>Environment idle days<input type="number" required min="1" max="36500" value={days()} onInput={e=>setDays(Number(e.currentTarget.value))} /></label>
        <button class="button outline" disabled={saving() || props.environment.state!=="ready" || props.environment.operation_pending}>Save retention</button>
      </form>
      <For each={retention().applications}>{(app: any)=><article class="review-thread"><strong>{app.app_id}</strong><p class="muted">Last activity: {date(app.last_activity)} · Retention: {app.idle_days} days</p><p>{app.required_by_active_app ? "Protected: required by an active application" : app.eligible_for_retirement ? "Idle period elapsed" : "Within retention period"}</p></article>}</For>
    </Show>
    <Show when={error()}><p role="alert" class="field-error">{error()}</p></Show>
  </section>;
}
