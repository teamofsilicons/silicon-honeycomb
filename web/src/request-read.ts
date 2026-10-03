import { request } from "./api";
import { markRequestRead } from "./sent-requests";

export type ReadTarget = {
  id: string;
  activity_version?: string;
  provider?: string;
  unread: boolean;
};
export type ReadBatch = { items: ReadTarget[]; partial?: boolean };

const fingerprint = (value: unknown): value is string =>
  typeof value === "string" && /^[a-f\d]{64}$/i.test(value);

export async function loadAllSentReadTargets(
  read: typeof request = request,
): Promise<ReadBatch> {
  const items = new Map<string, ReadTarget>();
  let partial = false;
  let pages = 1;
  let initialTotal: number | undefined;
  for (let page = 1; page <= pages; page++) {
    let loaded: {
      items?: unknown;
      page?: number;
      per_page?: number;
      total?: number;
      partial?: boolean;
    };
    try {
      loaded = await read(`/api/v1/sent-requests?page=${page}&per_page=100`);
    } catch (error) {
      partial = true;
      if (error instanceof Error && error.name === "AbortError") break;
      continue;
    }
    if (
      !loaded ||
      loaded.page !== page ||
      loaded.per_page !== 100 ||
      !Number.isSafeInteger(loaded.total) ||
      loaded.total! < 0 ||
      !Array.isArray(loaded.items) ||
      loaded.items.length > 100
    ) {
      partial = true;
      continue;
    }
    const total = loaded.total!;
    if (initialTotal === undefined) {
      initialTotal = total;
      // Bound this action to the first observed pagination window. New arrivals
      // must not extend a user's acknowledgement indefinitely.
      pages = Math.max(1, Math.ceil(total / 100));
    }
    if (
      total !== initialTotal ||
      loaded.partial === true ||
      loaded.items.length !== Math.min(100, Math.max(0, total - (page - 1) * 100))
    ) partial = true;
    for (const value of loaded.items) {
      if (
        !value || typeof value !== "object" ||
        typeof value.id !== "string" || !value.id ||
        typeof value.unread !== "boolean"
      ) {
        partial = true;
        continue;
      }
      if (items.has(value.id)) {
        // Activity can reorder pages while loading. Keep the earliest observed
        // version, so a later response cannot acknowledge an unseen update.
        partial = true;
        continue;
      }
      if (!fingerprint(value.activity_version)) partial = true;
      items.set(value.id, {
        id: value.id,
        unread: value.unread,
        activity_version: typeof value.activity_version === "string"
          ? value.activity_version : undefined,
      });
    }
  }
  if (initialTotal === undefined || items.size !== initialTotal) partial = true;
  return { items: [...items.values()], partial };
}

export async function markRequestsRead(
  items: ReadTarget[],
  view: "sent" | "received",
  onProgress?: (completed: number, total: number) => void,
  read: typeof request = request,
): Promise<{ marked: number; failed: number }> {
  const unique = new Map<string, ReadTarget>();
  for (const item of items) {
    if (!item.unread) continue;
    const key = JSON.stringify([item.id, view === "received" ? item.provider : ""]);
    if (!unique.has(key)) unique.set(key, {
      id: item.id,
      activity_version: item.activity_version,
      provider: item.provider,
      unread: true,
    });
  }
  const snapshot = [...unique.values()];
  let next = 0, completed = 0, marked = 0, failed = 0;
  const report = () => {
    // A presentation callback must not interrupt other saved acknowledgements.
    try { onProgress?.(completed, snapshot.length); } catch { /* keep processing */ }
  };
  report();
  const worker = async () => {
    while (next < snapshot.length) {
      const item = snapshot[next++];
      try {
        if (
          !item.id || !fingerprint(item.activity_version) ||
          (view === "received" && (!item.provider || item.provider.length > 128))
        ) throw new Error("The request has no valid read marker.");
        await markRequestRead(item, view, read);
        marked++;
      } catch {
        failed++;
      }
      completed++;
      report();
    }
  };
  await Promise.all(Array.from({ length: Math.min(4, snapshot.length) }, worker));
  return { marked, failed };
}
