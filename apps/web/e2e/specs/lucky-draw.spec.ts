import { expect, test, type Browser, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { ownerState } from "../stack";
import { sidewaysOverflow } from "../support/responsive";
import { credentials, expectAccessible } from "../support/run";

test.use({ storageState: ownerState });

/** 3 women, 3 men, pasted the way Excel's two columns arrive (tab), with the header line. */
const ROSTER = [
  "Họ tên\tGiới tính",
  "Nguyễn Thị E2E Lan\tNữ",
  "Trần Văn E2E Minh\tNam",
  "Lê Thu E2E Hà\tnu",
  "Phạm Quốc E2E Bảo\tNAM",
  "Đỗ Ngọc E2E Mai\tNữ",
  "Võ Thành E2E Nam\tNam",
].join("\n");
const WOMEN = ["Nguyễn Thị E2E Lan", "Lê Thu E2E Hà", "Đỗ Ngọc E2E Mai"];

const stage = (page: Page) => page.getByRole("region", { name: "Vòng quay" });
const prizes = (page: Page) => page.getByRole("list", { name: "Chọn giải để quay" });

async function addPrize(page: Page, name: string, audience: "Cả nam và nữ" | "Chỉ nữ", slots: number): Promise<void> {
  await page.getByRole("button", { name: "Thêm giải thưởng" }).click();
  const sheet = page.getByRole("dialog", { name: "Thêm giải thưởng" });
  await sheet.getByLabel("Tên giải").fill(name);
  await sheet.getByRole("button", { name: audience }).click();
  await sheet.getByLabel("Số suất").fill(String(slots));
  await sheet.getByRole("button", { name: "Thêm giải thưởng" }).click();
  await expect(sheet).toBeHidden();
  await expect(prizes(page).getByRole("button", { name: new RegExp(`^${name}`) })).toContainText(`Còn ${String(slots)}/${String(slots)} suất`);
}

/** Presses "Quay" / "Quay tiếp" and waits for the wheel to stop on spin number `seq`'s winner. */
async function spinOnce(page: Page, seq: number): Promise<string> {
  const button = stage(page).getByRole("button", { name: /^Quay/ });
  await button.click();
  // The slow-down lasts ~4 s; the winner shows only after it.
  await expect(stage(page).locator(".ld-result-label")).toHaveText(`Người trúng · lượt ${String(seq)}`, { timeout: 10_000 });
  await expect(button).not.toHaveAttribute("aria-busy", "true");
  return (await stage(page).locator(".ld-result-name").innerText()).split(",")[0]?.trim() ?? "";
}

async function signInAsManager(browser: Browser): Promise<Page> {
  const run = credentials();
  const context = await browser.newContext({ storageState: { cookies: [], origins: [] } });
  const page = await context.newPage();
  await page.goto("/dang-nhap");
  await page.getByLabel("Số điện thoại").fill(run.managerPhone ?? "");
  await page.getByLabel("Mật khẩu").fill(run.managerPassword ?? "");
  await page.getByRole("button", { name: "Đăng nhập" }).click();
  await expect(page).not.toHaveURL(/\/dang-nhap/);
  return page;
}

/** "Vòng quay may mắn" (spec lucky-draw, architecture/lucky-draw.md §4) on the real API. */
test.describe("vòng quay may mắn", () => {
  test("the owner runs a draw to the end, forfeits a winner, spins again, then deletes it", async ({ page }) => {
    test.setTimeout(120_000);
    const name = `Tri ân E2E ${String(Date.now()).slice(-6)}`;
    await page.setViewportSize({ width: 1280, height: 900 });

    // US1: the list, then a new program that opens at once.
    await page.goto("/vong-quay");
    await expect(page.getByRole("heading", { level: 1, name: "Vòng quay may mắn" })).toBeVisible();
    await expectAccessible(page, "lucky-draw-list");
    await page.getByRole("button", { name: "Tạo chương trình" }).click();
    const create = page.getByRole("dialog", { name: "Tạo chương trình" });
    await create.getByLabel("Tên chương trình").fill(name);
    await create.getByRole("button", { name: "Tạo chương trình" }).click();
    await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();
    await expect(stage(page).getByText("Thêm người tham gia và giải thưởng để bắt đầu quay.")).toBeVisible();
    await expectAccessible(page, "lucky-draw-empty");

    // US2: paste six people from Excel.
    await page.getByRole("button", { name: "Dán danh sách" }).click();
    const paste = page.getByRole("dialog", { name: "Dán danh sách người tham gia" });
    await paste.getByRole("textbox", { name: "Danh sách" }).fill(`${ROSTER}\nKhông giới tính`);
    await expect(paste.getByText("Dòng 8: thiếu giới tính — ví dụ: Trần Văn Minh, Nam")).toBeVisible();
    await paste.getByRole("textbox", { name: "Danh sách" }).fill(ROSTER);
    await expect(paste.getByText("Sẽ thêm 6 người (3 nữ, 3 nam).")).toBeVisible();
    await expectAccessible(page, "lucky-draw-paste");
    await paste.getByRole("button", { name: "Thêm 6 người" }).click();
    await expect(paste).toBeHidden();
    const chips = page.getByRole("group", { name: "Lọc người tham gia" });
    await expect(chips.getByRole("button", { name: "Nữ 3" })).toBeVisible();

    // US3: one prize for women only, one for anyone.
    await addPrize(page, "Gội đầu E2E", "Chỉ nữ", 2);
    await addPrize(page, "Cổ vai gáy E2E", "Cả nam và nữ", 2);
    await expect(prizes(page).getByRole("button", { name: /^Gội đầu E2E/ })).toHaveAttribute("aria-pressed", "true");
    await expectAccessible(page, "lucky-draw-ready");
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "lucky-draw-ready-dark");
    await page.emulateMedia({ colorScheme: "light" });

    // US4: the first spin with the wheel turning, the others without motion (same server path).
    const button = stage(page).getByRole("button", { name: "Quay" });
    const first = spinOnce(page, 1);
    // Busy at once, while the wheel turns and before the winner is shown.
    await expect(button).toHaveAttribute("aria-busy", "true");
    await expect(stage(page).locator(".ld-result-name")).toHaveCount(0);
    expect(WOMEN).toContain(await first);
    await expectAccessible(page, "lucky-draw-result");
    await page.emulateMedia({ reducedMotion: "reduce" });
    for (const seq of [2, 3, 4]) await spinOnce(page, seq);

    // US6: the last slot opens the summary; the women-only prize went to women.
    const summary = page.getByRole("dialog", { name: "Tổng kết chương trình" });
    await expect(summary).toBeVisible();
    await expect(summary.getByText("4 suất đã trao", { exact: false })).toBeVisible();
    const womenOnly = summary.getByRole("region", { name: "Gội đầu E2E" });
    await expect(womenOnly.getByRole("listitem")).toHaveCount(2);
    for (const item of await womenOnly.getByRole("listitem").allInnerTexts()) expect(item).toContain("(Nữ)");
    await expectAccessible(page, "lucky-draw-summary");
    const [download] = await Promise.all([page.waitForEvent("download"), summary.getByRole("button", { name: "Tải danh sách (CSV)" }).click()]);
    const csv = readFileSync(await download.path(), "utf8");
    expect(csv.startsWith("﻿\"Lượt\",\"Họ tên\",\"Giới tính\",\"Giải thưởng\",\"Lúc quay\",\"Trạng thái\"")).toBe(true);
    expect(csv.trim().split("\r\n")).toHaveLength(5);
    await summary.getByRole("button", { name: "Đóng", exact: true }).last().click();
    await expect(stage(page).getByRole("button", { name: "Quay tiếp" })).toHaveAttribute("aria-disabled", "true");

    // US5: a winner of the prize for anyone is absent; the slot comes back and goes to someone else.
    const winners = page.getByRole("table", { name: "Danh sách trúng thưởng" });
    const absentRow = winners.getByRole("row").filter({ hasText: "Cổ vai gáy E2E" }).first();
    const absent = (await absentRow.locator(".ld-who .font-semibold").innerText()).trim();
    await absentRow.getByRole("button", { name: /^Thao tác với lượt/ }).click();
    await page.getByRole("menuitem", { name: "Vắng mặt, trả giải" }).click();
    const forfeit = page.getByRole("dialog", { name: new RegExp(`^Trả giải “Cổ vai gáy E2E” của`) });
    await expect(forfeit).toContainText("Suất này quay lại cho lượt sau");
    await forfeit.getByRole("button", { name: "Vắng mặt, trả giải" }).click();
    await expect(forfeit).toBeHidden();
    await expect(winners.getByRole("row").filter({ hasText: absent })).toContainText("Vắng mặt");
    await expect(prizes(page).getByRole("button", { name: /^Cổ vai gáy E2E/ })).toContainText("Còn 1/2 suất");
    const again = await spinOnce(page, 5);
    expect(again).not.toBe(absent);
    await expect(page.getByRole("dialog", { name: "Tổng kết chương trình" })).toBeVisible();
    await page.keyboard.press("Escape");

    // Phones: one column, nothing sideways.
    await page.setViewportSize({ width: 390, height: 844 });
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "lucky-draw-390");
    await page.setViewportSize({ width: 1024, height: 900 });
    expect(await sidewaysOverflow(page)).toBe(0);
    await page.setViewportSize({ width: 1280, height: 900 });

    // US7: the presentation covers the screen; Esc gives focus back to its button.
    const present = page.getByRole("button", { name: "Trình chiếu" });
    await present.click();
    const show = page.getByRole("dialog", { name });
    await expect(show.getByRole("button", { name: "Thoát trình chiếu" })).toBeVisible();
    await expectAccessible(page, "lucky-draw-presentation");
    await show.getByRole("button", { name: "Thoát trình chiếu" }).click();
    await expect(present).toBeFocused();

    // US1: delete; the e2e leaves no program behind.
    await page.getByRole("button", { name: "Thao tác với chương trình" }).click();
    await page.getByRole("menuitem", { name: "Xóa chương trình" }).click();
    const remove = page.getByRole("dialog", { name: `Xóa chương trình “${name}”?` });
    await remove.getByRole("button", { name: "Xóa chương trình" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Vòng quay may mắn" })).toBeVisible();
    await expect(page.getByRole("link", { name })).toHaveCount(0);
  });

  test("a manager has no nav item, and the URL shows the app's 403 page", async ({ browser }) => {
    const page = await signInAsManager(browser);
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto("/");
    await expect(page.getByRole("navigation").first()).toBeVisible();
    await expect(page.getByRole("link", { name: "Vòng quay may mắn" })).toHaveCount(0);
    await page.goto("/vong-quay");
    await expect(page.getByRole("heading", { name: "Không có quyền mở màn hình này" })).toBeVisible();
    const api = await page.request.get("/api/v1/lucky-draws");
    expect(api.status()).toBe(403);
    await page.context().close();
  });
});
