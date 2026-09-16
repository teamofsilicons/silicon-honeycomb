import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
const pages = JSON.parse(
  await readFile(new URL("../pages.json", import.meta.url), "utf8"),
) as { slug: string; title: string }[];
test("all articles load directly with readable content and no horizontal page overflow", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  for (const item of pages) {
    const response = await page.goto(`/${item.slug ? item.slug + "/" : ""}`);
    expect(response?.status()).toBe(200);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(
      item.title,
    );
    await expect(page.locator("article")).not.toBeEmpty();
    await expect(
      page.getByRole("button", { name: "Search documentation" }),
    ).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  expect(errors).toEqual([]);
});
test("search finds upload guide, supports empty results, and follows navigation", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search documentation" }).click();
  const input = page.getByRole("searchbox", { name: "Search the docs" });
  await expect(input).toBeFocused();
  await input.fill("upload application");
  await page
    .getByRole("dialog")
    .getByRole("link", { name: /Upload an application/ })
    .click();
  await expect(page).toHaveURL(/\/upload-an-app\/$/);
  await page.getByRole("button", { name: "Search documentation" }).click();
  await input.fill("zzzznoexist987");
  await expect(page.getByText("No results. Try")).toBeVisible();
  await page.getByRole("button", { name: "Close search" }).click();
  await expect(page.getByRole("dialog")).not.toBeVisible();
  await expect(
    page.getByRole("button", { name: "Search documentation" }),
  ).toBeFocused();
});
test("search has actionable recovery when its index fails", async ({
  page,
}) => {
  let first = true;
  await page.route("**/search.json", (route) => {
    if (first) {
      first = false;
      return route.fulfill({ status: 503, body: "unavailable" });
    }
    return route.continue();
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Search documentation" }).click();
  await expect(page.getByText("Search could not load.")).toBeVisible();
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.locator(".search-result").first()).toBeVisible();
});
test("copy button copies only code and manifest download is valid", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.goto("/quickstart/");
  await page
    .getByRole("button", { name: "Copy code", exact: true })
    .first()
    .click();
  await expect(
    page.getByRole("button", { name: "Copy code", exact: true }).first(),
  ).toHaveText("Copied");
  expect(await page.evaluate(() => navigator.clipboard.readText())).toContain(
    "/bin/bash -c",
  );
  expect(
    await page.evaluate(() => navigator.clipboard.readText()),
  ).not.toContain("Copied");
  const result = await page.request.get("/examples/honeycomb.yaml");
  expect(result.ok()).toBe(true);
  expect(await result.text()).toContain("windows-aarch64:");
});
test("navigation is available at the current viewport", async ({
  page,
}, info) => {
  await page.goto("/");
  if (info.project.name === "mobile")
    await page
      .getByRole("button", { name: "Toggle documentation menu" })
      .click();
  await page
    .getByRole("navigation", { name: "Documentation", exact: true })
    .getByRole("link", { name: "Rust client SDK", exact: true })
    .click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Rust client SDK",
  );
  if (info.project.name === "mobile")
    await expect(page.locator("#sidebar")).not.toBeVisible();
});
test("static content and navigation work without JavaScript", async ({
  browser,
}) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto("http://127.0.0.1:4180/package-format/");
  await expect(
    page.getByRole("heading", { name: "Complete manifest", exact: false }),
  ).toBeVisible();
  await expect(page.locator("article")).toContainText("format_version: 1");
  await context.close();
});
