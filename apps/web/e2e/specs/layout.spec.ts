/**
 * Layout audit (EM 2026-11-05): no hover/press effect moves a neighbour, and no screen scrolls
 * sideways at 360 / 768 / 1280 / 1440 px. Every screen × width is attached as a screenshot to the
 * report for visual review.
 */
import { expect, test } from "@playwright/test";
import { ownerState } from "../stack";
import { interactiveSample, shiftsOnInteraction } from "../support/layout";

test.use({ storageState: ownerState, reducedMotion: "reduce" });

const SCREENS = [
  ["tong-quan", "/"],
  ["quay", "/quay"],
  ["khach-hang", "/khach-hang"],
  ["bao-cao", "/bao-cao"],
  ["goi-lieu-trinh", "/goi-lieu-trinh"],
  ["khach-lau-khong-quay-lai", "/khach-lau-khong-quay-lai"],
  ["bang-gia", "/bang-gia"],
  ["bang-gia-goi", "/bang-gia/goi"],
  ["cai-dat", "/cai-dat"],
] as const;

const WIDTHS = [360, 768, 1280, 1440] as const;

for (const [name, path] of SCREENS) {
  test(`hover and press move nothing else on ${name}`, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto(path);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await page.waitForLoadState("networkidle");
    const controls = await interactiveSample(page);
    expect(controls.length).toBeGreaterThan(0);
    for (const control of controls) {
      const label = (await control.getAttribute("aria-label")) ?? (await control.innerText()).slice(0, 30);
      const shifts = await shiftsOnInteraction(page, control);
      expect(shifts, `hover/press on "${label}" moved other elements`).toEqual([]);
    }
  });

  test(`${name} fits every width without sideways scroll`, async ({ page }, testInfo) => {
    for (const width of WIDTHS) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(path);
      await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
      await page.waitForLoadState("networkidle");
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
      expect(overflow, `${name} at ${String(width)}px scrolls sideways`).toBeLessThanOrEqual(0);
      await testInfo.attach(`${name}-${String(width)}`, {
        body: await page.screenshot({ fullPage: true }),
        contentType: "image/png",
      });
    }
  });
}
