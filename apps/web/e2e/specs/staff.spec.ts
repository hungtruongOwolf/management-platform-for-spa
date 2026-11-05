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

    // Attendance: one tap for a code, the sheet for the times.
    await page.getByRole("tab", { name: "Chấm công" }).click();
    const card = page.getByRole("listitem").filter({ hasText: name });
    await card.getByRole("button", { name: "Giờ & ghi chú" }).click();
    const day = page.getByRole("dialog", { name: `Chấm công ${name}` });
    await day.getByLabel("Giờ đến", { exact: true }).fill("09:20");
    await day.getByRole("button", { name: "Lưu" }).click();
    await expect(card).toContainText("muộn 20 phút");
    await expectAccessible(page, "staff-attendance-day");
    await page.getByRole("button", { name: "Cả tháng" }).click();
    await expect(page.getByRole("table", { name: /Bảng chấm công/ })).toBeVisible();
    await expect(page.getByRole("button", { name: new RegExp(`^${name}, .*muộn 20 phút`) })).toBeVisible();

    // Bonus.
    await page.getByRole("tab", { name: "Thưởng, phạt" }).click();
    await page.getByRole("button", { name: "+ Ghi thưởng / phạt" }).click();
    const adj = page.getByRole("dialog", { name: "Ghi thưởng / phạt" });
    await adj.getByLabel("Nhân viên").selectOption({ label: name });
    await adj.getByLabel("Số tiền (đ)").fill("100000");
    await adj.getByLabel("Lý do").fill("Khách khen đích danh");
    await adj.getByRole("button", { name: "Lưu" }).click();
    await expect(adj).toBeHidden();
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

  test("phones get the day list, never the wide grid, and no sideways scroll", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/nhan-vien");
    await expect(page.getByRole("tab", { name: "Chấm công" })).toBeVisible();
    await expect(page.getByRole("list", { name: /^Chấm công / })).toBeVisible();
    await expect(page.getByRole("button", { name: "Cả tháng" })).toHaveCount(0);
    expect(await sidewaysOverflow(page)).toBe(0);
    await page.getByRole("tab", { name: "Bảng lương" }).click();
    await expect(page.getByText("Tạm tính")).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "staff-phone-payroll");
  });
});
