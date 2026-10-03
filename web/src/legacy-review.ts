export type LegacyReview = {
  id: string;
  provider: string;
  app_id: string;
  direction: "received" | "sent";
};
export function legacyRequestId(query: string): string | undefined {
  const values = new URLSearchParams(query).getAll("legacy_request");
  return values.length === 1 && /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(values[0])
    ? values[0] : undefined;
}
export async function resolveLegacyReview(
  query: string,
  read: <T>(path: string) => Promise<T>,
): Promise<LegacyReview | undefined> {
  const id = legacyRequestId(query);
  if (!id) return;
  const result = await read<LegacyReview>(`/api/v1/legacy-review-requests/${encodeURIComponent(id)}`);
  if (!result || !result.id || !result.provider || !result.app_id ||
      !["received", "sent"].includes(result.direction))
    throw new Error("Invalid discussion response");
  return result;
}
