import { createResource, For, Show } from "solid-js";
import { ArrowUpRight } from "lucide-solid";
import { request, endpoint, type AppRecord } from "./api";
import type { ReceivedRequest } from "./ReceivedRequests";
export default function RequestLink(props: {
  requestId: string;
  appId: string;
  canManage: (app: AppRecord) => boolean;
  received: (item: ReceivedRequest) => void;
  sent: (app: AppRecord) => void;
}) {
  const [result, { refetch }] = createResource(
    () => [props.requestId, props.appId],
    async ([id, appId]) => {
      const [inbox, app] = await Promise.allSettled([
        request<{ items: ReceivedRequest[] }>(
          "/api/v1/review-requests?include_completed=true",
        ),
        appId
          ? request<AppRecord>(endpoint(appId))
          : Promise.resolve(undefined),
      ]);
      return {
        received:
          inbox.status === "fulfilled"
            ? inbox.value.items.filter((item) => item.id === id)
            : [],
        app:
          app.status === "fulfilled" && app.value && props.canManage(app.value)
            ? app.value
            : undefined,
        failed: inbox.status === "rejected",
      };
    },
  );
  return (
    <section>
      <div class="page-heading">
        <div>
          <span class="eyebrow">REQUEST ACTIVITY</span>
          <h1>Open your request.</h1>
          <p>Choose the discussion or publication you’d like to review.</p>
        </div>
      </div>
      <Show when={result.loading}>
        <p role="status">Finding your request…</p>
      </Show>
      <For each={result()?.received}>
        {(item) => (
          <button
            class="review-request-row"
            onClick={() => props.received(item)}
          >
            <div>
              <h2>{item.configuration?.name || item.app_id}</h2>
              <p>Received request · {item.provider}</p>
            </div>
            <ArrowUpRight size={18} />
          </button>
        )}
      </For>
      <Show when={result()?.app}>
        {(app) => (
          <button class="review-request-row" onClick={() => props.sent(app())}>
            <div>
              <h2>{app().name}</h2>
              <p>Sent request · Publication and approval progress</p>
            </div>
            <ArrowUpRight size={18} />
          </button>
        )}
      </Show>
      <Show
        when={!result.loading && !result()?.received.length && !result()?.app}
      >
        <p class="inline-notice">
          {result()?.failed
            ? "We couldn’t check all your review permissions. Please try again."
            : "This account has no available discussion for this request. Sign in with the account that received the notification, or open your request inbox."}
        </p>
        <button class="button outline" onClick={() => void refetch()}>
          Try again
        </button>
      </Show>
    </section>
  );
}
