import { request, type AppRecord } from "./api";
export type RequestActivity = {
  activity_version: string;
  unread: boolean;
  updated_at: number;
  message_count?: number;
};
export type SentRequest = RequestActivity & {
  id: string;
  revision: number;
  state: string;
  error?: string | null;
  gates: { provider: string; state: string }[];
  activation?: { error?: string | null } | null;
  app: AppRecord;
};
export type SentPage = {
  items: SentRequest[];
  total: number;
  page: number;
  per_page: number;
  partial?: boolean;
};
export async function loadSentRequests(
  page = 1,
  read: typeof request = request,
): Promise<SentPage> {
  if (!Number.isInteger(page) || page < 1)
    throw new Error("Request page must be a positive integer.");
  return read<SentPage>(`/api/v1/sent-requests?page=${page}&per_page=20`);
}
export async function markRequestRead(
  item: { id: string; activity_version?: string; provider?: string },
  view: "sent" | "received",
  read: typeof request = request,
) {
  if (!item.activity_version) return;
  await read(`/api/v1/requests/${encodeURIComponent(item.id)}/read`, {
    method: "POST",
    body: JSON.stringify({
      view,
      activity_version: item.activity_version,
      ...(view === "received" ? { provider: item.provider } : {}),
    }),
  });
}
export function publicationStatus(state: string) {
  return (
    (
      {
        awaiting_review_plan: "Preparing review",
        awaiting_scope_review: "Awaiting scope approval",
        awaiting_validator: "Awaiting Honeycomb approval",
        awaiting_activation: "Ready to publish",
        activating: "Publishing",
        published: "Published",
        denied: "Denied",
        pending: "Awaiting review",
        approved: "Approved",
        accepted: "Approved",
      } as Record<string, string>
    )[state] || state.replaceAll("_", " ")
  );
}
