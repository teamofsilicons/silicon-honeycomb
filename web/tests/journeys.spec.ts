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
