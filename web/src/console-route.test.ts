import assert from "node:assert/strict";
import test from "node:test";
import { consoleApplicationPath, parseConsoleApplicationRoute, discussionPath, parseDiscussionRoute, viewForPath } from "./console-route";

test("console tabs round trip as reloadable application routes", () => {
  for (const tab of ["overview", "reviews", "release", "access"] as const) {
    assert.deepEqual(parseConsoleApplicationRoute(consoleApplicationPath("briefcase",tab)), {appId:"briefcase",tab});
  }
  assert.equal(parseConsoleApplicationRoute("/apps/briefcase/releases/prod/2.1.0"),undefined);
  assert.equal(parseConsoleApplicationRoute("/apps/briefcase/ata"),undefined);
  assert.equal(parseConsoleApplicationRoute("/apps/..%2Fsecret/access"),undefined);
});

test("discussion links retain the inbox and reject malformed path segments", () => {
  for (const direction of ["sent","received"] as const) {
    const path=discussionPath({id:"request-one",provider:"briefcase"},direction);
    assert.deepEqual(parseDiscussionRoute(path),{id:"request-one",provider:"briefcase",direction});
    assert.equal(viewForPath(path),"discussion");
  }
  assert.equal(parseDiscussionRoute("/requests/received/%E0%A4%A/briefcase"),undefined);
  assert.equal(parseDiscussionRoute("/requests/sent/id%2Fother/briefcase"),undefined);
});

test("central verifications and existing console destinations have independent URLs", () => {
  assert.equal(viewForPath("/app-to-app"),"ata-verifications");
  assert.equal(viewForPath("/app-to-app/"),"ata-verifications");
  assert.equal(viewForPath("/requests/received"),"review-requests");
  assert.equal(viewForPath("/requests/sent"),"sent-requests");
  assert.equal(viewForPath("/drafts"),"drafts");
  assert.equal(viewForPath("/"),"applications");
});
