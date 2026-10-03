import assert from "node:assert/strict";
import test from "node:test";
import type { request } from "./api";
import { loadReceivedRequests } from "./received-requests";

const history = {
  items: [
    { id: "archived", provider: "tos>honeycomb", can_decide: false },
    { id: "current", provider: "honeycomb", can_decide: true },
  ],
  partial: true,
};

test("historical-only failures are distinguished after current requests are verified complete", async () => {
  const calls: string[] = [];
  const read = (async (path: string) => {
    calls.push(path);
    return calls.length === 1
      ? history
      : { items: [history.items[1]], partial: false };
  }) as typeof request;
  const page = await loadReceivedRequests(read);
  assert.deepEqual(calls, [
    "/api/v1/review-requests?include_completed=true",
    "/api/v1/review-requests",
  ]);
  assert.equal(page.partialScope, "historical");
  assert.equal(page.partial, true);
  // Diagnostics must not rewrite provider evidence or decision authority.
  assert.equal(page.items, history.items);
  assert.deepEqual(page.items, history.items);
});

test("a partial current inbox retains the reviewer warning", async () => {
  let calls = 0;
  const read = (async () =>
    ++calls === 1 ? history : { items: [], partial: true }) as typeof request;
  const page = await loadReceivedRequests(read);
  assert.equal(page.partialScope, "current");
  assert.equal(page.items, history.items);
});

test("a failed secondary check retains loaded requests and the reviewer warning", async () => {
  let calls = 0;
  const read = (async () => {
    if (++calls === 1) return history;
    throw new Error("Reviewer service unavailable");
  }) as typeof request;
  const page = await loadReceivedRequests(read);
  assert.equal(page.partialScope, "current");
  assert.equal(page.items, history.items);
});

test("missing completeness or malformed current items never claim current requests are up to date", async () => {
  for (const response of [{ items: [] }, { partial: false }]) {
    let calls = 0;
    const read = (async () =>
      ++calls === 1 ? history : response) as typeof request;
    const page = await loadReceivedRequests(read);
    assert.equal(page.partialScope, "current");
  }
});

test("a complete history response makes no extra request", async () => {
  let calls = 0;
  const complete = { ...history, partial: false };
  const read = (async () => {
    calls++;
    return complete;
  }) as typeof request;
  assert.equal(await loadReceivedRequests(read), complete);
  assert.equal(calls, 1);
});

test("the primary request failure still reaches the existing refresh error state", async () => {
  let calls = 0;
  const read = (async () => {
    calls++;
    throw new Error("Refresh unavailable");
  }) as typeof request;
  await assert.rejects(loadReceivedRequests(read), /Refresh unavailable/);
  assert.equal(calls, 1);
});
