import assert from "node:assert/strict";
import test from "node:test";
import { resolveLegacyReview, type LegacyReview } from "./legacy-review.ts";
const oldId = "7badb5d6-6451-4e58-a7ed-c202401f9658";
test("old email ID resolves to its provider discussion and authorized direction", async () => {
  for (const direction of ["sent", "received"] as const) {
    const expected: LegacyReview = {id:"new-request",provider:"ting",app_id:"ring",direction};
    const result = await resolveLegacyReview(`?legacy_request=${oldId}`, async <T>(path: string) => {
      assert.equal(path, `/api/v1/legacy-review-requests/${oldId}`);
      return expected as T;
    });
    assert.deepEqual(result, expected);
  }
});
test("invalid or duplicate legacy identifiers make no API request", async () => {
  for (const query of ["", "legacy_request=https://outside.example", `legacy_request=${oldId}&legacy_request=${oldId}`])
    assert.equal(await resolveLegacyReview(query, async () => {throw new Error("Unexpected request");}), undefined);
});
test("unavailable legacy reviews preserve API failure instead of opening a different thread", async () => {
  await assert.rejects(resolveLegacyReview(`legacy_request=${oldId}`, async () => {throw new Error("not_found");}), /not_found/);
});
