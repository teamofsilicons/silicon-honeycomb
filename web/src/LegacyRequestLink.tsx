import { createEffect, createResource, Show } from "solid-js";
import { ApiError, request } from "./api";
import { legacyRequestId, resolveLegacyReview, type LegacyReview } from "./legacy-review";

export default function LegacyRequestLink(props: {
  query: string;
  open: (review: LegacyReview) => void;
}) {
  const [result, { refetch }] = createResource(
    () => props.query,
    async (query): Promise<{ review?: LegacyReview; error?: unknown }> => {
      try { return { review: await resolveLegacyReview(query, request) }; }
      catch (error) { return { error }; }
    },
  );
  let opened = "";
  createEffect(() => {
    const review = result()?.review;
    if (!review) return;
    const key = `${review.id}:${review.provider}:${review.direction}`;
    if (key !== opened) {
      opened = key;
      props.open(review);
    }
  });
  const unavailable = () => {
    const error = result()?.error;
    return !legacyRequestId(props.query) ||
      (error instanceof ApiError && [403, 404].includes(error.status || 0));
  };
  return <section class="empty">
    <Show when={!result.loading && !result()?.review} fallback={<p role="status">Opening your discussion…</p>}>
      <h1>Discussion unavailable</h1>
      <p>{unavailable()
        ? "This request is unavailable to this account. Open your requests, or sign in with the account that received the email."
        : "We couldn’t open this discussion right now. Please try again."}</p>
      <div class="actions">
        <a class="button primary" href="/requests/received">Open requests</a>
        <Show when={!unavailable()}><button class="button outline" onClick={() => void refetch()}>Try again</button></Show>
      </div>
    </Show>
  </section>;
}
