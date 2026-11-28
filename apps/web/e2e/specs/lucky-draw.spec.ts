import { expect, test, type Browser, type Locator, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { ownerState } from "../stack";
import { WRITE } from "../support/fixtures";
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

/** Opens one tab of the side column ("Giải thưởng", "Trúng thưởng", "Người tham gia"). */
async function openTab(page: Page, label: string): Promise<void> {
  const tab = page.getByRole("tab", { name: new RegExp(`^${label}`) });
  await tab.click();
  await expect(tab).toHaveAttribute("aria-selected", "true");
}

/**
 * The desktop layout (EM 2026-11-28): the page fits the screen (no page scroll), the wheel takes
 * the height it is given, and a list scrolls inside its panel.
 */
async function fitsTheScreen(page: Page): Promise<{ pageScroll: number; wheelOfHeight: number }> {
  return page.evaluate(() => {
    const wheel = document.querySelector(".ld-stage .ld-wheel")?.getBoundingClientRect();
    const se = document.scrollingElement ?? document.documentElement;
    return { pageScroll: se.scrollHeight - innerHeight, wheelOfHeight: wheel ? wheel.height / innerHeight : 0 };
  });
}

/** A spin through the API (no wheel): the flow needs five spins, the wheel is checked on three. */
async function spinByApi(page: Page, drawId: string, prizeName: string): Promise<void> {
  const draw = (await (await page.request.get(`/api/v1/lucky-draws/${drawId}`)).json()) as { prizes: { id: string; name: string }[] };
  const prize = draw.prizes.find((p) => p.name === prizeName);
  expect(prize, prizeName).toBeDefined();
  const res = await page.request.post(`/api/v1/lucky-draws/${drawId}/spins`, {
    headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
    data: { prize_id: prize?.id },
  });
  expect(res.status()).toBe(201);
}

const winner = (page: Page) => page.getByRole("dialog", { name: "Chúc mừng" });
const disk = (page: Page) => page.getByRole("img", { name: "Vòng quay tám cánh sen" }).locator(".ld-wheel-disk");
const angleOf = (page: Page) => disk(page).evaluate((el) => getComputedStyle(el).transform);
/** The page's worst vertical overflow over `ms` of a turn, read every frame. */
const overflowWhileTurning = (page: Page, ms: number) =>
  page.evaluate(
    (ms) =>
      new Promise<number>((resolve) => {
        const se = document.scrollingElement ?? document.documentElement;
        let worst = se.scrollHeight - innerHeight;
        const t0 = performance.now();
        const tick = () => {
          worst = Math.max(worst, se.scrollHeight - innerHeight);
          if (performance.now() - t0 < ms) requestAnimationFrame(tick);
          else resolve(worst);
        };
        requestAnimationFrame(tick);
      }),
    ms,
  );

/**
 * Presses `press` (the stage's "Quay" or the popup's "Quay tiếp") and checks, in the real browser,
 * that the wheel's disk turns while the popup waits; then waits for spin number `seq`'s popup and
 * returns the winner's name. `probeMs`: when to sample the turn (shorter for the gentle spin). A
 * spin lasts 6 s (angles.ts `SPIN_MS`), so the popup gets 15 s. `fits`: the page fits the screen,
 * and must keep fitting every frame of the turn (EM "giật giật": the turned square disk once pushed
 * a scrollbar in and out, and the whole page jumped sideways).
 */
async function spinOnce(page: Page, press: Locator, seq: number, probeMs = 700, fits = false): Promise<string> {
  const before = await angleOf(page);
  await press.click();
  if (fits) expect(await overflowWhileTurning(page, probeMs), "page overflow while the wheel turns").toBeLessThanOrEqual(0);
  else await page.waitForTimeout(probeMs);
  const a = await angleOf(page);
  await page.waitForTimeout(probeMs / 2);
  const b = await angleOf(page);
  expect(a, "the disk has left its resting angle").not.toBe(before);
  expect(b, "the disk is still turning").not.toBe(a);
  // Still turning: no winner yet.
  await expect(winner(page)).toBeHidden();
  await expect(page.getByText("Đang quay…").first()).toBeVisible();
  await expect(winner(page)).toBeVisible({ timeout: 15_000 });
  await expect(winner(page).getByText(`Lượt ${String(seq)}`, { exact: true })).toBeVisible();
  return (await winner(page).locator(".ld-winner-name").innerText()).trim();
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
    test.setTimeout(150_000);
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

    const drawId = new URL(page.url()).pathname.split("/").pop() ?? "";

    // US2: paste six people from Excel (the "Người tham gia" tab).
    await openTab(page, "Người tham gia");
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

    // US3: one prize for women only, one for anyone (the "Giải thưởng" tab).
    await openTab(page, "Giải thưởng");
    await addPrize(page, "Gội đầu E2E", "Chỉ nữ", 2);
    await addPrize(page, "Cổ vai gáy E2E", "Cả nam và nữ", 2);
    await expect(prizes(page).getByRole("button", { name: /^Gội đầu E2E/ })).toHaveAttribute("aria-pressed", "true");
    await expectAccessible(page, "lucky-draw-ready");
    // EM 2026-11-28: the wheel is the page; at the EM's screen nothing scrolls but the lists.
    await page.setViewportSize({ width: 1536, height: 730 });
    for (const tab of ["Trúng thưởng", "Người tham gia", "Giải thưởng"]) {
      await openTab(page, tab);
      const fit = await fitsTheScreen(page);
      expect(fit.pageScroll, `page scroll on "${tab}"`).toBeLessThanOrEqual(0);
      expect(fit.wheelOfHeight).toBeGreaterThanOrEqual(0.6);
    }
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "lucky-draw-ready-dark");
    await page.emulateMedia({ colorScheme: "light" });

    // US4: every press turns the wheel (EM: the spin is the content); the winner pops up once it stops.
    // At the EM's window (1920 × 912), where the wheel is as tall as the stage and its turned square
    // disk reached past the page's end: nothing may move but the wheel (EM "giật giật").
    await page.setViewportSize({ width: 1920, height: 912 });
    const first = await spinOnce(page, stage(page).getByRole("button", { name: "Quay" }), 1, 1000, true);
    await page.setViewportSize({ width: 1280, height: 900 });
    expect(WOMEN).toContain(first);
    await expect(winner(page).getByRole("button", { name: "Quay tiếp" })).toBeFocused();
    // Not under the wheel: only the popup shows the winner.
    await expect(stage(page).getByText(first, { exact: true })).toHaveCount(0);
    await expectAccessible(page, "lucky-draw-popup");
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "lucky-draw-popup-dark");
    await page.emulateMedia({ colorScheme: "light" });

    await winner(page).getByRole("button", { name: "Đóng", exact: true }).click();
    await expect(winner(page)).toBeHidden();

    // Spins 2 and 3 through the API (each wheel spin takes 6 s); the page shows them once reloaded.
    await spinByApi(page, drawId, "Gội đầu E2E");
    await spinByApi(page, drawId, "Cổ vai gáy E2E");
    await page.reload();
    await expect(stage(page).getByText("Cổ vai gáy E2E", { exact: true })).toBeVisible();

    // Reduced motion still spins, gentler (3 turns in the same 6 s); on a phone.
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.setViewportSize({ width: 390, height: 844 });
    await spinOnce(page, stage(page).getByRole("button", { name: "Quay tiếp" }), 4, 500);
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "lucky-draw-popup-390");
    await page.setViewportSize({ width: 1280, height: 900 });

    // US6: the spin that gives out the last slot offers the summary; the women-only prize went to women.
    await expect(winner(page).getByRole("button", { name: "Quay tiếp" })).toHaveCount(0);
    await winner(page).getByRole("button", { name: "Xem tổng kết" }).click();
    await expect(winner(page)).toBeHidden();
    const summary = page.getByRole("dialog", { name: "Tổng kết chương trình" });
    await expect(summary).toBeVisible();
    await expect(summary.getByText("4 suất đã trao", { exact: false })).toBeVisible();
    const womenOnly = summary.getByRole("region", { name: "Gội đầu E2E" });
    await expect(womenOnly.getByRole("listitem")).toHaveCount(2);
    for (const chip of await womenOnly.locator(".ld-win .gender-chip").allInnerTexts()) expect(chip).toBe("Nữ");
    await expectAccessible(page, "lucky-draw-summary");
    const [download] = await Promise.all([page.waitForEvent("download"), summary.getByRole("button", { name: "Tải danh sách (CSV)" }).click()]);
    const csv = readFileSync(await download.path(), "utf8");
    expect(csv.startsWith('﻿"Lượt","Họ tên","Giới tính","Giải thưởng","Lúc quay","Trạng thái"')).toBe(true);
    expect(csv.trim().split("\r\n")).toHaveLength(5);
    await summary.getByRole("button", { name: "Đóng", exact: true }).last().click();
    await expect(stage(page).getByRole("button", { name: "Quay tiếp" })).toHaveAttribute("aria-disabled", "true");

    // The winners list ("Trúng thưởng" tab): cards grouped by prize.
    await openTab(page, "Trúng thưởng");
    const anyone = page.getByRole("list", { name: "Người trúng giải Cổ vai gáy E2E" });
    await expect(anyone.getByRole("listitem")).toHaveCount(2);
    await expectAccessible(page, "lucky-draw-winners");
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "lucky-draw-winners-dark");
    await page.emulateMedia({ colorScheme: "light" });

    // US5: a winner of the prize for anyone is absent; the slot comes back and goes to someone else.
    const absentRow = anyone.getByRole("listitem").first();
    const absent = (await absentRow.locator(".ld-win-who").innerText()).trim();
    await absentRow.getByRole("button", { name: /^Thao tác với lượt/ }).click();
    await page.getByRole("menuitem", { name: "Vắng mặt, trả giải" }).click();
    const forfeit = page.getByRole("dialog", { name: /^Trả giải “Cổ vai gáy E2E” của/ });
    await expect(forfeit).toContainText("Suất này quay lại cho lượt sau");
    await forfeit.getByRole("button", { name: "Vắng mặt, trả giải" }).click();
    await expect(forfeit).toBeHidden();
    await expect(anyone.getByRole("listitem").filter({ hasText: absent })).toContainText("Vắng mặt");
    await openTab(page, "Giải thưởng");
    await expect(prizes(page).getByRole("button", { name: /^Cổ vai gáy E2E/ })).toContainText("Còn 1/2 suất");

    // US7: the presentation; the popup opens over it, TV size; then both close.
    const present = page.getByRole("button", { name: "Trình chiếu" });
    await present.click();
    const show = page.getByRole("dialog", { name });
    await expect(show.getByRole("button", { name: "Thoát trình chiếu" })).toBeVisible();
    await expectAccessible(page, "lucky-draw-presentation");
    await expect(show.getByRole("combobox", { name: "Giải đang quay" })).toHaveValue(/.+/);
    const again = await spinOnce(page, show.getByRole("button", { name: "Quay tiếp" }), 5, 500);
    expect(again).not.toBe(absent);
    await expect(winner(page)).toHaveClass(/is-big/);
    await expectAccessible(page, "lucky-draw-presentation-popup");
    await winner(page).getByRole("button", { name: "Đóng", exact: true }).click();
    await expect(winner(page)).toBeHidden();
    await show.getByRole("button", { name: "Thoát trình chiếu" }).click();
    await expect(present).toBeFocused();

    // Phones: one column, nothing sideways.
    await page.setViewportSize({ width: 390, height: 844 });
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "lucky-draw-390");
    await page.setViewportSize({ width: 1024, height: 900 });
    expect(await sidewaysOverflow(page)).toBe(0);
    await page.setViewportSize({ width: 1280, height: 900 });

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
