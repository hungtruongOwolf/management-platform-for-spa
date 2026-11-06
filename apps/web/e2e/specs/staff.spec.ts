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
    await expect(page.getByRole("heading", { level: 1, name: "Nhân viên" })).toBeVisible();
    await page.getByRole("button", { name: "+ Thêm nhân viên" }).click();
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
    await choice.getByRole("radio", { name: "Nghỉ có phép (P)" }).click();
    await expect(choice.getByRole("radio", { name: "Nghỉ có phép (P)" })).toHaveAttribute("aria-checked", "true");
    await expect(card.getByText("Chưa chấm công")).toBeHidden();
    // Taps wait for the running save (and the refetch behind it) to finish.
    await expect(choice).not.toHaveAttribute("aria-busy");
    await choice.getByRole("radio", { name: "Nghỉ có phép (P)" }).press("ArrowLeft");
    await expect(choice.getByRole("radio", { name: "Đi làm" })).toBeFocused();
    await expect(choice.getByRole("radio", { name: "Nghỉ có phép (P)" })).toHaveAttribute("aria-checked", "true");
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
    await expect(page.getByText("Khách khen đích danh")).toBeVisible();

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

  test("phones: daily book and month grid fit 390 px without sideways page scroll", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/nhan-vien");
    await expect(page.getByRole("tab", { name: "Sổ hằng ngày" })).toBeVisible();
    await expect(page.getByLabel("Ngày", { exact: true })).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    // The four attendance options keep their words on a phone: 2 × 2, every target ≥ 44 px.
    const choice = page.getByRole("radiogroup").first();
    await expect(choice.getByRole("radio", { name: "Nghỉ không phép (K)" })).toContainText("Nghỉ không phép");
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
});
