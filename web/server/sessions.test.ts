import { test } from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Sessions } from "./sessions.ts";

const key = Buffer.alloc(32, 7);
const tokens = (suffix = "new", expires_in = 1800) => ({ access_token: `access-${suffix}`, refresh_token: `refresh-${suffix}`, expires_in });
function fixture(t: any, refresh: (token: string, key: string) => Promise<Response>) {
  const directory = mkdtempSync(join(tmpdir(), "honeycomb-session-test-"));
  const databases: DatabaseSync[] = [];
  const open = (handler = refresh) => {
    const db = new DatabaseSync(join(directory, "sessions.db")); databases.push(db);
    return new Sessions(db, key, handler);
  };
  t.after(() => { databases.forEach(db => db.close()); rmSync(directory, { recursive: true }); });
  const sessions = open();
  sessions.create("session", tokens("old"), Date.now() - 31 * 60000, Date.now() + 86400000);
  return { sessions, open };
}

test("concurrent requests renew once and the rotated credentials survive a restart", async t => {
  let calls = 0;
  const { sessions, open } = fixture(t, async () => { calls++; await new Promise(resolve => setTimeout(resolve, 20)); return Response.json(tokens()); });
  const results = await Promise.all(Array.from({ length: 12 }, () => sessions.current("session")));
  assert.equal(calls, 1);
  assert.ok(results.every(result => result?.tokens.access_token === "access-new"));
  assert.equal((await open().current("session"))?.tokens.refresh_token, "refresh-new");
  assert.equal(calls, 1);
});

test("lost refresh response retries the same key after reopening the database", async t => {
  const attempts: string[] = [];
  const { sessions, open } = fixture(t, async (token, operation) => {
    assert.equal(token, "refresh-old"); attempts.push(operation);
    if (attempts.length === 1) throw new Error("connection reset after rotation");
    assert.equal(operation, attempts[0]);
    return Response.json(tokens());
  });
  await assert.rejects(sessions.current("session"), /connection reset/);
  assert.ok(sessions.load("session")?.tokens.pending_refresh);
  assert.equal((await open().current("session"))?.tokens.access_token, "access-new");
  assert.equal(attempts.length, 2);
  assert.equal(new Set(attempts).size, 1);
});

test("separate database connections share one durable refresh operation and cannot overwrite successors", async t => {
  const attempts: string[] = [];
  let release!: () => void;
  const waiting = new Promise<void>(resolve => { release = resolve; });
  const { sessions, open } = fixture(t, async (_, operation) => {
    attempts.push(operation);
    await waiting;
    return Response.json(tokens());
  });
  const first = sessions.current("session");
  const second = open().current("session");
  assert.equal(attempts.length, 2);
  assert.equal(new Set(attempts).size, 1);
  release();
  await Promise.all([first, second]);
  assert.equal(sessions.load("session")?.tokens.refresh_token, "refresh-new");
});

test("outages and app authentication errors preserve the saved session and retry key", async t => {
  let status = 503;
  const keys: string[] = [];
  const { sessions } = fixture(t, async (_, operation) => { keys.push(operation); return Response.json({ error: { code: "service_unavailable" } }, { status }); });
  for (status of [503, 429, 400, 401, 403]) {
    await assert.rejects(sessions.current("session"), /could not renew/);
    assert.equal(sessions.load("session")?.tokens.refresh_token, "refresh-old");
  }
  assert.equal(new Set(keys).size, 1);
});

test("an explicit invalid grant removes the revoked family", async t => {
  const { sessions } = fixture(t, async () => Response.json({ error: { code: "invalid_grant" } }, { status: 401 }));
  assert.equal(await sessions.current("session"), undefined);
  assert.equal(sessions.load("session"), undefined);
});

test("an in-flight refresh cannot recreate a signed out session", async t => {
  let release!: () => void;
  const wait = new Promise<void>(resolve => { release = resolve; });
  const { sessions, open } = fixture(t, async () => { await wait; return Response.json(tokens()); });
  const refresh = sessions.current("session");
  open().remove("session"); release();
  assert.equal(await refresh, undefined);
});

test("a stale access token gets one renewal even before its advertised expiry", async t => {
  let calls = 0;
  const { sessions } = fixture(t, async () => { calls++; return Response.json(tokens()); });
  sessions.create("fresh", tokens("old"), Date.now(), Date.now() + 86400000);
  assert.equal((await sessions.current("fresh", "access-old"))?.tokens.access_token, "access-new");
  assert.equal((await sessions.current("fresh", "access-old"))?.tokens.access_token, "access-new");
  assert.equal(calls, 1);
});

test("malformed responses preserve the refresh operation for safe retry", async t => {
  const { sessions, open } = fixture(t, async () => Response.json({ access_token: "new", expires_in: -1 }));
  await assert.rejects(sessions.current("session"), /invalid session response/);
  const pending = sessions.load("session")?.tokens.pending_refresh;
  assert.ok(pending);
  assert.equal(open().load("session")?.tokens.pending_refresh?.key, pending.key);
});

test("expiry is measured from the saved request start, including response-loss recovery", async t => {
  let calls = 0;
  const { sessions, open } = fixture(t, async (_, operation) => {
    calls++;
    if (calls === 1) throw new Error("lost response");
    return Response.json(tokens(calls === 2 ? "replayed" : "renewed"));
  });
  await assert.rejects(sessions.current("session"));
  const savedNow = Date.now;
  const start = savedNow();
  // Simulate a restart more than an access lifetime after the uncertain rotation.
  Date.now = () => start + 31 * 60000;
  try {
    const result = await open().current("session");
    assert.equal(result?.tokens.access_token, "access-renewed");
    assert.equal(calls, 3);
    assert.ok(result!.tokens.expires_at <= Date.now() + 1800000);
  } finally { Date.now = savedNow; }
});
