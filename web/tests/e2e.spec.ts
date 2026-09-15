import { test, expect } from "@playwright/test";
const consoleSite = "http://localhost:19174";

test("webhook approval and signing-secret rotation use transient verification and durable retry", async ({ page }, info) => {
  await page.goto(consoleSite);
  await page.getByRole("link", { name: "Continue with IAM" }).click();
  const handle = `webhook-${info.project.name}-${Date.now()}`;
  const name = `Webhook test ${info.project.name}`, appId = `tos>${handle}`;
  const created = await page.context().request.post(`${consoleSite}/api/v1/apps`, {
    headers: { Origin: consoleSite, "Idempotency-Key": `${handle}-create` },
    data: { org_id: "tos", local_app_id: handle, name,
      description: "This application exercises isolated webhook configuration and verified signing key rotation for the Silicon ecosystem. ".repeat(7),
      webhook_url: "https://example.com/webhook/", webhook_secret: "fixture-webhook-secret-00000000000000" },
  });
  expect(created.ok()).toBe(true);
  await page.reload();
  await page.getByRole("button").filter({ has: page.getByRole("heading", { name, exact: true }) }).click();
  await page.getByRole("button", { name: "Access & secrets" }).click();
  const section = page.getByRole("region", { name: "Webhook management" });
  const proof = section.getByLabel("IAM webhook step-up assertion");
  await expect(section.getByRole("button", { name: "Approve webhook destination" })).toBeDisabled();
  await proof.fill("expired-proof");
  await section.getByRole("button", { name: "Approve webhook destination" }).click();
  await expect(section.getByText("IAM requires fresh identity verification for this action.")).toBeVisible();
  await expect(proof).toHaveValue("");
  await proof.fill("fixture-webhook-proof");
  await section.getByRole("button", { name: "Retry webhook change" }).click();
  await expect(section.getByText("IAM has no destination awaiting approval.")).toBeVisible();
  await proof.fill("fixture-webhook-proof");
  const secret = section.getByLabel("New webhook signing secret");
  await secret.fill("browser-webhook-signing-secret-0000000000000");
  await section.getByLabel("Confirm application ID for webhook rotation").fill(appId);
  await section.getByRole("button", { name: "Rotate webhook signing secret" }).click();
  await expect(section.getByText("Webhook signing secret rotated. Update your receiver to verify the new secret.")).toBeVisible();
  await expect(secret).toHaveValue("");
  await expect(proof).toHaveValue("");
  await section.getByRole("heading", { name: "Webhook management", exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: `/tmp/honeycomb-webhook-${info.project.name}.png` });
  expect(await page.evaluate(() => JSON.stringify({ local: localStorage, session: sessionStorage }))).not.toContain("browser-webhook-signing-secret");
});
