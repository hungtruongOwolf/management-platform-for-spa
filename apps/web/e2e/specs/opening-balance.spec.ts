import { expect, test } from "@playwright/test";
import { ownerState } from "../stack";
import { expectAccessible, uniquePhone } from "../support/run";

test.use({ storageState: ownerState });

test.describe("nhập liệu trình từ sổ cũ (US8)", () => {
  test("new customer + free-form package 4 left of 10 → a plain package on the profile, no revenue", async ({
    page,
  }) => {
    // EM 2026-11-05: no navigation entry any more; the owner opens the page by URL until the import
    // moves to an operator CLI.
    await page.goto("/");
    await expect(page.getByRole("link", { name: /sổ cũ|sổ giấy/i })).toHaveCount(0);
    await page.goto("/nhap-so-cu");
    await expect(
      page.getByText("Không tính vào doanh thu hôm nay."),
    ).toBeVisible();
    const search = page.getByRole("combobox", {
      name: "Tìm khách theo SĐT hoặc tên",
    });
    await expect(search).toBeFocused();
    // EM decision 2026-11-05: no "open import today" switch; the import is always today's.
    await expect(page.getByRole("switch")).toHaveCount(0);

    const phone = uniquePhone();
    await search.fill(phone);
    await page.getByRole("button", { name: "+ Khách mới" }).click();
    const form = page.getByRole("form", { name: "Nhập liệu trình từ sổ cũ" });
    await expect(form.getByLabel("Số điện thoại")).toHaveValue(phone);
    await form.getByLabel("Họ tên").fill("Đỗ Thị Sổ Cũ");
    await form.getByLabel("Tên gói").fill("Gội đầu 10 buổi (sổ cũ)");
    const service = form.getByLabel("Dịch vụ");
    await expect(service.locator("option")).not.toHaveCount(1);
    await service.selectOption({ index: 1 });
    await form.getByLabel("Buổi mua").fill("10");
    await form.getByLabel("Còn lại").fill("4");
    await form.getByLabel("Giá đã bán (đ)").fill("1500000");
    await form.getByLabel("Ngày đăng ký gốc").fill("2026-06-01");
    await expectAccessible(page, "opening-balance");

    // "No sale, no payment" (AC8.1/8.3) is asserted against the database in
    // apps/api/crates/app/tests/opening_balances.rs; here we check the user-visible result.
    await form.getByRole("button", { name: "Lưu và nhập tiếp" }).click();
    await expect(page.getByText(/còn 4\/10 buổi/)).toBeVisible();
    await expect(page.getByRole("status")).toContainText(
      "Đã nhập 1 liệu trình",
    );
    await expect(search).toBeFocused();

    await page.getByRole("link", { name: "Xem hồ sơ" }).click();
    await expect(page).toHaveURL(/\/khach-hang\/[0-9a-f-]{36}$/);
    const list = page.getByRole("list", { name: "Liệu trình của khách" });
    await expect(list).toContainText("Gội đầu 10 buổi (sổ cũ)");
    // Migrated packages look like any other package (EM 2026-11-05).
    await expect(list).not.toContainText("Nhập từ sổ cũ");
    await expect(list).toContainText("còn 4/10 buổi");
  });

  test("more left than total is stopped before saving", async ({ page }) => {
    await page.goto("/nhap-so-cu");
    await page
      .getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" })
      .fill(uniquePhone());
    await page.getByRole("button", { name: "+ Khách mới" }).click();
    const form = page.getByRole("form", { name: "Nhập liệu trình từ sổ cũ" });
    await form.getByLabel("Họ tên").fill("Khách Quá Buổi");
    await form.getByLabel("Tên gói").fill("Gói lỗi");
    await form.getByLabel("Dịch vụ").selectOption({ index: 1 });
    await form.getByLabel("Buổi mua").fill("10");
    await form.getByLabel("Còn lại").fill("11");
    await form.getByLabel("Giá đã bán (đ)").fill("0");
    await form.getByLabel("Ngày đăng ký gốc").fill("2026-06-01");
    await form.getByRole("button", { name: "Lưu và nhập tiếp" }).click();
    await expect(
      form.getByText("Còn lại không được lớn hơn tổng 10 buổi."),
    ).toBeVisible();
  });
});
