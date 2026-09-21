import { endpoint, request, type AppRecord } from "./api";

export type SentRequest = {
  id: string;
  revision: number;
  state: string;
  error?: string | null;
  gates: { provider: string; state: string }[];
  activation?: { error?: string | null } | null;
  app: AppRecord;
};

export async function loadSentRequests(read: typeof request = request): Promise<SentRequest[]> {
  const apps = new Map<string, AppRecord>();
  for (let page = 1; ; page++) {
    const result = await read<{ items: AppRecord[]; total: number }>(`/api/v1/apps?managed=true&page=${page}`);
    const previous = apps.size;
    for (const app of result.items) apps.set(app.app_id, app);
    if (apps.size >= result.total) break;
    if (apps.size === previous) throw new Error("The application list changed while loading. Refresh sent requests to try again.");
  }
  const managed = [...apps.values()];
  const items: SentRequest[] = [];
  // Bound concurrent history reads; each endpoint checks current manager authority.
  for (let index = 0; index < managed.length; index += 4) {
    const results = await Promise.all(managed.slice(index, index + 4).map(async app => {
      const result = await read<{ items: Omit<SentRequest, "app">[] }>(endpoint(app.app_id) + "/publication");
      return result.items.map(item => ({ ...item, app }));
    }));
    items.push(...results.flat());
  }
  return items.sort((a, b) => a.app.name.localeCompare(b.app.name) || b.revision - a.revision);
}

export function publicationStatus(state: string) {
  return ({ awaiting_review_plan: "Preparing review", awaiting_scope_review: "Awaiting scope approval", awaiting_validator: "Awaiting Honeycomb approval", awaiting_activation: "Ready to publish", activating: "Publishing", published: "Published", denied: "Denied" } as Record<string, string>)[state] || state.replaceAll("_", " ");
}

