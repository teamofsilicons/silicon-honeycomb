import { test, expect } from "@playwright/test";
import { signIn } from "./sign-in";
const site = "http://localhost:19174";
test("Carbon popup signs in and returns to the page that opened it", async ({ page }) => {
  await page.goto(site + "/requests/sent");
  await signIn(page);
  await expect(page).toHaveURL(site + "/requests/sent");
  await expect(page.getByRole("heading", { name: "Sent requests", exact: true })).toBeVisible();
});
test("a Silicon entry point rejects a Carbon response before establishing a session", async ({ page }) => {
  // The fixture intentionally returns Carbon even when Silicon is requested.
  await page.goto(site);
  const opened = page.waitForEvent("popup");
  await page.getByRole("button", { name: "Continue as Silicon" }).first().click();
  const popup = await opened;
  await expect(popup.locator("body")).toContainText("did not return a Silicon account");
  const session = await page.context().request.get(site + "/api/session");
  expect(await session.json()).toEqual({ authenticated: false });
  await popup.close();
  await expect(page.getByRole("alert").first()).toContainText("sign-in window was closed");
});

test("storage callback follows only the completed server-bound redirect", async ({ page }) => {
  await page.route("**/api/v1/storage-authorizations/request/complete", route => route.fulfill({ json: { status: "ready", redirect_url: site + "/requests/sent" } }));
  await page.goto(site + "/storage-authorization?authorization_id=request&state=bound&code=one-use&redirect_url=https%3A%2F%2Fevil.invalid");
  await expect(page).toHaveURL(site + "/requests/sent");
});

test("storage callback without a return destination keeps a useful completion page", async ({ page }) => {
  await page.route("**/api/v1/storage-authorizations/request/complete", route => route.fulfill({ json: { status: "ready", redirect_url: null } }));
  await page.goto(site + "/storage-authorization?authorization_id=request&state=bound&code=one-use");
  await expect(page.getByRole("heading", { name: "You’re ready to continue." })).toBeVisible();
  await expect(page).toHaveURL(site + "/storage-authorization");
  await page.getByRole("button", { name: "Return to Honeycomb" }).click();
  await expect(page).toHaveURL(site + "/");
});
