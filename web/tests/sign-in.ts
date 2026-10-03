import { expect, type Page } from "@playwright/test";
export async function signIn(page: Page) {
  const popupOpened = page.waitForEvent("popup");
  await page.getByRole("button", { name: "Continue as Carbon" }).first().click();
  const popup = await popupOpened;
  await expect(page.getByRole("button", { name: "Sign out", exact: true })).toBeVisible();
  await expect.poll(() => popup.isClosed()).toBe(true);
}
