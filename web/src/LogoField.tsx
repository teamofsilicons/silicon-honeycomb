import { Show, createSignal, onCleanup } from "solid-js";
import { request, safeLink } from "./api";

export function LogoField(props: { org: string; value: string; change: (url: string) => void; onBusy: (busy: boolean) => void }) {
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [attempt, setAttempt] = createSignal<{ file: File; org: string; key: string }>();
  let active = true;
  onCleanup(() => { active = false; props.onBusy(false); });
  async function upload(file?: File) {
    if (busy()) return;
    if (file) {
      if (!file.size || file.size > 2 * 1024 * 1024) { setAttempt(undefined); setError("Choose an image no larger than 2 MiB."); return; }
      setAttempt({ file, org: props.org, key: crypto.randomUUID() });
    }
    const pending = attempt();
    if (!pending || pending.org !== props.org) return;
    setBusy(true); props.onBusy(true); setError("");
    try {
      const result = await request(`/api/v1/organizations/${encodeURIComponent(pending.org)}/logos`, {
        method: "POST", headers: { "Content-Type": "application/octet-stream", "Idempotency-Key": pending.key }, body: pending.file,
      });
      if (active && props.org === pending.org) { props.change(result.logo_url); setAttempt(undefined); }
    } catch (e) {
      if (active) setError((e as Error).message);
    } finally {
      if (active) { setBusy(false); props.onBusy(false); }
    }
  }
  return <section class="logo-field" aria-label="Application logo">
    <label>Logo URL <span class="optional">Optional</span>
      <input type="url" placeholder="https://example.com/logo.png" value={props.value || ""} disabled={busy()} onInput={e => props.change(e.currentTarget.value)} />
    </label>
    <Show when={safeLink(props.value)}>{url => <img class="logo-preview" src={url()} alt="Application logo preview" referrerpolicy="no-referrer" />}</Show>
    <label class="upload-zone">
      <strong>{busy() ? "Uploading logo…" : "Upload a logo"}</strong>
      <span>PNG, JPEG or WebP · Up to 2 MiB · Maximum 2048 × 2048</span>
      <input type="file" aria-label="Upload a logo" accept="image/png,image/jpeg,image/webp" disabled={busy() || !props.org} onChange={e => { const file=e.currentTarget.files?.[0]; e.currentTarget.value=""; if(file) void upload(file); }} />
    </label>
    <small>Uploaded logos are stored in Briefcase with a publicly viewable link. Your application's access settings remain separate.</small>
    <Show when={error()}><p role="alert">{error()}</p><Show when={attempt()?.org === props.org}><button type="button" class="button outline" disabled={busy()} onClick={() => void upload()}>Retry logo upload</button></Show></Show>
  </section>;
}
