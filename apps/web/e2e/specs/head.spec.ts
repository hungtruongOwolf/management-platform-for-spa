import { expect, test } from "@playwright/test";

// Tab icons and the manifest are declared after the first paint (app/deferredHead.ts, LCP budget):
// they must still end up in <head>, and every declared file must be served. (Headless Chromium
// does not fetch favicons itself, so the files are requested here.)
test("tab icons and the manifest are declared once the first screen is painted", async ({ page }) => {
  await page.goto("/dang-nhap");
  await expect(page.getByRole("heading", { name: "Viên Phúc Y Đường" })).toBeVisible();
  const icons = page.locator('head link[rel="icon"]');
  await expect(icons).toHaveCount(4);
  await expect(page.locator('head link[rel="icon"][href^="/favicon.svg?v="]')).toHaveCount(1);
  await expect(page.locator('head link[rel="manifest"]')).toHaveCount(1);
  await expect(page.locator("head link[data-placeholder]")).toHaveCount(0);
  const hrefs = await page
    .locator('head link[rel="icon"], head link[rel="manifest"]')
    .evaluateAll((links) => links.map((l) => l.getAttribute("href") ?? ""));
  for (const href of hrefs) {
    expect((await page.request.get(href)).status(), href).toBe(200);
  }
});
