import { test, expect } from "@playwright/test";
test("bundle editor preserves existing members and retries an uncertain save", async ({ page }) => {
  const app_ids = ["tos>iam", "tos>dm", "tos>briefcase", "tos>commit", "tos>remind", "tos>waveform", "tos>browser", "tos>starter"];
  const bundle = { bundle_id: "tos>interface", app_name: "Silicon Interface", app_ids, iam_revision: 1 };
  const writes: { key: string; revision: string; body: unknown }[] = [];
  await page.route("**/api/session", route => route.fulfill({json: {authenticated: true, identity: {principal_id:"fixture-owner", organizations:{tos:"org_owner"}}}}));
  await page.route("**/api/v1/bundles", route => route.fulfill({json:{items:[bundle]}}));
  await page.route("**/api/v1/bundles/*", async route => {
    writes.push({key:route.request().headers()["idempotency-key"], revision:route.request().headers()["if-match"], body:route.request().postDataJSON()});
    if (writes.length === 1) return route.fulfill({status:503,json:{error:{message:"Response interrupted; retry the save."}}});
    return route.fulfill({json:{state:"accepted",iam_revision:2}});
  });
  await page.goto("http://localhost:19174");
  if (await page.getByRole("button", {name:"Open navigation"}).isVisible()) await page.getByRole("button", {name:"Open navigation"}).click();
  await page.getByRole("button", {name:"Bundles",exact:true}).click();
  await page.getByRole("button", {name:/Silicon Interface.*Edit bundle/}).click();
  await expect(page.getByLabel("Bundle application ID")).toBeDisabled();
  await expect(page.getByLabel("Application IDs")).toHaveValue(app_ids.join("\n"));
  await page.getByRole("button", {name:"Save bundle",exact:true}).click();
  await expect(page.getByRole("alert")).toContainText("Response interrupted");
  await page.getByRole("button", {name:"Save bundle",exact:true}).click();
  await expect(page.getByRole("status").filter({ hasText: "Bundle accepted by IAM" })).toBeVisible();
  expect(writes).toHaveLength(2);expect(writes[0]).toEqual(writes[1]);expect(writes[0].revision).toBe("1");
});
