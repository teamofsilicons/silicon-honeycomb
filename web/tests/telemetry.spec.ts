import { test, expect } from "@playwright/test";

for (const [site, origin, login] of [
  ["library", "http://localhost:19173", "Sign in with IAM"],
  ["console", "http://localhost:19174", "Continue with IAM"],
] as const) {
  test(`${site} diagnostics respect a persistent opt-out`, async ({ page }) => {
    const events: Record<string, unknown>[] = [];
    page.on("request", request => {
      if (request.url() === `${origin}/api/v1/telemetry`)
        events.push(request.postDataJSON());
    });
    await page.goto(origin);
    await page.getByRole("link", { name: login }).click();
    await expect.poll(() => events.length).toBeGreaterThan(0);
    expect(events.every(event => event.source === site)).toBe(true);
    expect(Object.keys(events[0]).sort()).toEqual(["action", "duration_ms", "event", "source", "success"]);
    await page.getByRole("button", { name: "Telemetry settings", exact: true }).click();
    const preference = page.getByRole("checkbox", { name: "Share usage and diagnostics" });
    await expect(preference).toBeChecked();
    await preference.uncheck();
    const beforeReload = events.length;
    const request = page.waitForRequest(request => request.url().includes("/api/v1/apps"));
    await page.reload();
    expect((await request).headers()["x-honeycomb-telemetry"]).toBe("false");
    await page.getByRole("button", { name: "Telemetry settings", exact: true }).click();
    await expect(preference).not.toBeChecked();
    expect(events).toHaveLength(beforeReload);
    await preference.check();
    await page.reload();
    await expect.poll(() => events.length).toBeGreaterThan(beforeReload);
  });
}
