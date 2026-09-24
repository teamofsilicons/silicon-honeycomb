import { createResource, createSignal, For, Show } from "solid-js";
import { releaseEndpoint, request, type AppRecord } from "./api";

export type ReleaseChannel = "prod" | "dev";
type Release = { version: string; channel: ReleaseChannel; created_at: number; visibility: "private" | "public"; permission_approval_required?: boolean; approval_status?: string };

export default function Releases(props: {
  app: AppRecord;
  refresh: number;
  busy: boolean;
  setBusy: (busy: boolean) => void;
  changed: () => Promise<void>;
}) {
  const [channel, setChannel] = createSignal<ReleaseChannel>("prod");
  const [source, setSource] = createSignal("");
  const [version, setVersion] = createSignal("");
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  let pending: { signature: string; key: string } | undefined;
  const [releases, { refetch }] = createResource(
    () => ({ appId: props.app.app_id, channel: channel(), refresh: props.refresh }),
    ({ appId, channel }) => request<{ items: Release[] }>(`${releaseEndpoint(appId)}/releases?channel=${channel}&include_private=true`),
  );

  async function promote(event: Event) {
    event.preventDefault();
    if (props.busy) return;
    const productionVersion = version().trim();
    if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(productionVersion)) {
      setError("Enter a production version in x.x.x format, such as 2.4.1.");
      return;
    }
    const signature = JSON.stringify([props.app.app_id, props.app.revision, source(), productionVersion]);
    if (pending?.signature !== signature) pending = { signature, key: crypto.randomUUID() };
    props.setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await request(`${releaseEndpoint(props.app.app_id)}/releases/${encodeURIComponent(source())}/promote`, {
        method: "POST",
        headers: { "If-Match": String(props.app.revision), "Idempotency-Key": pending.key },
        body: JSON.stringify({ version: productionVersion }),
      });
      pending = undefined;
      setSource("");
      setVersion("");
      setChannel("prod");
      await props.changed();
      await refetch();
      setNotice(`Production release ${productionVersion} created. The development release is unchanged.`);
      if (result.publication?.state === "request_pending")
        setError(`The release was promoted, but its public approval request needs attention. ${result.publication.error}`);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      props.setBusy(false);
    }
  }

  return <section class="release-history" aria-label="Release history">
    <h3>Release history</h3>
    <label>Release history channel
      <select value={channel()} disabled={props.busy} onChange={e => {
        setChannel(e.currentTarget.value as ReleaseChannel);
        setSource(""); setVersion(""); setError(""); setNotice("");
      }}>
        <option value="prod">Production — official releases</option>
        <option value="dev">Development — experimental releases</option>
      </select>
    </label>
    <p class="muted">Production and development have independent versions and updates. Releases awaiting approval stay private; existing installations keep the last public version.</p>
    <Show when={notice()}><p class="inline-notice" role="status">{notice()}</p></Show>
    <Show when={error()}><p class="field-error" role="alert">{error()}</p></Show>
    <Show when={releases.error}>
      <p class="field-error" role="alert">{String(releases.error?.message || releases.error)}</p>
      <button class="button outline" disabled={props.busy} onClick={() => void refetch()}>Retry release history</button>
    </Show>
    <Show when={!releases.error}>
      <Show when={!releases.loading} fallback={<p class="muted">Loading releases…</p>}>
        <Show when={releases()?.items.length} fallback={<p class="muted">No {channel() === "prod" ? "production" : "development"} releases yet.</p>}>
          <ul class="release-list">
            <For each={releases()?.items}>{release => <li>
              <div><strong>{release.version}</strong> <span class="badge">{release.channel === "dev" ? "Development" : "Production"}</span>
                <span class="badge">{release.visibility === "private" ? "Private" : "Public"}</span>
                <Show when={release.visibility === "private"}>
                  <p class="muted">{release.approval_status === "denied"
                    ? "Approval denied. This release remains private and will not be sent as an update."
                    : release.approval_status === "superseded"
                      ? "A newer application configuration superseded this release. Upload a new release against the current configuration for review."
                      : release.permission_approval_required || release.approval_status === "awaiting_scope_review"
                        ? "This release requires additional permission approval. Existing installations will not update until it becomes public."
                        : props.app.visibility === "public" || props.app.config.visibility === "public"
                          ? "Awaiting publication approval. Existing installations will not update until this release becomes public."
                          : "Available only within the owning organization."}</p>
                </Show>
                <small>{new Date(release.created_at * 1000).toLocaleString()}</small>
                <Show when={release.visibility !== "private" || props.app.visibility === "private"}><code>{`honeycomb install '${props.app.app_id}${release.channel === "dev" ? ">test" : ""}@${release.version}'`}</code></Show>
              </div>
              <Show when={release.channel === "dev"}>
                <button class="button outline" disabled={props.busy} onClick={() => { setSource(release.version); setVersion(""); setError(""); setNotice(""); }}>
                  Promote {release.version} to production
                </button>
              </Show>
            </li>}</For>
          </ul>
        </Show>
      </Show>
    </Show>
    <Show when={source()}>
      <form class="review-form" onSubmit={e => void promote(e)}>
        <h4>Promote development release {source()}</h4>
        <p class="muted">Choose a new production version. Honeycomb creates its production archive from this development release.</p>
        <label>Production version
          <input required placeholder="2.4.1" value={version()} disabled={props.busy} onInput={e => setVersion(e.currentTarget.value)} />
        </label>
        <div class="release-actions">
          <button class="button primary" type="submit" disabled={props.busy}>Create production release</button>
          <button class="button outline" type="button" disabled={props.busy} onClick={() => setSource("")}>Cancel promotion</button>
        </div>
      </form>
    </Show>
  </section>;
}
