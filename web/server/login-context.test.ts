import { test } from "node:test";
import assert from "node:assert/strict";
import { readLoginContext, signLoginContext, identityKind } from "./login-context.ts";
import { isLoginCompletion } from "../src/login-popup.ts";
const key = Buffer.alloc(32, 1);
test("requested identity, popup nonce and return path cannot be changed or replayed for another login", () => {
  const context = { state: "server-state", kind: "carbon" as const, next: "/apps/briefcase", nonce: "11111111-1111-4111-8111-111111111111", expires: Date.now() + 60000 };
  const signed = signLoginContext(context, key);
  assert.deepEqual(readLoginContext(signed, context.state, key), context);
  assert.equal(readLoginContext(signed, "another-state", key), undefined);
  assert.equal(readLoginContext(signed, context.state, Buffer.alloc(32, 2)), undefined);
  const modified = Buffer.from(JSON.stringify({ ...context, kind: "silicon" })).toString("base64url") + "." + signed.split(".")[1];
  assert.equal(readLoginContext(modified, context.state, key), undefined);
  assert.equal(readLoginContext(signLoginContext({ ...context, expires: Date.now() - 1 }, key), context.state, key), undefined);
  assert.equal(readLoginContext(signLoginContext({ ...context, next: "//evil.invalid" }, key), context.state, key)?.next, "/");
  assert.equal(identityKind("other"), undefined);
});
test("popup completion requires the initiating window, app origin, message type and nonce", () => {
  const popup = {} as Window, origin = "https://console.honeycomb.teamofsilicons.com", state = "nonce";
  const event = { source: popup, origin, data: { type: "honeycomb-login-complete", state } };
  assert.equal(isLoginCompletion(event, popup, origin, state), true);
  for (const changed of [{ ...event, origin: "https://evil.invalid" }, { ...event, source: {} as Window }, { ...event, data: { ...event.data, state: "other" } }, { ...event, data: { ...event.data, type: "other" } }]) {
    assert.equal(isLoginCompletion(changed, popup, origin, state), false);
  }
});
