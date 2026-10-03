import assert from "node:assert/strict";
import test from "node:test";
import type { request } from "./api";
import { loadAllSentReadTargets, markRequestsRead, type ReadTarget } from "./request-read";

const version = "a".repeat(64);
const target = (id: string, extra: Partial<ReadTarget> = {}): ReadTarget =>
  ({ id, activity_version: version, unread: true, ...extra });

test("bulk reads preserve unread snapshots and isolate provider markers without decision permission", async () => {
  const items = [
    target("same/id", { provider: "iam" }),
    target("same/id", { provider: "ting" }),
    target("same/id", { provider: "iam", activity_version: "b".repeat(64) }),
    target("already-read", { provider: "iam", unread: false }),
  ];
  const calls: { path: string; body: unknown }[] = [];
  const read = (async (path: string, options: RequestInit) => {
    calls.push({ path, body: JSON.parse(String(options.body)) });
    items[1].activity_version = "c".repeat(64);
    return { read: true };
  }) as typeof request;
  assert.deepEqual(await markRequestsRead(items, "received", undefined, read), { marked: 2, failed: 0 });
  assert.deepEqual(calls, [
    { path: "/api/v1/requests/same%2Fid/read", body: { view: "received", activity_version: version, provider: "iam" } },
    { path: "/api/v1/requests/same%2Fid/read", body: { view: "received", activity_version: version, provider: "ting" } },
  ]);
});

test("bulk reads keep successful writes, cap concurrent requests at four, and report completion", async () => {
  let running = 0, maximum = 0;
  const releases: (() => void)[] = [];
  const progress: [number, number][] = [];
  const read = (async (path: string, options: RequestInit) => {
    running++; maximum = Math.max(maximum, running);
    assert.equal(JSON.parse(String(options.body)).provider, undefined);
    await new Promise<void>((resolve) => releases.push(resolve));
    running--;
    if (path.includes("/fail/")) throw new Error("No longer authorized");
    return { read: true };
  }) as typeof request;
  const done = markRequestsRead(
    [target("one"), target("two"), target("fail"), target("four"), target("five"), target("missing", { activity_version: undefined })],
    "sent", (completed, total) => progress.push([completed, total]), read,
  );
  assert.equal(running, 4);
  while (releases.length) {
    releases.shift()!();
    await new Promise<void>((resolve) => setImmediate(resolve));
  }
  assert.deepEqual(await done, { marked: 4, failed: 2 });
  assert.equal(maximum, 4);
  assert.deepEqual(progress, Array.from({ length: 7 }, (_, completed) => [completed, 6]));
});

test("invalid received markers fail without writes and an empty batch is harmless", async () => {
  let calls = 0;
  const read = (async () => { calls++; return {}; }) as typeof request;
  assert.deepEqual(await markRequestsRead([
    target("missing-provider"),
    target("missing-version", { provider: "iam", activity_version: undefined }),
    target("invalid-version", { provider: "iam", activity_version: "newest" }),
  ], "received", undefined, read), { marked: 0, failed: 3 });
  assert.deepEqual(await markRequestsRead([target("read", { unread: false })], "sent", undefined, read), { marked: 0, failed: 0 });
  assert.equal(calls, 0);
});

test("all sent pages load at 100 per page and retain read flags", async () => {
  const paths: string[] = [];
  const read = (async (path: string) => {
    paths.push(path);
    const page = paths.length;
    return { page, per_page: 100, total: 102, partial: false,
      items: page === 1 ? Array.from({ length: 100 }, (_, i) => target(String(i))) : [target("100"), target("101", { unread: false })] };
  }) as typeof request;
  const result = await loadAllSentReadTargets(read);
  assert.deepEqual(paths, ["/api/v1/sent-requests?page=1&per_page=100", "/api/v1/sent-requests?page=2&per_page=100"]);
  assert.equal(result.items.length, 102);
  assert.equal(result.items[101].unread, false);
  assert.equal(result.partial, false);
});

test("reordered sent pages keep the first fingerprint and report incomplete coverage", async () => {
  let page = 0;
  const read = (async () => ({ page: ++page, per_page: 100, total: 101,
    items: page === 1 ? Array.from({ length: 100 }, (_, i) => target(String(i))) : [target("0", { activity_version: "b".repeat(64) })] })) as typeof request;
  const result = await loadAllSentReadTargets(read);
  assert.equal(result.items.length, 100);
  assert.equal(result.items[0].activity_version, version);
  assert.equal(result.partial, true);
});

test("inaccessible sent pages preserve accessible pages and partial responses stay partial", async () => {
  let page = 0;
  const read = (async () => {
    page++;
    if (page === 2) throw new Error("Unavailable");
    return { page, per_page: 100, total: 201, partial: page === 3,
      items: page === 1 ? Array.from({ length: 100 }, (_, i) => target(String(i))) : [target("200")] };
  }) as typeof request;
  const result = await loadAllSentReadTargets(read);
  assert.equal(page, 3);
  assert.equal(result.items.length, 101);
  assert.equal(result.partial, true);
});

test("malformed pagination, missing rows and fingerprints are never reported complete", async () => {
  for (const response of [
    { page: 2, per_page: 100, total: 0, items: [] },
    { page: 1, per_page: 20, total: 0, items: [] },
    { page: 1, per_page: 100, total: -1, items: [] },
    { page: 1, per_page: 100, total: 1, items: [] },
    { page: 1, per_page: 100, total: 1, items: [{ id: "missing", unread: true }] },
    { page: 1, per_page: 100, total: 1, items: [{ id: "invalid", unread: "yes" }] },
  ]) {
    const read = (async () => response) as typeof request;
    assert.equal((await loadAllSentReadTargets(read)).partial, true);
  }
  const empty = (async () => ({ page: 1, per_page: 100, total: 0, items: [], partial: false })) as typeof request;
  assert.deepEqual(await loadAllSentReadTargets(empty), { items: [], partial: false });
});

test("cancelled pagination stops scheduling pages and keeps its partial snapshot", async () => {
  let calls = 0;
  const read = (async () => {
    if (++calls === 2) throw new DOMException("Account or page changed", "AbortError");
    return { page: 1, per_page: 100, total: 301, items: Array.from({ length: 100 }, (_, i) => target(String(i))) };
  }) as typeof request;
  const result = await loadAllSentReadTargets(read);
  assert.equal(calls, 2);
  assert.equal(result.items.length, 100);
  assert.equal(result.partial, true);
});
