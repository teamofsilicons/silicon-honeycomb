import assert from "node:assert/strict";
import test from "node:test";
import { loadSentRequests, markRequestRead } from "./sent-requests";
import type { request } from "./api";
import { applicationPath, parseApplicationRoute } from "./application-route";

test("sent requests use one paginated account-scoped aggregate read including completed history", async () => {
  const calls: string[] = [];
  const read = (async (path: string) => {
    calls.push(path);
    return {
      items: [
        { id: "published", state: "published" },
        { id: "pending", state: "awaiting_validator" },
      ],
      total: 23,
      page: 2,
      per_page: 20,
    };
  }) as typeof request;
  const page = await loadSentRequests(2, read);
  assert.deepEqual(calls, ["/api/v1/sent-requests?page=2&per_page=20"]);
  assert.equal(page.total, 23);
  assert.deepEqual(
    page.items.map((item) => item.state),
    ["published", "awaiting_validator"],
  );
});
test("request failures propagate and invalid pages never reach the API", async () => {
  const read = (async () => {
    throw Error("Service temporarily unavailable");
  }) as typeof request;
  await assert.rejects(loadSentRequests(1, read), /temporarily unavailable/);
  await assert.rejects(loadSentRequests(0, read), /positive integer/);
});
test("read markers use the exact viewed fingerprint and isolate received provider context", async () => {
  const calls: { path: string; body: unknown }[] = [];
  const read = (async (path: string, options: RequestInit) => {
    calls.push({ path, body: JSON.parse(String(options.body)) });
    return {};
  }) as typeof request;
  const activity_version = "a".repeat(64);
  await markRequestRead(
    { id: "request-one", provider: "briefcase", activity_version },
    "received",
    read,
  );
  await markRequestRead(
    { id: "request-one", provider: "briefcase", activity_version },
    "sent",
    read,
  );
  await markRequestRead({ id: "missing-fingerprint" }, "sent", read);
  assert.deepEqual(calls, [
    {
      path: "/api/v1/requests/request-one/read",
      body: { view: "received", activity_version, provider: "briefcase" },
    },
    {
      path: "/api/v1/requests/request-one/read",
      body: { view: "sent", activity_version },
    },
  ]);
});
test("application URLs distinguish exact production and development releases", () => {
  for (const channel of ["prod", "dev"] as const) {
    const route = applicationPath("briefcase", channel, "2.1.0");
    assert.deepEqual(parseApplicationRoute(route), {
      appId: "briefcase",
      channel,
      version: "2.1.0",
    });
  }
  assert.deepEqual(parseApplicationRoute(applicationPath("briefcase")), {
    appId: "briefcase",
  });
  for (const path of [
    "/apps/%ZZ",
    "/apps/a%2Fb",
    "/apps/briefcase/releases/prod/latest",
    "/apps/briefcase/releases/beta/2.1.0",
    "/apps/briefcase/unknown",
  ])
    assert.equal(parseApplicationRoute(path), undefined);
});

import {
  trustedConsentUrl,
  isStorageCompletion,
} from "./storage-authorization";
test("storage consent completion is bound to the initiating window, origin, request, and state", () => {
  const popup = {} as Window,
    authorization = {
      authorization_id: "request",
      state: "opaque-state",
      consent_url: "https://iam.teamofsilicons.com/obo/consent?request=id",
    };
  const event = {
    source: popup,
    origin: "https://console.honeycomb.teamofsilicons.com",
    data: {
      type: "honeycomb-storage-authorized",
      authorization_id: "request",
      state: "opaque-state",
    },
  };
  assert.equal(
    isStorageCompletion(event, popup, event.origin, authorization),
    true,
  );
  assert.equal(
    isStorageCompletion(
      { ...event, origin: "https://elsewhere.example" },
      popup,
      event.origin,
      authorization,
    ),
    false,
  );
  assert.equal(
    isStorageCompletion(
      { ...event, source: {} as Window },
      popup,
      event.origin,
      authorization,
    ),
    false,
  );
  assert.equal(
    isStorageCompletion(
      { ...event, data: { ...event.data, state: "other-state" } },
      popup,
      event.origin,
      authorization,
    ),
    false,
  );
  assert.equal(
    trustedConsentUrl(authorization.consent_url),
    authorization.consent_url,
  );
  for (const url of [
    "javascript:alert(1)",
    "https://user:pass@example.com",
    "http://remote.example",
    null,
  ])
    assert.throws(() => trustedConsentUrl(url));
});
