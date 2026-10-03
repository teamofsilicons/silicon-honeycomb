import { request } from "./api";
import type { RequestActivity } from "./sent-requests";

export type ReceivedRequest = RequestActivity & {
  id: string;
  provider: string;
  app_id: string;
  state: string;
  request_state?: string;
  can_decide?: boolean;
  configuration?: { name?: string };
  scopes: string[];
};

type ReceivedResponse = {
  items: ReceivedRequest[];
  partial?: boolean;
};

export type ReceivedPage = ReceivedResponse & {
  partialScope?: "historical" | "current";
};

export async function loadReceivedRequests(
  read: typeof request = request,
): Promise<ReceivedPage> {
  const loaded = await read<ReceivedResponse>(
    "/api/v1/review-requests?include_completed=true",
  );
  if (loaded.partial !== true) return loaded;

  try {
    const current = await read<ReceivedResponse>("/api/v1/review-requests");
    if (current.partial === false && Array.isArray(current.items)) {
      return { ...loaded, partialScope: "historical" };
    }
  } catch {
    // Keep the original warning and loaded requests if current access is unknown.
  }
  return { ...loaded, partialScope: "current" };
}
