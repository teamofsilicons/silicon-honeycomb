import { createResource, createSignal, For, Show } from "solid-js";
import { ArrowUpRight, Check, Clock3, GitBranch, LockKeyhole, Package, ShieldCheck } from "lucide-solid";
import { applicationPath } from "./application-route";
import { authorizeStorage } from "./storage-authorization";
import { ApiError, releaseEndpoint, request, type AppRecord } from "./api";
import { SegmentedControl, StatusBadge } from "./ui/Arc";
import "./release-pages.css";

export type ReleaseChannel = "prod" | "dev";
type Release = { version: string; channel: ReleaseChannel; created_at: number; visibility: "private" | "public"; permission_approval_required?: boolean; approval_status?: string };

export default function Releases(props: {
  app: AppRecord;
  libraryOrigin: string;
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
  const [needsStorage,setNeedsStorage]=createSignal(false);
  async function connectStorage(){
    props.setBusy(true);setError("");
    try{await authorizeStorage(props.app.org_id);setNeedsStorage(false);setNotice("Storage permission connected. Retry the promotion below.");}
    catch(error){setError((error as Error).message);}
    finally{props.setBusy(false);}
  }
  let pending: { signature: string; key: string } | undefined;
  const [releases, { refetch }] = createResource(
    () => ({ appId: props.app.app_id, channel: channel(), refresh: props.refresh }),
    ({ appId, channel }) => request<{ items: Release[] }>(`${releaseEndpoint(appId)}/releases?channel=${channel}&include_private=true`),
  );

  // Management app records may include a higher private version. The first
  // public production release is the version consumers currently receive.
  const currentPublicVersion = () => releases()?.items.find(release => release.channel === "prod" && release.visibility === "public")?.version;
  const latestPrivateVersion = () => {
    const latest = releases()?.items.find(release => release.channel === "prod");
    return latest?.visibility === "private" ? latest.version : undefined;
  };
  const isCurrent = (release: Release) => release.channel === "prod" && release.visibility === "public" && release.version === currentPublicVersion();
  const releaseStatus = (release: Release) => {
    if (release.visibility === "public") return "Public";
    if (release.approval_status === "denied") return "Review denied";
    if (release.approval_status === "superseded") return "Superseded";
    if (release.permission_approval_required || release.approval_status === "awaiting_scope_review") return "Permission review";
    if (props.app.visibility === "public" || props.app.config.visibility === "public") return "Awaiting approval";
    return "Private";
  };

  function selectChannel(value: string) {
    setChannel(value as ReleaseChannel);
    setSource(""); setVersion(""); setError(""); setNotice("");
  }

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
      if(e instanceof ApiError && e.code==="storage_authorization_required")setNeedsStorage(true);
    } finally {
      props.setBusy(false);
    }
  }

  return <section class="release-history arc-release-history" aria-label="Release history">
    <div class="release-section-heading">
      <div><h3>Release history</h3><p class="muted">Every version, with its own page and publication status.</p></div>
      <fieldset disabled={props.busy} class="release-channel-control">
        <SegmentedControl label="Release history channel" value={channel()} onChange={selectChannel} options={[
          { value: "prod", label: "Production" },
          { value: "dev", label: "Development" },
        ]} />
      </fieldset>
    </div>
    <div class="release-channel-note"><ShieldCheck size={17} aria-hidden="true"/><p>Releases awaiting approval stay private. Existing installations keep the last public version.</p></div>
    <Show when={notice()}><p class="inline-notice" role="status">{notice()}</p></Show>
    <Show when={needsStorage()}><button class="button primary" disabled={props.busy} onClick={()=>void connectStorage()}>Review storage permissions</button></Show>
    <Show when={error()}><p class="field-error" role="alert">{error()}</p></Show>
    <Show when={releases.error}>
      <p class="field-error" role="alert">{String(releases.error?.message || releases.error)}</p>
      <button class="button outline" disabled={props.busy} onClick={() => void refetch()}>Retry release history</button>
    </Show>
    <Show when={!releases.error}>
      <Show when={!releases.loading} fallback={<div class="release-empty" role="status"><Package size={22} aria-hidden="true"/><span>Loading releases…</span></div>}>
        <Show when={releases()?.items.length} fallback={<div class="release-empty"><Package size={24} aria-hidden="true"/><div><strong>No {channel() === "prod" ? "production" : "development"} releases yet</strong><p>Upload an archive to this channel to give your application its first version.</p></div></div>}>
          <div class="release-list-caption"><span>{channel() === "prod" ? "Official releases" : "Experimental releases"}</span><span>{releases()?.items.length} {releases()?.items.length === 1 ? "version" : "versions"}</span></div>
          <ul class="release-list">
            <For each={releases()?.items}>{release => <li classList={{ "release-current": isCurrent(release), "release-private": release.visibility === "private" }}>
              <div class="release-item-main">
                <div class="release-item-heading"><span class="release-item-icon" aria-hidden="true"><Show when={release.visibility === "public"} fallback={<LockKeyhole size={17}/>}><Package size={17}/></Show></span><strong>{release.version}</strong>
                  <Show when={isCurrent(release)}><StatusBadge tone="info"><Check size={12} aria-hidden="true"/>Current version</StatusBadge></Show>
                  <Show when={release.channel === "prod" && release.visibility === "private" && release.version === latestPrivateVersion()}><StatusBadge tone="neutral">Latest private version</StatusBadge></Show>
                  <StatusBadge tone={release.visibility === "public" ? "success" : release.approval_status === "denied" ? "danger" : releaseStatus(release) === "Private" || release.approval_status === "superseded" ? "neutral" : "warning"}>{releaseStatus(release)}</StatusBadge>
                </div>
                <Show when={release.visibility === "private"}>
                  <p class="release-visibility-note">{release.approval_status === "denied"
                    ? "Approval denied. This release remains private and will not be sent as an update."
                    : release.approval_status === "superseded"
                      ? "A newer application configuration superseded this release. Upload a new release against the current configuration for review."
                      : release.permission_approval_required || release.approval_status === "awaiting_scope_review"
                        ? "This release requires additional permission approval. Existing installations will not update until it becomes public."
                        : props.app.visibility === "public" || props.app.config.visibility === "public"
                          ? "Awaiting publication approval. Existing installations will not update until this release becomes public."
                          : "Available only within the owning organization."}</p>
                </Show>
                <div class="release-item-meta"><span><Clock3 size={13} aria-hidden="true"/><time dateTime={new Date(release.created_at * 1000).toISOString()}>{new Date(release.created_at * 1000).toLocaleString()}</time></span><a class="text-link" href={`${props.libraryOrigin}${applicationPath(props.app.app_id,release.channel,release.version)}`} target="_blank" rel="noopener noreferrer">Open version page<ArrowUpRight size={14} aria-hidden="true"/></a></div>
                <Show when={release.visibility !== "private" || props.app.visibility === "private"}><div class="release-install"><span>Install this version</span><code>{`honeycomb install '${props.app.app_id}${release.channel === "dev" ? ">test" : ""}@${release.version}'`}</code></div></Show>
              </div>
              <Show when={release.channel === "dev"}>
                <button class="button outline release-promote-button" disabled={props.busy} aria-label={`Promote ${release.version} to production`} onClick={() => { setSource(release.version); setVersion(""); setError(""); setNotice(""); }}>
                  <GitBranch size={15} aria-hidden="true"/>Promote to production
                </button>
              </Show>
            </li>}</For>
          </ul>
        </Show>
      </Show>
    </Show>
    <Show when={source()}>
      <form class="review-form release-promotion-form" onSubmit={e => void promote(e)}>
        <div class="release-promotion-heading"><GitBranch size={19} aria-hidden="true"/><div><h4>Promote to production</h4><p>From development version {source()}</p></div></div>
        <p class="muted">Choose a new production version. Honeycomb creates its production archive from this development release.</p>
        <label>Production version
          <input required placeholder="2.4.1" value={version()} disabled={props.busy} onInput={e => setVersion(e.currentTarget.value)} />
        </label>
        <div class="release-actions">
          <button class="button primary" type="submit" disabled={props.busy}>{props.busy ? "Creating production release…" : "Create production release"}</button>
          <button class="button outline" type="button" disabled={props.busy} onClick={() => setSource("")}>Cancel promotion</button>
        </div>
      </form>
    </Show>
  </section>;
}
