import { createResource, createSignal, For, Show } from "solid-js";
import { releaseEndpoint, request, type AppRecord } from "./api";

export type ReleaseChannel = "prod" | "dev";
type Release = { version: string; channel: ReleaseChannel; created_at: number };

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
    ({ appId, channel }) => request<{ items: Release[] }>(`${releaseEndpoint(appId)}/releases?channel=${channel}`),
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
    <p class="muted">Production and development have independent versions and updates.</p>
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
                <small>{new Date(release.created_at * 1000).toLocaleString()}</small>
                <code>{`honeycomb install '${props.app.app_id}${release.channel === "dev" ? ">test" : ""}@${release.version}'`}</code>
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
