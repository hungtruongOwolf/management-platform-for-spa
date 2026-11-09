import { expect, test } from "@playwright/test";
import { ownerState } from "../stack";
import { expectAccessible, uniquePhone } from "../support/run";

test.use({ storageState: ownerState });

test.describe("khách hàng (US2)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/khach-hang");
    await expect(
      page.getByRole("heading", { name: "Danh sách khách hàng", level: 1, exact: true }),
    ).toBeVisible();
  });

  test("create → find without accents → profile shows the health warning", async ({
    page,
  }) => {
    const phone = uniquePhone();
    await expectAccessible(page, "customers");

    await page.getByRole("button", { name: "Khách mới", exact: true }).click();
    const form = page.getByRole("form", { name: "Khách mới" });
    await form.getByLabel("Họ tên").fill("Nguyễn Thị Hoà");
    await form.getByLabel("Số điện thoại").fill(phone);
    await form.getByLabel("Nữ").check();
    await form
      .getByLabel(/Lưu ý sức khỏe/)
      .fill("Huyết áp cao — tránh đá nóng");
    await form.getByRole("button", { name: "Lưu khách" }).click();

    await expect(page).toHaveURL(/\/khach-hang\/[0-9a-f-]{36}$/);
    await expect(page.getByRole("note")).toContainText(
      "Huyết áp cao — tránh đá nóng",
    );
    await expectAccessible(page, "customer-profile");

    // Desktop: the breadcrumb is the way up (the back link to the same parent hides from 640px).
    await page.getByRole("navigation", { name: "Vị trí trang" }).getByRole("link", { name: "Khách hàng" }).click();
    const search = page.getByRole("combobox", {
      name: "Tìm khách theo SĐT hoặc tên",
    });
    await search.fill("nguyen thi hoa");
    const hit = page
      .getByRole("listbox", { name: "Kết quả tìm khách" })
      .getByRole("option", { name: /Nguyễn Thị Hoà/ });
    await expect(hit.first()).toBeVisible();

    // Keyboard path (spec §6: fully usable by keyboard): Enter opens the highlighted hit.
    await search.press("Enter");
    await expect(page).toHaveURL(/\/khach-hang\/[0-9a-f-]{36}$/);
    await expect(page.getByRole("note")).toContainText("Huyết áp cao");
  });

  test("search by the last digits of the phone", async ({ page }) => {
    const phone = uniquePhone();
    await page.getByRole("button", { name: "Khách mới", exact: true }).click();
    const form = page.getByRole("form", { name: "Khách mới" });
    await form.getByLabel("Họ tên").fill("Lê Văn Đức");
    await form.getByLabel("Số điện thoại").fill(phone);
    await form.getByRole("button", { name: "Lưu khách" }).click();
    await expect(page).toHaveURL(/\/khach-hang\/[0-9a-f-]{36}$/);
    await expect(page.getByRole("note")).toHaveCount(0);

    // Desktop: the breadcrumb is the way up (the back link to the same parent hides from 640px).
    await page.getByRole("navigation", { name: "Vị trí trang" }).getByRole("link", { name: "Khách hàng" }).click();
    await page
      .getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" })
      .fill(phone.slice(-6));
    await expect(
      page.getByRole("option", { name: /Lê Văn Đức/ }),
    ).toBeVisible();
  });
});
