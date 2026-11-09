import { expect, test } from "@playwright/test";
import { ownerState } from "../stack";
import { sidewaysOverflow } from "../support/responsive";
import { expectAccessible } from "../support/run";

test.use({ storageState: ownerState });

/** Staff management (spec staff-management): the owner's key flows end to end on the real API. */
test.describe("nhân viên", () => {
  test("profile with pay → attendance with lateness → bonus → payroll and payslip", async ({ page }) => {
    const name = `KTV Hồng ${String(Date.now()).slice(-5)}`;
    await page.goto("/nhan-vien?tab=profiles");
    await expect(page.getByRole("heading", { level: 1, name: "Quản lý nhân sự" })).toBeVisible();
    await page.getByRole("button", { name: "Thêm nhân viên" }).click();
    const profile = page.getByRole("dialog", { name: "Thêm nhân viên" });
    await profile.getByLabel("Họ tên").fill(name);
    await profile.getByLabel("Lương cứng thực nhận / tháng (đ)").fill("5000000");
    await expectAccessible(page, "staff-profile-form");
    await profile.getByRole("button", { name: "Lưu" }).click();
    await expect(profile).toBeHidden();
    await expect(page.getByRole("button", { name })).toBeVisible();

    // Daily book: one card per person — times in the sheet, the bonus inline, progress on top.
    await page.getByRole("tab", { name: "Sổ hằng ngày" }).click();
    await expect(page.getByRole("status", { name: "Tiến độ chấm công" })).toBeVisible();
    const card = page.getByRole("listitem").filter({ has: page.getByText(name, { exact: true }) });
    // Attendance in words, one tap saves; arrows move focus only, Space records. The chosen option
    // is the filled radio (no separate status chip).
    const choice = card.getByRole("radiogroup", { name: `Chấm công ${name}` });
    await expect(card.getByText("Chưa chấm công")).toBeVisible();
    await choice.getByRole("radio", { name: "Nghỉ có phép" }).click();
    await expect(choice.getByRole("radio", { name: "Nghỉ có phép" })).toHaveAttribute("aria-checked", "true");
    await expect(card.getByText("Chưa chấm công")).toBeHidden();
    // Taps wait for the running save (and the refetch behind it) to finish.
    await expect(choice).not.toHaveAttribute("aria-busy");
    await choice.getByRole("radio", { name: "Nghỉ có phép" }).press("ArrowLeft");
    await expect(choice.getByRole("radio", { name: "Đi làm" })).toBeFocused();
    await expect(choice.getByRole("radio", { name: "Nghỉ có phép" })).toHaveAttribute("aria-checked", "true");
    await page.keyboard.press("Space");
    await expect(choice.getByRole("radio", { name: "Đi làm" })).toHaveAttribute("aria-checked", "true");
    await card.getByRole("button", { name: "Giờ & ghi chú" }).click();
    const day = page.getByRole("dialog", { name });
    await day.getByLabel("Giờ đến", { exact: true }).fill("09:20");
    await day.getByRole("button", { name: "Lưu" }).click();
    await expect(day).toBeHidden();
    await expect(card).toContainText("Đi muộn 20 phút");
    await expect(card.getByRole("radio", { name: "Đi làm" })).toHaveAttribute("aria-checked", "true");
    await card.getByRole("button", { name: "Ghi thưởng / phạt" }).click();
    const adj = page.getByRole("dialog", { name: `Thưởng / phạt · ${name}` });
    await adj.getByLabel("Số tiền (đ)").fill("100000");
    await adj.getByLabel("Lý do").fill("Khách khen đích danh");
    await adj.getByRole("button", { name: "Lưu" }).click();
    await expect(adj).toBeHidden();
    await expect(card).toContainText("Khách khen đích danh");
    await expectAccessible(page, "staff-daily-book");

    // Month sheet: the grid, then the month's bonus log.
    await page.getByRole("tab", { name: "Bảng công tháng" }).click();
    await expect(page.getByRole("table", { name: /Bảng chấm công/ })).toBeVisible();
    await expect(page.getByRole("button", { name: new RegExp(`^${name}, .*muộn 20 phút`) })).toBeVisible();
    // Scoped to this person's row: a retry adds a second "Khách khen đích danh" to the same month.
    await expect(page.getByRole("row").filter({ hasText: name }).getByText("Khách khen đích danh")).toBeVisible();

    // Payroll: live payslip with every line.
    await page.getByRole("tab", { name: "Bảng lương" }).click();
    await expect(page.getByText("Tạm tính")).toBeVisible();
    await page.getByRole("button", { name }).click();
    const slip = page.getByRole("dialog", { name: `Phiếu lương ${name}` });
    await expect(slip.getByText("Thực lĩnh")).toBeVisible();
    await expect(slip.getByText("Muộn 1 lần, 20 phút")).toBeVisible();
    await expect(slip).toContainText("100.000");
    await expectAccessible(page, "staff-payslip");
  });

  // EM 2026-11-07 (image 52): a person's card was far taller than its content and "Khen" sat apart
  // from its stepper. EM 2026-11-07 (image 65): one card per person in a grid (2 columns at 1280),
  // the four attendance choices as separate tiles without P/ĐX/K badges.
  test("desktop: one card per person in a grid in the daily book", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto("/nhan-vien");
    const book = page.getByRole("list", { name: /^Sổ ngày/ });
    const cards = book.locator(":scope > li");
    const row = cards.first();
    await expect(row.getByRole("radiogroup")).toBeVisible();
    if ((await cards.count()) > 1) {
      const lefts = await cards.evaluateAll((els) => els.slice(0, 3).map((el) => Math.round(el.getBoundingClientRect().left)));
      expect(new Set(lefts).size, "columns at 1280").toBe(2);
    }
    const tiles = await row.getByRole("radio").evaluateAll((els) => els.map((el) => el.getBoundingClientRect().toJSON() as DOMRect));
    for (const t of tiles) expect(t.height).toBeGreaterThanOrEqual(44);
    // Spaced apart, not one strip.
    const [a, b] = tiles;
    if (a && b) expect(b.left - a.right).toBeGreaterThanOrEqual(8);
    expect(await row.getByRole("radio").allTextContents()).toEqual(["Đi làm", "Nghỉ có phép", "Nghỉ đột xuất", "Nghỉ không phép"]);
    for (const group of await row.getByRole("group").all()) {
      const gap = await group.evaluate((el) => {
        const label = el.firstElementChild?.getBoundingClientRect();
        const box = el.lastElementChild?.getBoundingClientRect();
        return label && box ? Math.round(box.left - label.right) : -1;
      });
      expect(gap, "label → stepper").toBe(8);
    }
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "staff-daily-book-cards");
  });

  test("phones: daily book and month grid fit 390 px without sideways page scroll", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/nhan-vien");
    await expect(page.getByRole("tab", { name: "Sổ hằng ngày" })).toBeVisible();
    await expect(page.getByLabel("Ngày", { exact: true })).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    // The four attendance options keep their words on a phone: 2 × 2, every target ≥ 44 px.
    const choice = page.getByRole("radiogroup").first();
    await expect(choice.getByRole("radio", { name: "Nghỉ không phép" })).toContainText("Nghỉ không phép");
    const boxes = await choice.getByRole("radio").evaluateAll((els) => els.map((el) => el.getBoundingClientRect().toJSON() as DOMRect));
    expect(new Set(boxes.map((b) => Math.round(b.top))).size).toBe(2);
    for (const b of boxes) expect(b.height).toBeGreaterThanOrEqual(44);
    await expectAccessible(page, "staff-phone-daily-book");
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "staff-phone-daily-book-dark");
    await page.emulateMedia({ colorScheme: "light" });
    await page.getByRole("tab", { name: "Bảng công tháng" }).click();
    // The grid scrolls inside its own box; the page itself never scrolls sideways.
    await expect(page.getByRole("table", { name: /Bảng chấm công/ })).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "staff-phone-month");
    await page.getByRole("tab", { name: "Bảng lương" }).click();
    await expect(page.getByText("Tạm tính")).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "staff-phone-payroll");
  });

  // EM 2026-11-07 item 9 (spec §13): hire → end employment with a reason → "Đã nghỉ" → rehire.
  for (const [label, viewport] of [
    ["desktop", { width: 1280, height: 800 }],
    ["phone", { width: 390, height: 844 }],
  ] as const) {
    test(`roster (${label}): hire, end employment, find under Đã nghỉ, rehire`, async ({ page }) => {
      await page.setViewportSize(viewport);
      const name = `KTV Nghỉ ${label} ${String(Date.now()).slice(-5)}`;
      await page.goto("/nhan-vien?tab=profiles");
      await expect(page.getByRole("tab", { name: "Hồ sơ nhân viên" })).toHaveAttribute("aria-selected", "true");
      await page.getByRole("button", { name: "Thêm nhân viên" }).click();
      const form = page.getByRole("dialog", { name: "Thêm nhân viên" });
      await form.getByLabel("Họ tên").fill(name);
      await form.getByRole("button", { name: "Lưu hồ sơ" }).click();
      await expect(form).toBeHidden();
      const row = page.getByRole("button", { name, exact: true });
      await expect(row).toBeVisible();
      await expect(page.getByRole("button", { name: /^Đang làm \(\d+\)$/ })).toHaveAttribute("aria-pressed", "true");
      expect(await sidewaysOverflow(page)).toBe(0);
      await expectAccessible(page, `roster-${label}`);

      await row.click();
      const sheet = page.getByRole("dialog", { name });
      await expect(sheet.getByRole("heading", { name: "Quá trình làm việc" })).toBeVisible();
      await sheet.getByRole("button", { name: "Cho nghỉ việc" }).click();
      const confirm = page.getByRole("dialog", { name: `Cho ${name} nghỉ việc` });
      await expect(confirm.getByRole("button", { name: "Xác nhận nghỉ việc" })).toBeDisabled();
      await confirm.getByLabel("Lý do nghỉ việc").fill("Chuyển về quê");
      await expectAccessible(page, `roster-end-${label}`);
      await confirm.getByRole("button", { name: "Xác nhận nghỉ việc" }).click();
      // ADR-0019 D20: done means back to the roster, not to the person's sheet.
      await expect(confirm).toBeHidden();
      await expect(page.getByRole("dialog", { name })).toBeHidden();

      // Out of "Đang làm" and of the front desk's KTV picker, in "Đã nghỉ".
      await expect(page.getByRole("button", { name, exact: true })).toBeHidden();
      await page.getByRole("button", { name: /^Đã nghỉ \(\d+\)$/ }).click();
      await expect(page.getByRole("button", { name, exact: true })).toBeVisible();
      await page.emulateMedia({ colorScheme: "dark" });
      await expectAccessible(page, `roster-left-${label}-dark`);
      await page.emulateMedia({ colorScheme: "light" });
      const picker = await page.request.get("/api/v1/staff");
      expect(((await picker.json()) as { full_name: string }[]).some((s) => s.full_name === name)).toBe(false);

      // Rehire: a second period; back under "Đang làm".
      await page.getByRole("button", { name, exact: true }).click();
      const left = page.getByRole("dialog", { name });
      await expect(left.getByText("Nghỉ: Chuyển về quê")).toBeVisible();
      await left.getByRole("button", { name: "Nhận lại làm" }).click();
      const rehire = page.getByRole("dialog", { name: `Nhận ${name} lại làm` });
      await rehire.getByRole("button", { name: "Nhận lại làm" }).click();
      const back = page.getByRole("dialog", { name });
      await expect(back.getByRole("button", { name: "Cho nghỉ việc" })).toBeVisible();
      await expect(back.getByRole("listitem").filter({ hasText: "Nghỉ: Chuyển về quê" })).toBeVisible();
      await expect(back.getByRole("heading", { name: "Lịch sử thay đổi" })).toBeVisible();
      await expect(back.getByText("Nhận lại làm", { exact: true })).toBeVisible();
    });
  }

  // EM 2026-11-08: a person's evaluations across months, and the days behind a month.
  test("evaluation history: a name opens the person's months, then that month's days (1280 / 390)", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto("/nhan-vien?tab=evaluation");
    await page.getByRole("button", { name: /: lịch sử đánh giá các tháng$/ }).first().click();
    await expect(page.getByRole("heading", { name: /^Lịch sử đánh giá · / })).toBeVisible();
    await expect(page).toHaveURL(/staff=/);
    await expect(page.getByRole("list", { name: /^Các tháng của / })).toBeVisible();
    await expectAccessible(page, "evaluation-history");
    await page.setViewportSize({ width: 390, height: 844 });
    expect(await sidewaysOverflow(page)).toBe(0);
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "evaluation-history-390-dark");
    await page.emulateMedia({ colorScheme: "light" });
    await page.getByRole("link", { name: /^Từng ngày / }).first().click();
    await expect(page.getByRole("region", { name: /^Từng ngày · / })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Xem cả spa thay vì / })).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "person-days-390");
  });
});
