import { createResource, createSignal, onCleanup, For, Show } from "solid-js";
import {
  ArrowUpRight,
  RefreshCw,
  ChevronLeft,
  ChevronRight,
  Send,
  CheckCheck,
  MessageSquare,
  Bell,
  Clock3,
} from "lucide-solid";
import {
  loadSentRequests,
  publicationStatus,
  type SentRequest,
  type SentPage,
} from "./sent-requests";
import { StatusBadge } from "./ui/Arc";
import MarkAllRead from "./MarkAllRead";
import { loadAllSentReadTargets } from "./request-read";
import "./request-pages.css";
export { publicationStatus } from "./sent-requests";

function statusTone(state: string) {
  if (["published", "approved", "accepted"].includes(state)) return "success";
  if (["denied", "failed"].includes(state)) return "danger";
  if (["awaiting_activation", "activating"].includes(state)) return "info";
  if (state === "superseded") return "neutral";
  return "warning";
}

export default function SentRequests(props: {
  revision: number;
  unreadCount?: number;
  onRead: () => Promise<void>;
  open: (item: SentRequest) => void;
  discuss: (
    item: { id: string; provider: string },
    request: SentRequest,
  ) => void;
}) {
  const [page, setPage] = createSignal(1);
  const [cached, setCached] = createSignal<SentPage>();
  let loadRevision = 0;
  const [result, { refetch }] = createResource(
    () => ({ revision: props.revision, page: page() }),
    async (value) => {
      const revision = ++loadRevision;
      const loaded = await loadSentRequests(value.page);
      if (revision === loadRevision) setCached(loaded);
      return loaded;
    },
  );
  const latest = () => cached();
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
    <section class="requests-page" aria-label="Sent requests">
      <div class="page-heading">
        <div>
          <h1>Sent requests</h1>
          <p>
            A clear view of your application reviews, replies, and releases.
          </p>
        </div>
        <div class="requests-heading-actions">
        <MarkAllRead view="sent"
          disabled={!latest() || (!props.unreadCount && !latest()?.items.some(item => item.unread))}
          load={loadAllSentReadTargets}
          refresh={async () => { await Promise.all([refetch(), props.onRead()]); }} />
        <button
          class="button outline"
          disabled={result.loading}
          onClick={refresh}
        >
          <RefreshCw size={16} class={result.loading ? "spinning" : ""} />
          Refresh
        </button>
        </div>
      </div>
      <Show when={latest()}>
        <div class="requests-overview">
          <div class="requests-overview-item">
            <span class="requests-overview-icon"><Send size={17} /></span>
            <span><strong>{latest()?.total || 0}</strong> request{latest()?.total === 1 ? "" : "s"} sent</span>
          </div>
          <div class="requests-overview-item" aria-live="polite" aria-atomic="true">
            <span class="requests-overview-icon activity"><Bell size={17} /></span>
            <span><strong>{latest()?.items.filter((item) => item.unread).length || 0}</strong> new update{latest()?.items.filter((item) => item.unread).length === 1 ? "" : "s"} on this page</span>
          </div>
          <span class="requests-refresh-note">Updates automatically</span>
        </div>
      </Show>
      <Show when={result.loading && !latest()}>
        <p role="status" class="loading">
          Loading sent requests…
        </p>
      </Show>
      <Show when={result.error}>
        <p class="field-error" role="alert">
          Could not refresh sent requests.{" "}
          {String(result.error?.message || "Please try again.")}
          <Show when={latest()}> Your last loaded requests are still shown below.</Show>
        </p>
      </Show>
      <Show when={latest()?.partial}>
        <p class="inline-notice" role="status">
          Some applications could not be checked. Refresh to see their requests.
        </p>
      </Show>
      <Show
        when={latest()?.items.length}
        fallback={
          <Show when={!result.loading && !result.error}>
            <div class="request-empty">
              <span class="requests-empty-icon"><Send size={24} /></span>
              <h2>No requests sent yet</h2>
              <p>
                Create an application and upload its first production release.
                Public approval begins when both are ready.
              </p>
            </div>
          </Show>
        }
      >
        <div class="request-list" aria-busy={result.loading}>
          <For each={latest()?.items}>
            {(item) => (
              <article
                class={`sent-request request-card ${item.unread ? "has-update" : ""}`}
              >
                <div class="sent-request-heading">
                  <div class="request-identity">
                    <span class="request-app-mark" aria-hidden="true">{item.app.name.slice(0, 1).toUpperCase()}</span>
                    <div>
                      <div class="request-title">
                        <h2>{item.app.name}</h2>
                        <Show when={item.unread}>
                          <span class="request-unread-label"><span />New update</span>
                        </Show>
                      </div>
                      <p class="app-id">{item.app.app_id} <span aria-hidden="true">/</span> Revision {item.revision}</p>
                    </div>
                  </div>
                  <StatusBadge tone={statusTone(item.state)}>{publicationStatus(item.state)}</StatusBadge>
                </div>
                <Show when={item.updated_at}>
                  <p class="request-timestamp">
                    Updated {new Date(item.updated_at * 1000).toLocaleString()}
                  </p>
                </Show>
                <Show when={item.revision !== item.app.revision}>
                  <p class="muted">
                    An earlier revision. The application is now on revision{" "}
                    {item.app.revision}.
                  </p>
                </Show>
                <Show
                  when={["awaiting_activation", "activating"].includes(
                    item.state,
                  )}
                >
                  <p class="muted">
                    Approvals are complete. Your release is being published.
                  </p>
                </Show>
                <Show when={item.error || item.activation?.error}>
                  <p class="field-error" role="alert">
                    {item.error || item.activation?.error}
                  </p>
                </Show>
                <Show when={item.gates.length}>
                  <div class="request-gates">
                    <div class="request-section-label">Approval progress</div>
                    <For each={item.gates}>
                      {(gate) => (
                        <div class="sent-request-gate">
                          <div class="request-gate-detail">
                            <span class="request-gate-icon" aria-hidden="true">
                              <Show when={["approved", "accepted"].includes(gate.state)} fallback={<Clock3 size={16} />}>
                                <CheckCheck size={16} />
                              </Show>
                            </span>
                            <span class="request-provider">{gate.provider}</span>
                            <StatusBadge tone={statusTone(gate.state)}>{publicationStatus(gate.state)}</StatusBadge>
                          </div>
                          <button
                            class="text-button request-discussion-link"
                            onClick={() =>
                              props.discuss(
                                { id: item.id, provider: gate.provider },
                                item,
                              )
                            }
                          >
                            <MessageSquare size={14} />
                            Discussion
                            <span class="sr-only"> with {gate.provider}</span>
                          </button>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
                <div class="request-card-footer">
                  <span class="request-footer-note">
                    <Show when={item.unread} fallback="View the release and its review history.">New activity is ready to review.</Show>
                  </span>
                  <button class="button outline" onClick={() => props.open(item)}>
                    View publication
                    <ArrowUpRight size={15} />
                  </button>
                </div>
              </article>
            )}
          </For>
        </div>
      </Show>
      <Show when={(latest()?.total || 0) > 20}>
        <nav class="pagination" aria-label="Sent requests pages">
          <button
            class="button outline"
            disabled={result.loading || latest()?.page === 1}
            onClick={() => setPage((latest()?.page || 1) - 1)}
          >
            <ChevronLeft size={16} />
            Previous
          </button>
          <span>
            Page {latest()?.page} of {Math.ceil((latest()?.total || 0) / 20)}
          </span>
          <button
            class="button outline"
            disabled={
              result.loading ||
              (latest()?.page || 1) * 20 >= (latest()?.total || 0)
            }
            onClick={() => setPage((latest()?.page || 1) + 1)}
          >
            Next
            <ChevronRight size={16} />
          </button>
        </nav>
      </Show>
    </section>
  );
}
