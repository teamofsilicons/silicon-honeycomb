import { createResource, createSignal, For, Show, onCleanup } from "solid-js";
import { ArrowUpRight, RefreshCw, Inbox, Bell, ShieldCheck, CheckCheck } from "lucide-solid";
import { publicationStatus } from "./sent-requests";
import { loadReceivedRequests, type ReceivedPage, type ReceivedRequest } from "./received-requests";
import { SegmentedControl, StatusBadge } from "./ui/Arc";
import "./request-pages.css";

export type { ReceivedRequest } from "./received-requests";
function displayState(item: ReceivedRequest) {
  return ["published", "denied", "superseded"].includes(item.request_state || "")
    ? item.request_state!
    : item.state;
}
function statusTone(state: string) {
  if (["published", "approved", "accepted"].includes(state)) return "success";
  if (["denied", "failed"].includes(state)) return "danger";
  if (["awaiting_activation", "activating"].includes(state)) return "info";
  if (state === "superseded") return "neutral";
  return "warning";
}
export default function ReceivedRequests(props: {
  revision: number;
  open: (item: ReceivedRequest) => void;
}) {
  const [filter, setFilter] = createSignal("all");
  const [cached, setCached] = createSignal<ReceivedPage>();
  let loadRevision = 0;
  const [result, { refetch }] = createResource(
    () => props.revision,
    async () => {
      const revision = ++loadRevision;
      const loaded = await loadReceivedRequests();
      if (revision === loadRevision) setCached(loaded);
      return loaded;
    },
  );
  const latest = () => cached();
  const pendingCount = () => latest()?.items.filter((item) => item.can_decide === true).length || 0;
  const unreadCount = () => latest()?.items.filter((item) => item.unread).length || 0;
  const visible = () =>
    latest()?.items.filter(
      (item) =>
        filter() === "all" ||
        (filter() === "unread" ? item.unread : item.can_decide === true),
    );
  const refresh = () => {
    if (!result.loading) void refetch();
  };
  const timer = setInterval(() => {
    if (document.visibilityState === "visible") refresh();
  }, 30000);
  window.addEventListener("focus", refresh);
  onCleanup(() => {
    loadRevision++;
    clearInterval(timer);
    window.removeEventListener("focus", refresh);
  });
  return (
    <section class="requests-page" aria-label="Received requests">
      <div class="page-heading">
        <div>
          <h1>Received requests</h1>
          <p>
            Review permissions, follow conversations, and see what needs your decision.
          </p>
        </div>
        <button
          class="button outline"
          disabled={result.loading}
          onClick={refresh}
        >
          <RefreshCw size={16} class={result.loading ? "spinning" : ""} />
          Refresh
        </button>
      </div>
      <Show when={latest()}>
        <div class="requests-overview">
          <div class="requests-overview-item" aria-live="polite" aria-atomic="true">
            <span class="requests-overview-icon approval"><ShieldCheck size={18} /></span>
            <span><strong>{pendingCount()}</strong> {pendingCount() === 1 ? "needs" : "need"} your approval</span>
          </div>
          <div class="requests-overview-item" aria-live="polite" aria-atomic="true">
            <span class="requests-overview-icon activity"><Bell size={17} /></span>
            <span><strong>{unreadCount()}</strong> with new activity</span>
          </div>
          <span class="requests-refresh-note">Updates automatically</span>
        </div>
      </Show>
      <div class="requests-toolbar">
        <SegmentedControl
          label="Filter received requests"
          value={filter()}
          onChange={setFilter}
          options={[
            { value: "all", label: <>All requests <span class="request-filter-count">{latest()?.items.length || 0}</span></> },
            { value: "pending", label: <>Needs approval <span class="request-filter-count">{pendingCount()}</span></> },
            { value: "unread", label: <>New activity <span class="request-filter-count">{unreadCount()}</span></> },
          ]}
        />
        <p class="requests-filter-note">New activity includes replies and status changes.</p>
      </div>
      <Show when={result.loading && !latest()}>
        <p class="loading" role="status">
          Loading received requests…
        </p>
      </Show>
      <Show when={result.error}>
        <p class="field-error" role="alert">
          Could not refresh received requests.{" "}
          {String(result.error?.message || "Please try again.")}
          <Show when={latest()}> Your last loaded requests are still shown below.</Show>
        </p>
      </Show>
      <Show when={latest()?.partial}>
        <p class="inline-notice" classList={{ "request-history-notice": latest()?.partialScope === "historical" }} role="status">
          {latest()?.partialScope === "historical"
            ? "Some historical reviews are unavailable. Current requests are up to date."
            : "Some reviewer permissions could not be checked. Refresh to see all available requests."}
        </p>
      </Show>
      <Show
        when={visible()?.length}
        fallback={
          <Show when={!result.loading && !result.error}>
            <div class="request-empty">
              <span class="requests-empty-icon">
                <Show when={filter() === "all"} fallback={<CheckCheck size={24} />}><Inbox size={24} /></Show>
              </span>
              <h2>
                {filter() === "unread"
                  ? "You’re all caught up"
                  : filter() === "pending"
                    ? "No approvals waiting"
                    : "No received requests yet"}
              </h2>
              <p>
                {filter() === "unread"
                  ? "You’ve seen the latest replies and status changes. New updates will appear here."
                  : filter() === "pending"
                    ? "You have no requests that need a decision. You can still follow ongoing reviews in All requests."
                    : "Requests appear here when you can review provider permissions or approve an application for publication."}
              </p>
            </div>
          </Show>
        }
      >
        <div class="request-list" aria-busy={result.loading}>
          <For each={visible()}>
            {(item) => (
              <button
                class={`review-request-row request-card ${item.unread ? "has-update" : ""}`}
                onClick={() => props.open(item)}
              >
                <span class="request-app-mark" aria-hidden="true">{(item.configuration?.name || item.app_id).slice(0, 1).toUpperCase()}</span>
                <div class="received-request-content">
                  <div class="request-title">
                    <h2>{item.configuration?.name || item.app_id}</h2>
                    <Show when={item.unread}>
                      <span class="request-unread-label"><span />New activity</span>
                    </Show>
                  </div>
                  <p class="app-id">{item.app_id}</p>
                  <div class="request-row-metadata">
                    <span>{item.provider}</span>
                    <span class="request-metadata-divider" aria-hidden="true">·</span>
                    <span>
                      {item.scopes?.length
                        ? `${item.scopes.length} requested permission${item.scopes.length === 1 ? "" : "s"}`
                        : "Publication review"}
                    </span>
                    <Show when={item.updated_at}>
                      <span class="request-metadata-divider" aria-hidden="true">·</span>
                      <time dateTime={new Date(item.updated_at * 1000).toISOString()}>
                        Updated {new Date(item.updated_at * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" })}
                      </time>
                    </Show>
                  </div>
                </div>
                <span class="received-request-action">
                  <StatusBadge tone={statusTone(displayState(item))}>
                    {item.can_decide ? "Needs your approval" : publicationStatus(displayState(item))}
                  </StatusBadge>
                  <span class="request-open-label">
                    {item.can_decide ? "Review request" : "View request"}<ArrowUpRight size={15} />
                  </span>
                </span>
              </button>
            )}
          </For>
        </div>
      </Show>
    </section>
  );
}
