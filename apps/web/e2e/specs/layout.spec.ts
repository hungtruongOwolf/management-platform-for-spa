/**
 * Layout audit (EM 2026-11-05, rounds 2 and 3):
 * - no hover/press effect moves a neighbour;
 * - every screen holds from 320 to 1920 px, swept in 80 px steps, plus 1280×800 at 200 % zoom
 *   (= a 640×400 viewport): no sideways scroll, no overlapping siblings, no clipped text;
 * - no focus ring covers other text (at desktop and phone widths).
 * Screenshots of every screen at 360 / 768 / 1280 / 1440 are attached to the report.
 */
import { expect, test, type Page } from "@playwright/test";
import { ownerState, stack } from "../stack";
import { interactiveSample, shiftsOnInteraction } from "../support/layout";
import { clippedText, focusRingsOverText, overlappingSiblings, sidewaysOverflow } from "../support/responsive";
import { uniquePhone } from "../support/run";

test.use({ storageState: ownerState, reducedMotion: "reduce" });

const SCREENS: [string, string][] = [
  ["tong-quan", "/"],
  ["quay", "/quay"],
  ["khach-hang", "/khach-hang"],
  ["bao-cao", "/bao-cao"],
  ["so-doanh-thu", "/bao-cao?tab=ledger"],
  ["goi-lieu-trinh", "/goi-lieu-trinh"],
  ["khach-lau-khong-quay-lai", "/khach-lau-khong-quay-lai"],
  ["bang-gia", "/bang-gia"],
  ["bang-gia-goi", "/bang-gia/goi"],
  ["cai-dat", "/cai-dat"],
];

/** 320, 400, …, 1920: every layout step in between the breakpoints gets hit. */
const SWEEP = Array.from({ length: 21 }, (_, i) => 320 + i * 80);
const SHOTS = new Set([360, 400, 768, 1280, 1440]);
/** 1280×800 at 200 % browser zoom lays out like a 640×400 viewport. */
const ZOOM_200 = { width: 640, height: 400 };

let profilePath = "";
test.beforeAll(async ({ browser }) => {
  // A real profile page (breadcrumbs, gender chip, two columns) joins the sweep.
  const ctx = await browser.newContext({ storageState: ownerState, baseURL: stack.webOrigin });
  const res = await ctx.request.post("/api/v1/customers", {
    headers: { Origin: stack.webOrigin },
    data: { full_name: "Nguyễn Thị Bố Cục Rất Dài Tên Để Thử Xuống Dòng", phone: uniquePhone(), gender: "female" },
  });
  expect(res.status()).toBe(201);
  profilePath = `/khach-hang/${((await res.json()) as { id: string }).id}`;
  await ctx.close();
});

async function open(page: Page, path: string) {
  await page.goto(path);
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await page.waitForLoadState("networkidle");
}

const screens = () => [...SCREENS, ["ho-so", profilePath] as [string, string]];

for (const [name, path] of SCREENS) {
  test(`hover and press move nothing else on ${name}`, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page, path);
    const controls = await interactiveSample(page);
    expect(controls.length).toBeGreaterThan(0);
    for (const control of controls) {
      const label = (await control.getAttribute("aria-label")) ?? (await control.innerText()).slice(0, 30);
      const shifts = await shiftsOnInteraction(page, control);
      expect(shifts, `hover/press on "${label}" moved other elements`).toEqual([]);
    }
  });
}

test("every screen holds from 320 to 1920 px and at 200 % zoom", async ({ page }, testInfo) => {
  test.setTimeout(10 * 60_000);
  const problems: string[] = [];
  for (const [name, path] of screens()) {
    const sizes = [...SWEEP.map((width) => ({ width, height: 900 })), ZOOM_200];
    let first = true;
    for (const size of sizes) {
      await page.setViewportSize(size);
      // Load once per screen, then resize like a user dragging the window (plus one reload at 200 %).
      if (first || size === ZOOM_200) await open(page, path);
      else await page.waitForTimeout(60);
      first = false;
      const at = `${name} @ ${String(size.width)}×${String(size.height)}`;
      const overflow = await sidewaysOverflow(page);
      if (overflow > 0) problems.push(`${at}: scrolls sideways by ${String(overflow)}px`);
      for (const o of await overlappingSiblings(page)) problems.push(`${at}: overlap ${o}`);
      for (const c of await clippedText(page)) problems.push(`${at}: clipped ${c}`);
      if (SHOTS.has(size.width) && size.height === 900) {
        await testInfo.attach(`${name}-${String(size.width)}`, {
          body: await page.screenshot({ fullPage: true }),
          contentType: "image/png",
        });
      }
    }
  }
  expect(problems).toEqual([]);
});

/**
 * Phones get their own shapes (EM round 4 item 3, design-system § Mobile patterns): a 5-tab bar,
 * filters behind "Bộ lọc" in a bottom sheet glued to the bottom edge, cards instead of tables,
 * and 44 px targets for every control the thumb uses.
 */
test("phones: tab bar, filter sheet, cards instead of tables, 44 px targets", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await open(page, "/khach-lau-khong-quay-lai?follow=all&days=7");

  const tabs = page.getByRole("navigation", { name: "Thanh điều hướng" });
  await expect(tabs.getByRole("link")).toHaveText(["Trang chủ", "Quầy", "Khách", "Báo cáo"]);
  await expect(tabs.getByRole("button", { name: "Thêm màn hình" })).toBeVisible();

  // Search stays on the page; the other filters move into a sheet.
  const filters = page.getByRole("search", { name: "Lọc khách vắng" });
  await expect(filters.getByRole("searchbox")).toBeVisible();
  await expect(page.getByLabel("Vắng từ")).toHaveCount(0);
  await filters.getByRole("button", { name: /^Bộ lọc/ }).click();
  const sheet = page.getByRole("dialog", { name: "Lọc khách vắng" });
  await expect(sheet.getByLabel("Vắng từ")).toBeVisible();
  const box = await sheet.boundingBox();
  expect(box && Math.round(box.y + box.height)).toBe(844);
  expect(box?.width).toBe(390);
  await sheet.getByLabel("Vắng từ").selectOption("14");
  await expect(filters.getByRole("button", { name: "Bộ lọc, 1 đang chọn" })).toBeAttached();
  await sheet.getByRole("button", { name: "Xóa bộ lọc" }).click();
  await sheet.getByRole("button", { name: "Xem kết quả" }).click();
  await expect(sheet).toBeHidden();
  await expect(page).not.toHaveURL(/days=14/);

  // No wide table anywhere on the management lists: cards (a named list) instead.
  for (const path of ["/khach-lau-khong-quay-lai?follow=all", "/goi-lieu-trinh?status=all", "/bao-cao?tab=ledger", "/bao-cao"]) {
    await open(page, path);
    await expect(page.getByRole("table"), `${path} shows a table on a phone`).toHaveCount(0);
  }

  // Thumb controls are at least 44 px tall (WCAG 2.5.5, HIG 44 pt).
  for (const [name, path] of screens()) {
    await open(page, path);
    const small = await page.evaluate(() =>
      Array.from(document.querySelectorAll<HTMLElement>(".btn, .tab, .filter-trigger, .icon-btn, main select, main input:not([type=checkbox], [type=radio], [type=hidden], [type=date])"))
        .filter((el) => el.offsetParent !== null)
        .map((el) => [el.getAttribute("aria-label") ?? el.textContent.trim().slice(0, 30), Math.round(el.getBoundingClientRect().height)] as const)
        .filter(([, h]) => h < 44)
        .map(([label, h]) => `${label} (${String(h)}px)`),
    );
    expect(small, `${name}: controls under 44 px`).toEqual([]);
  }
});

test("focus rings never cover neighbouring text", async ({ page }) => {
  test.setTimeout(5 * 60_000);
  const problems: string[] = [];
  for (const width of [1280, 360]) {
    await page.setViewportSize({ width, height: 900 });
    for (const [name, path] of screens()) {
      await open(page, path);
      for (const p of await focusRingsOverText(page)) problems.push(`${name} @ ${String(width)}: ${p}`);
    }
  }
  expect(problems).toEqual([]);
});
