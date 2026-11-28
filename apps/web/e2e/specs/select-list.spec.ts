import { expect, test, type Locator, type Page } from "@playwright/test";
import { ownerState } from "../stack";

test.use({ storageState: ownerState });

/**
 * The open list of every select is the app's own (design-system.md "Danh sách mở của select"):
 * the customizable select, styled like the app's menus, its keys kept inside the list. jsdom has
 * no `appearance: base-select`, so this is the only layer that sees it.
 */
test.describe("danh sách mở của select", () => {
  /** The open list, measured from its rows (the picker itself is not in the DOM). */
  function list(select: Locator) {
    return select.evaluate((el: HTMLSelectElement) => {
      const rows = Array.from(el.querySelectorAll("option"), (o) => o.getBoundingClientRect());
      const probe = document.body.appendChild(document.createElement("div"));
      probe.style.background = "var(--primary-soft)";
      const primarySoft = getComputedStyle(probe).backgroundColor;
      probe.remove();
      const checked = el.querySelector("option:checked");
      return {
        open: el.matches(":open"),
        appearance: getComputedStyle(el).appearance,
        select: el.getBoundingClientRect().width,
        lowestRow: Math.min(...rows.map((r) => r.height)),
        left: Math.min(...rows.map((r) => r.left)),
        right: Math.max(...rows.map((r) => r.right)),
        checkedFill: checked ? getComputedStyle(checked).backgroundColor : "",
        primarySoft,
      };
    });
  }

  async function supported(page: Page) {
    return page.evaluate(() => CSS.supports("appearance", "base-select"));
  }

  test("desktop: the list reads as a menu of the app, Esc closes only the list, a pick filters", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/khach-lau-khong-quay-lai");
    test.skip(!(await supported(page)), "no customizable select: the browser's own list (Firefox)");
    const since = page.getByLabel("Vắng từ");
    await expect(since).toBeVisible();
    expect((await list(since)).appearance).toBe("base-select");

    await since.click();
    await expect.poll(async () => (await list(since)).open).toBe(true);
    // 44px rows (rulebook A), once the pop-in (scale .98 → 1) has settled.
    await expect.poll(async () => (await list(since)).lowestRow).toBeGreaterThanOrEqual(44);
    const open = await list(since);
    // At least as wide as the select and RowMenu's 200px (rows + 2 × 7px).
    expect(open.right - open.left + 14).toBeGreaterThanOrEqual(Math.max(open.select, 200) - 1);
    // The current value is the palette's chosen row.
    expect(open.checkedFill).toBe(open.primarySoft);

    await page.keyboard.press("Escape");
    await expect.poll(async () => (await list(since)).open).toBe(false);
    await expect(page.getByRole("heading", { level: 1, name: "Khách lâu chưa quay lại" })).toBeVisible();

    const other = await since.evaluate((el: HTMLSelectElement) => el.options[1]?.value ?? "");
    await since.click();
    await page.locator(`option[value="${other}"]`).first().click();
    await expect(since).toHaveValue(other);
    await expect.poll(async () => (await list(since)).open).toBe(false);
  });

  test("phone: in the filter sheet the list fits the screen with 44px rows; Esc closes the list, then the sheet", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/khach-lau-khong-quay-lai");
    test.skip(!(await supported(page)), "no customizable select: the browser's own list (Firefox)");
    await page.getByRole("button", { name: /^Bộ lọc/ }).click();
    const sheet = page.getByRole("dialog");
    const scope = sheet.getByLabel("Phạm vi");
    await scope.click();
    await expect.poll(async () => (await list(scope)).open).toBe(true);
    await expect.poll(async () => (await list(scope)).lowestRow).toBeGreaterThanOrEqual(44);
    const open = await list(scope);
    expect(open.left).toBeGreaterThanOrEqual(0);
    expect(open.right).toBeLessThanOrEqual(390);

    await page.keyboard.press("Escape");
    await expect.poll(async () => (await list(scope)).open).toBe(false);
    await expect(sheet).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(sheet).toBeHidden();
  });
});
