import { test, expect, type Page } from "@playwright/test";

const consoleSite = "http://localhost:19174";
const archive = { name: "release.tar.gz", mimeType: "application/gzip", buffer: Buffer.from("browser fixture archive") };

async function fixture(page: Page) {
  const app = {
    app_id: "channels", org_id: "tos", name: "Channel fixture", description: "Tools with independent release channels.",
    visibility: "private", state: "active", revision: 4, iam_revision: 1, effective_revision: 4,
    effective_config: {}, config: { visibility: "private" }, latest_version: "1.0.0", rating: 0, reviews: 0, stars: 0, installs: 0,
  };
  const releases = {
    prod: [{ version: "1.0.0", channel: "prod", created_at: 1750000000 }],
    dev: [{ version: "7.4.2", channel: "dev", created_at: 1750000001 }],
  };
  const uploads: { channel: string | null; key: string; revision: string }[] = [];
  const promotions: { version: string; key: string; revision: string }[] = [];
  const creations: Record<string, unknown>[] = [];
  await page.route("**/api/session", route => route.fulfill({ json: {
    authenticated: true, identity: { principal_id: "fixture-owner", organizations: { tos: "org_owner" } },
  } }));
  await page.route("**/api/v1/packages/validate", route => route.fulfill({ json: { valid: true, errors: [], manifest: { app_id: app.app_id, version: "7.4.2" } } }));
  await page.route(/\/api\/v[12]\/apps(?:[/?]|$)/, async route => {
    const req = route.request();
    const url = new URL(req.url());
    if (url.pathname.endsWith("/promote")) {
      expect(url.pathname).toMatch(/^\/api\/v2\//);
      const version = req.postDataJSON().version;
      promotions.push({ version, key: req.headers()["idempotency-key"], revision: req.headers()["if-match"] });
      if (promotions.length === 1) return route.fulfill({ status: 503, json: { error: { message: "Promotion response interrupted; retry." } } });
      releases.prod.push({ version, channel: "prod", created_at: 1750000002 });
      app.latest_version = version;
      return route.fulfill({ json: { state: "accepted", version, channel: "prod" } });
    }
    if (url.pathname.endsWith("/releases")) {
      expect(url.pathname).toMatch(/^\/api\/v2\//);
      const channel = url.searchParams.get("channel");
      if (req.method() === "POST") {
        uploads.push({ channel, key: req.headers()["idempotency-key"], revision: req.headers()["if-match"] });
        if (uploads.length === 1) return route.fulfill({ status: 503, json: { error: { message: "Upload interrupted; retry." } } });
        return route.fulfill({ json: { state: "accepted", channel } });
      }
      return route.fulfill({ json: { items: releases[channel === "dev" ? "dev" : "prod"] } });
    }
    if (/\/(reviews|publication|operations)$/.test(url.pathname)) return route.fulfill({ json: { items: [] } });
    if (url.pathname.endsWith("/reconciliation")) return route.fulfill({ json: { issues: [] } });
    if (url.pathname === "/api/v1/apps") {
      if (req.method() === "POST") {
        creations.push(req.postDataJSON());
        return route.fulfill({ json: { state: "accepted", app_id: app.app_id } });
      }
      return route.fulfill({ json: { items: [app], total: 1 } });
    }
    return route.fulfill({ json: app });
  });
  return { uploads, promotions, creations };
}

test("registration requires a channel for its first archive and preserves development on retry", async ({ page }) => {
  const { uploads, creations } = await fixture(page);
  let validations = 0;
  await page.route("**/api/v1/packages/validate", route => route.fulfill({ json: {
    valid: true, errors: [], manifest: { app_id: "channels", version: ++validations === 1 ? "7.4.2-beta" : "7.4.2" },
  } }));
  await page.goto(consoleSite);
  await page.getByRole("button", { name: "Create application", exact: true }).click();
  const form = page.getByRole("dialog", { name: "Create an application", exact: true });
  await form.getByLabel("Application handle").fill("channels");
  await form.getByLabel("Application name", { exact: true }).fill("Channel fixture");
  await form.getByLabel("Description", { exact: true }).fill("This useful application provides tools for the Silicon ecosystem. ".repeat(7));
  await form.getByLabel("Webhook URL").fill("https://example.com/webhook/");
  await form.getByLabel("Webhook signing secret").fill("fixture-signing-secret-0000000000000000000");
  await form.getByLabel("First CLI archive").setInputFiles(archive);
  await form.getByRole("button", { name: "Create application", exact: true }).click();
  expect(creations).toHaveLength(0);
  expect(await form.getByLabel("First release channel").evaluate((element: HTMLSelectElement) => element.validity.valueMissing)).toBe(true);
  await form.getByLabel("First release channel").selectOption("dev");
  await form.getByRole("button", { name: "Create application", exact: true }).click();
  await expect(form.getByRole("alert")).toContainText("CLI release version must use x.x.x");
  expect(creations).toHaveLength(0);
  await form.getByRole("button", { name: "Create application", exact: true }).click();
  const detail = page.getByRole("dialog", { name: "Channel fixture", exact: true });
  await expect(detail.getByRole("alert")).toContainText("application was saved, but the release upload failed");
  await expect(detail).toContainText("release.tar.gz · Development");
  await detail.getByRole("button", { name: "Retry release upload" }).click();
  await expect(detail.getByRole("button", { name: "Retry release upload" })).toHaveCount(0);
  expect(creations).toHaveLength(1);
  expect(creations[0]).not.toHaveProperty("draft_release_channel");
  expect(uploads).toHaveLength(2);
  expect(uploads[0]).toEqual(uploads[1]);
  expect(uploads[0].channel).toBe("dev");
});

test("channel histories stay separate and promotion asks for a production version with retry safety", async ({ page }) => {
  const { promotions } = await fixture(page);
  await page.goto(consoleSite);
  await page.getByRole("heading", { name: "Channel fixture", exact: true }).click();
  await page.getByRole("button", { name: "Releases & publication" }).click();
  const history = page.getByRole("region", { name: "Release history" });
  await expect(history).toContainText("channels@1.0.0");
  await expect(history).not.toContainText("7.4.2");
  await history.getByLabel("Release history channel").selectOption("dev");
  await expect(history).toContainText("channels>test@7.4.2");
  await expect(history).not.toContainText("channels@1.0.0");
  await history.getByRole("button", { name: "Promote 7.4.2 to production" }).click();
  await expect(history.getByLabel("Production version")).toHaveValue("");
  await history.getByLabel("Production version").fill("2.0.0-beta");
  await history.getByRole("button", { name: "Create production release" }).click();
  await expect(history.getByRole("alert")).toContainText("x.x.x");
  expect(promotions).toHaveLength(0);
  await history.getByLabel("Production version").fill("2.0.0");
  await history.getByLabel("Production version").scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: test.info().outputPath("release-promotion.png") });
  await history.getByRole("button", { name: "Create production release" }).click();
  await expect(history.getByRole("alert")).toContainText("Promotion response interrupted");
  await history.getByRole("button", { name: "Create production release" }).click();
  await expect(history.getByRole("status")).toContainText("Production release 2.0.0 created");
  await expect(history.getByLabel("Release history channel")).toHaveValue("prod");
  await expect(history).toContainText("channels@2.0.0");
  expect(promotions).toHaveLength(2);
  expect(promotions[0]).toEqual(promotions[1]);
  expect(promotions[0].key).toBeTruthy();
  expect(promotions[0].revision).toBe("4");
  await history.getByLabel("Release history channel").selectOption("dev");
  await expect(history).toContainText("channels>test@7.4.2");
  await expect(history).not.toContainText("channels@2.0.0");
});

test("uploads require an explicit channel and retain it when retrying", async ({ page }) => {
  const { uploads } = await fixture(page);
  await page.goto(consoleSite);
  await page.getByRole("heading", { name: "Channel fixture", exact: true }).click();
  await page.getByRole("button", { name: "Releases & publication" }).click();
  const detail = page.getByRole("dialog", { name: "Channel fixture", exact: true });
  await expect(detail.getByLabel("Upload CLI archive")).toBeDisabled();
  await detail.getByLabel("Upload release channel").selectOption("dev");
  await detail.getByLabel("Upload CLI archive").setInputFiles(archive);
  await expect(detail.getByRole("alert")).toContainText("Upload interrupted");
  await expect(detail.getByLabel("Upload CLI archive")).toHaveValue("");
  await expect(detail).toContainText("release.tar.gz · Development");
  await detail.getByLabel("Upload release channel").selectOption("prod");
  await detail.getByRole("button", { name: "Retry release upload" }).click();
  await expect(detail.getByRole("button", { name: "Retry release upload" })).toHaveCount(0);
  expect(uploads).toHaveLength(2);
  expect(uploads[0]).toEqual(uploads[1]);
  expect(uploads[0]).toMatchObject({ channel: "dev", revision: "4" });
  expect(uploads[0].key).toBeTruthy();
  await detail.getByLabel("Upload CLI archive").setInputFiles(archive);
  await expect.poll(() => uploads.length).toBe(3);
  expect(uploads[2].channel).toBe("prod");
  expect(uploads[2].key).not.toBe(uploads[0].key);
});

test("library defaults to official installs and explains experimental opt-in", async ({ page }) => {
  await fixture(page);
  await page.goto("http://localhost:19173");
  await page.getByRole("heading", { name: "Channel fixture", exact: true }).click();
  const detail = page.getByRole("dialog", { name: "Channel fixture", exact: true });
  await expect(detail.locator("code").filter({ hasText: "honeycomb install 'channels'" })).toBeVisible();
  await expect(detail).toContainText("follows production updates");
  await detail.getByText("Experimental development releases", { exact: true }).click();
  await expect(detail.locator("code").filter({ hasText: "honeycomb install 'channels>test'" })).toBeVisible();
  await expect(detail).toContainText("append @x.x.x");
});

test("publication progress distinguishes equal versions from both channels", async ({ page }) => {
  await fixture(page);
  await page.route("**/api/v1/apps/*/publication", route => route.fulfill({ json: { items: [{
    id: "fixture-publication", state: "awaiting_activation", revision: 4, gates: [], messages: [],
    activation: { archives: [
      { channel: "prod", version: "1.0.0", state: "accepted" },
      { channel: "dev", version: "1.0.0", state: "pending" },
    ] },
  }] } }));
  await page.goto(consoleSite);
  await page.getByRole("heading", { name: "Channel fixture", exact: true }).click();
  await page.getByRole("button", { name: "Releases & publication" }).click();
  const publication = page.locator(".review-thread");
  await expect(publication).toContainText("Production release 1.0.0 · accepted");
  await expect(publication).toContainText("Dev release 1.0.0 · pending");
});
