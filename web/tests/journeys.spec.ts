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
  await dialog.getByRole("button", { name: "Refresh IAM state", exact: true }).click();
  await expect(dialog.getByRole("status")).toContainText("Synchronization pending");
  await expect(dialog.getByRole("status")).toContainText("not yet available");
  await expect(dialog.getByRole("button", { name: "Rotate application secret", exact: true })).toBeDisabled();
  await dialog.getByLabel("Type the application ID to rotate its secret").fill(appId);
  await dialog.getByLabel("IAM application-secret step-up assertion").fill("transient-app-proof");
  const initialRequest = page.waitForRequest(r=>r.url().endsWith("/secret-rotations") && r.method()==="POST");
  await dialog.getByRole("button", { name: "Rotate application secret", exact: true }).click();
  const sent = await initialRequest;
  expect(sent.postDataJSON().step_up_assertion).toBe("transient-app-proof");
  await expect(dialog.getByLabel("IAM application-secret step-up assertion")).toHaveValue("");
  const operations=dialog.locator(".review-thread");
  await expect(operations).toHaveCount(1);
  const id=await operations.locator(".app-id").textContent();
  await expect(operations).toContainText("Waiting for IAM’s protected management integration.");
  await dialog.getByLabel("IAM application-secret step-up assertion").fill("renewed-app-proof");
  const retryRequest = page.waitForRequest(r=>r.url().endsWith("/secret-rotations") && r.method()==="POST");
  await operations.getByRole("button", { name: "Retry operation", exact: true }).click();
  const retried = await retryRequest;
  expect(retried.postDataJSON().step_up_assertion).toBe("renewed-app-proof");
  expect(retried.headers()["idempotency-key"]).toBe(sent.headers()["idempotency-key"]);
  await expect(dialog.getByLabel("IAM application-secret step-up assertion")).toHaveValue("");
  await expect(operations).toHaveCount(1);
  await expect(operations.locator(".app-id")).toHaveText(id!);
  await expect(operations.getByRole("button", { name: "Retry operation", exact: true })).toBeEnabled();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
});


test("provider administrators can discuss and approve their review gate", async ({ page }, info) => {
  await page.goto(consoleSite);
  await page.getByRole("link", { name: "Continue with IAM" }).click();
  await expect(page.getByRole("heading", { name: "Your applications.", exact: true })).toBeVisible();
  if (await page.getByRole("button", { name: "Open navigation" }).isVisible()) await page.getByRole("button", { name: "Open navigation" }).click();
  await page.getByRole("button", { name: "Review requests", exact: true }).click();
  await page.getByRole("heading", { name: `Review candidate ${info.project.name}`, exact: true }).click();
  const dialog=page.getByRole("dialog");
  await dialog.getByLabel("Reply to this review").fill("The selected file access is appropriate for this application.");
  await dialog.getByRole("button", { name: "Send reply", exact: true }).click();
  await expect(dialog.locator(".review-thread")).toContainText("selected file access is appropriate");
  await dialog.getByLabel("Reason", { exact: true }).fill("Reviewed the declared file access.");
  await dialog.getByRole("button", { name: "Submit decision", exact: true }).click();
  await expect(dialog).toContainText("approved");
  await expect(dialog.getByRole("button", { name: "Submit decision", exact: true })).toBeDisabled();
});

test("console uploads and activates an approved release visible in the anonymous library", async ({ page, request }, info) => {
  const { mkdtempSync, mkdirSync, writeFileSync, rmSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const { fileURLToPath } = await import("node:url");
  const cliPath = fileURLToPath(new URL("../../target/debug/honeycomb", import.meta.url));
  const { execFileSync } = await import("node:child_process");
  const root = mkdtempSync(join(tmpdir(), "honeycomb-browser-release-"));
  const handle = `release-${info.project.name}-${Date.now()}`;
  const appId = `tos>${handle}`, name = `Public release ${info.project.name}`;
  try {
    await page.goto(consoleSite);
    await page.getByRole("link", { name: "Continue with IAM" }).click();
    await expect(page.getByRole("heading", { name: "Your applications." })).toBeVisible();
    const create = await page.context().request.post(`${consoleSite}/api/v1/apps`, {
      headers: { Origin: consoleSite, "Idempotency-Key": `${handle}-create` },
      data: { org_id: "tos", local_app_id: handle, name,
        description: "This release exercises uploading, review, activation and public discovery in the Silicon ecosystem. ".repeat(7),
        webhook_url: "https://example.com/webhook/", webhook_secret: "fixture-webhook-secret-00000000000000" },
    });
    expect(create.ok()).toBe(true);
    const targets: Record<string, unknown> = {};
    for (const target of ["linux-x86_64", "linux-aarch64", "windows-x86_64", "windows-aarch64", "macos-x86_64", "macos-aarch64"]) {
      mkdirSync(join(root, "targets", target, "bin"), { recursive: true });
      writeFileSync(join(root, "targets", target, "bin", "greet"), "#!/bin/sh\necho hello\n", { mode: 0o755 });
      targets[target] = { root: `targets/${target}`, executables: { app: "bin/greet" } };
    }
    writeFileSync(join(root, "honeycomb.yaml"), JSON.stringify({ format_version: 1, app_id: appId, version: "1.0.0", bin: { greet: "app" }, targets }));
    const archive = join(root, "release.tar.gz");
    const cliEnv = { ...process.env, SILICON_HOME: join(root, "isolated-cli-home") };
    execFileSync(cliPath, ["config", "set", "auto_update", "false"], { env: cliEnv });
    execFileSync(cliPath, ["pack", root, "--output", archive], { env: cliEnv });
    await page.reload();
    await page.getByRole("heading", { name, exact: true }).click();
    await page.getByRole("button", { name: "Releases & publication" }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Upload CLI archive").setInputFiles(archive);
    await expect(dialog.getByRole("button", { name: "Request publication", exact: true })).toBeEnabled();
    await dialog.getByLabel("Tell reviewers about your application").fill("Please review this complete six-target release.");
    await dialog.getByRole("button", { name: "Request publication", exact: true }).click();
    await expect(dialog).toContainText("awaiting validator");
    const publications = await (await page.context().request.get(`${consoleSite}/api/v1/apps/${encodeURIComponent(appId)}/publication`)).json();
    // The independent validator uses an explicit test identity; the owner cannot self-approve.
    const login = await request.post("http://127.0.0.1:19180/api/v1/auth/login", {
      headers: { "Idempotency-Key": `${handle}-validator-login` }, data: { slt: "fixture-validator" },
    });
    const session = await login.json();
    const decision = await request.post(`http://127.0.0.1:19180/api/v1/review-requests/${publications.items[0].id}/honeycomb/decisions`, {
      headers: { Authorization: `Bearer ${session.access_token}`, "Idempotency-Key": `${handle}-approve`, "If-Match": "1" }, data: { decision: "approve" },
    });
    expect(decision.ok()).toBe(true);
    await page.reload();
    await page.getByRole("heading", { name, exact: true }).click();
    await page.getByRole("button", { name: "Releases & publication" }).click();
    await page.getByRole("button", { name: "Activate public release", exact: true }).click();
    await expect(page.getByRole("dialog")).toContainText("published");
    await page.goto(library);
    await page.getByRole("searchbox", { name: "Search applications" }).fill(name);
    await expect(page.getByRole("heading", { name, exact: true })).toBeVisible();
    await expect(page.getByRole("link", { name: "Sign in with IAM" })).toBeVisible();
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test("permission picker reflects IAM eligibility and preserves scope edits", async ({ page }) => {
  await page.goto(consoleSite);
  await page.getByRole("link", { name: "Continue with IAM" }).click();
  await page.getByRole("button", { name: "Create application", exact: true }).click();
  const dialog=page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Configure scopes & OBO endpoints" }).click();
  await expect(dialog.getByRole("checkbox", { name: "self.profile.read", exact: true })).toBeChecked();
  await expect(dialog.getByRole("checkbox", { name: "directory.carbons.read", exact: true })).toBeDisabled();
  await expect(dialog).toContainText("Unavailable for this organization");
  await dialog.getByRole("checkbox", { name: "self.profile.read", exact: true }).uncheck();
  let scopes=JSON.parse(await dialog.getByLabel("Application scopes").inputValue());
  expect(scopes.iam).not.toContain("self.profile.read");
  expect(scopes.iam).toContain("self.identity.read");
  await dialog.getByRole("checkbox", { name: "self.profile.read", exact: true }).check();
  scopes=JSON.parse(await dialog.getByLabel("Application scopes").inputValue());
  expect(scopes.iam).toContain("self.profile.read");
  await dialog.getByLabel("Application scopes").fill("{");
  await expect(dialog.getByRole("checkbox", { name: "self.profile.read", exact: true })).toBeDisabled();
  await expect(dialog).toContainText("Correct the application scopes JSON");
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
});
