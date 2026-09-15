import { test, expect } from "@playwright/test";
const library = "http://localhost:19173",
  consoleSite = "http://localhost:19174";
test("public catalog, typo search, private visibility, sign-in and sign-out", async ({
  page,
}) => {
  await page.goto(library);
  await expect(
    page.getByRole("heading", { name: "Briefcase", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Internal tools", exact: true }),
  ).toHaveCount(0);
  await page
    .getByRole("searchbox", { name: "Search applications" })
    .fill("breifcase");
  await expect(
    page.getByRole("heading", { name: "Briefcase", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Waveform", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("searchbox", { name: "Search applications" }).fill("");
  await page.getByRole("link", { name: "Sign in with IAM" }).click();
  await expect(
    page.getByRole("heading", { name: "Internal tools", exact: true }),
  ).toBeVisible();
  const cookies = await page.context().cookies();
  expect(
    cookies.find((c) => c.name === "honeycomb_library_session")?.httpOnly,
  ).toBe(true);
  expect(await page.evaluate(() => Object.keys(localStorage))).not.toContain(
    "access_token",
  );
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Internal tools", exact: true }),
  ).toHaveCount(0);
});
test("console is gated and creates a private application through the backend", async ({
  page,
}, info) => {
  await page.goto(consoleSite);
  await expect(
    page.getByRole("heading", { name: /Your next application/ }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Create application", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("link", { name: "Continue with IAM" }).click();
  await expect(
    page.getByRole("heading", { name: "Your applications." }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Create application", exact: true })
    .click();
  const dialog = page.getByRole("dialog");
  await dialog
    .getByLabel("Application handle")
    .fill(`web-e2e-${info.project.name}-${Date.now()}`);
  const name = `Web E2E ${info.project.name} ${Date.now()}`;
  await dialog.getByLabel("Application name", { exact: true }).fill(name);
  await dialog
    .getByLabel("Description", { exact: true })
    .fill("This is a useful application for the Silicon ecosystem. ".repeat(7));
  await dialog.getByLabel("Webhook URL").fill("https://example.com/webhook/");
  await dialog
    .getByLabel("Webhook signing secret")
    .fill("fixture-signing-secret-0000000000000000000");
  await dialog
    .getByRole("button", { name: "Create private application" })
    .click();
  await expect(dialog).not.toBeVisible();
  await expect(page.getByRole("heading", { name, exact: true })).toBeVisible();
  const card = page
    .getByRole("button")
    .filter({ has: page.getByRole("heading", { name, exact: true }) });
  await expect(card).toContainText("private");
  await card.click();
  await page
    .getByRole("button", { name: "Edit configuration", exact: true })
    .click();
  await page
    .getByRole("dialog")
    .getByLabel("Application name", { exact: true })
    .fill(name + " updated");
  await expect(
    page.getByRole("dialog").getByLabel("Webhook signing secret"),
  ).toHaveValue("");
  await page
    .getByRole("button", { name: "Save configuration", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: name + " updated", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button")
    .filter({
      has: page.getByRole("heading", { name: name + " updated", exact: true }),
    })
    .click();
  await page.getByRole("button", { name: "Releases & publication" }).click();
  await expect(
    page.getByRole("button", { name: "Request publication", exact: true }),
  ).toBeDisabled();
});
test("login callback rejects missing state and writes reject cross-origin requests", async ({
  request,
}) => {
  const callback = await request.get(
    `${consoleSite}/auth/callback?slt=fixture-owner&state=wrong`,
  );
  expect(callback.status()).toBe(400);
  const write = await request.post(`${consoleSite}/api/v1/apps`, {
    data: {},
    headers: { Origin: "https://attacker.invalid" },
  });
  expect(write.status()).toBe(403);
});
test("both websites fit the viewport and show installation instructions", async ({
  page,
}) => {
  for (const url of [library, consoleSite]) {
    await page.goto(url);
    await expect(page.locator("main")).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  await page.goto(library);
  if (await page.getByRole("button", { name: "Open navigation" }).isVisible())
    await page.getByRole("button", { name: "Open navigation" }).click();
  await page.getByRole("button", { name: "Get started", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Install Honeycomb", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("cargo install silicon-honeycomb-cli --locked", {
      exact: true,
    }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});

test("draft closing flushes the latest fields and scope edits", async ({
  page,
}) => {
  await page.goto(consoleSite);
  await page.getByRole("link", { name: "Continue with IAM" }).click();
  await page
    .getByRole("button", { name: "Create application", exact: true })
    .click();
  const dialog = page.getByRole("dialog");
  const name = `Draft ${Date.now()}`;
  await dialog.getByLabel("Application name", { exact: true }).fill(name);
  await dialog
    .getByRole("button", { name: "Configure scopes & OBO endpoints" })
    .click();
  const scopes = JSON.stringify({
    iam: ["self.identity.read", "self.profile.read"],
    external: [],
  });
  await dialog.getByLabel("Application scopes").fill(scopes);
  await dialog.getByRole("button", { name: "Close dialog" }).click();
  await expect(dialog).not.toBeVisible();
  if (await page.getByRole("button", { name: "Open navigation" }).isVisible())
    await page.getByRole("button", { name: "Open navigation" }).click();
  await page.getByRole("button", { name: "Drafts", exact: true }).click();
  await page.getByRole("button").filter({ hasText: name }).click();
  await expect(
    dialog.getByLabel("Application name", { exact: true }),
  ).toHaveValue(name);
  if (!(await dialog.getByLabel("Application scopes").isVisible()))
    await dialog
      .getByRole("button", { name: "Configure scopes & OBO endpoints" })
      .click();
  await expect(dialog.getByLabel("Application scopes")).toHaveValue(scopes);
});

test("testing setup shows per-service failures and supports retry", async ({
  page,
}) => {
  await page.goto(consoleSite);
  await page.getByRole("link", { name: "Continue with IAM" }).click();
  await expect(
    page.getByRole("heading", { name: "Your applications.", exact: true }),
  ).toBeVisible();
  if (await page.getByRole("button", { name: "Open navigation" }).isVisible())
    await page.getByRole("button", { name: "Open navigation" }).click();
  await page
    .getByRole("button", { name: "Testing environments", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Create environment", exact: true })
    .click();
  const name = `Browser environment ${Date.now()}`;
  await page.getByRole("dialog").getByLabel("Environment name").fill(name);
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Create environment", exact: true })
    .click();
  const row = page
    .locator(".environment-row")
    .filter({ has: page.getByRole("heading", { name, exact: true }) });
  await expect(row).toContainText("provisioning");
  await row.getByRole("button", { name: "Manage", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(
    dialog.getByRole("heading", { name: "Service progress", exact: true }),
  ).toBeVisible();
  await expect(dialog).toContainText("tos>iam");
  await expect(dialog).toContainText(
    "Fixture deliberately leaves shared lifecycle provisioning pending",
  );
  await dialog
    .getByRole("button", { name: "Retry setup", exact: true })
    .click();
  await expect(dialog).toContainText("provisioning");
  await expect(
    dialog.getByRole("button", { name: "Retry setup", exact: true }),
  ).toBeEnabled();
});


test("concurrent browser requests share one rotating refresh and sign out cleanly", async ({ request }) => {
  // This follows the hosted callback without executing the page's JavaScript.
  // The fixture issues a one-second access token and single-use refresh token.
  expect((await request.get(`${library}/auth/login`)).ok()).toBe(true);
  const sessions = await Promise.all(Array.from({ length: 6 }, async () => {
    const response = await request.get(`${library}/api/session`);
    expect(response.ok()).toBe(true);
    return response.json();
  }));
  for (const session of sessions) expect(session.authenticated).toBe(true);
  const logout = await request.post(`${library}/auth/logout`, { headers: { Origin: library } });
  expect(logout.ok()).toBe(true);
  expect(await (await request.get(`${library}/api/session`)).json()).toEqual({ authenticated: false });
  const catalog = await (await request.get(`${library}/api/v1/apps`)).json();
  expect(JSON.stringify(catalog)).not.toContain("tos>internal-tools");
});


test("console import exposes pending integration without disabling the ready environment", async ({ page }, info) => {
  await page.goto(consoleSite);
  await page.getByRole("link", { name: "Continue with IAM" }).click();
  await expect(page.getByRole("heading", { name: "Your applications.", exact: true })).toBeVisible();
  if (await page.getByRole("button", { name: "Open navigation" }).isVisible()) await page.getByRole("button", { name: "Open navigation" }).click();
  await page.getByRole("button", { name: "Testing environments", exact: true }).click();
  const row=page.locator(".environment-row").filter({ has: page.getByRole("heading", { name: `Import sandbox ${info.project.name}`, exact: true }) });
  await row.getByRole("button", { name: "Manage", exact: true }).click();
  const dialog=page.getByRole("dialog");
  {
    await dialog.getByText("Import an application", { exact: true }).click();
    await dialog.getByLabel("Application ID", { exact: true }).fill("tos>briefcase");
    await dialog.getByRole("button", { name: "Import application", exact: true }).click();
  }
  await expect(dialog.getByRole("button", { name: "Retry setup", exact: true })).toBeVisible();
  await expect(dialog).toContainText("Fixture deliberately leaves shared lifecycle provisioning pending");
  await expect(dialog).toContainText("ready");
  await expect(dialog.getByRole("button", { name: "Clean test data", exact: true })).toHaveCount(0);
});


test("secret rotation is explicit and browser retries preserve the same operation", async ({ page }, info) => {
  await page.goto(consoleSite);
  await page.getByRole("link", { name: "Continue with IAM" }).click();
  await expect(page.getByRole("heading", { name: "Your applications.", exact: true })).toBeVisible();
  const name=info.project.name === "desktop" ? "Briefcase" : "Waveform";
  const appId=`tos>${name.toLowerCase()}`;
  await page.getByRole("heading", { name, exact: true }).click();
  const dialog=page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Access & secrets", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Rotate application secret", exact: true })).toBeDisabled();
  await dialog.getByLabel("Type the application ID to rotate its secret").fill(appId);
  await dialog.getByRole("button", { name: "Rotate application secret", exact: true }).click();
  const operations=dialog.locator(".review-thread");
  await expect(operations).toHaveCount(1);
  const id=await operations.locator(".app-id").textContent();
  await expect(operations).toContainText("Waiting for IAM’s protected management integration.");
  await operations.getByRole("button", { name: "Retry operation", exact: true }).click();
  await expect(operations).toHaveCount(1);
  await expect(operations.locator(".app-id")).toHaveText(id!);
  await expect(operations.getByRole("button", { name: "Retry operation", exact: true })).toBeEnabled();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
});
