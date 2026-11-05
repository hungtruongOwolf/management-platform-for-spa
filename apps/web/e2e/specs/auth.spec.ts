import { expect, test } from "@playwright/test";
import {
  credentials,
  expectAccessible,
  signIn,
  submitLogin,
} from "../support/run";

test.describe("đăng nhập", () => {
  test("login form is accessible", async ({ page }) => {
    await page.goto("/dang-nhap");
    await expect(
      page.getByRole("heading", { name: "Viên Phúc Y Đường" }),
    ).toBeVisible();
    await expectAccessible(page, "login");
  });

  test("owner signs in and reads the seeded price list", async ({ page }) => {
    await signIn(page);
    await expect(
      page.getByRole("heading", { name: /Xin chào, Chủ Spa E2E/ }),
    ).toBeVisible();
    await page
      .getByRole("navigation", { name: "Chính" })
      .getByRole("link", { name: "Dịch vụ" })
      .click();
    await expect(page).toHaveURL(/\/bang-gia$/);
    await expect(
      page.getByRole("heading", { name: "Bảng giá dịch vụ" }),
    ).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Gội đầu dưỡng sinh" }),
    ).toBeVisible();
    await expect(page.getByText("Chăm sóc cổ vai gáy")).toBeVisible();
    await expectAccessible(page, "price-list");
  });

  test("a deep link without a session goes through login and comes back", async ({
    page,
  }) => {
    await page.goto("/khach-hang");
    await expect(page).toHaveURL(/\/dang-nhap\?next=%2Fkhach-hang$/);
    // Owner: password, then the TOTP step, then back to the deep link.
    await submitLogin(page);
    await expect(page).toHaveURL(/\/khach-hang$/);
    await expect(
      page.getByRole("heading", { name: "Khách hàng", level: 1 }),
    ).toBeVisible();
  });

  test("an off-site next parameter is ignored", async ({ page }) => {
    await signIn(page, "/dang-nhap?next=%2F%2Fevil.example%2Fphish");
    await expect(page).toHaveURL(/localhost:\d+\/$/);
  });

  test("wrong password shows an error and no session", async ({ page }) => {
    const { phone } = credentials();
    await page.goto("/dang-nhap");
    await page.getByLabel("Số điện thoại").fill(phone);
    await page.getByLabel("Mật khẩu").fill("sai-mat-khau");
    await page.getByRole("button", { name: "Đăng nhập" }).click();
    await expect(page.getByRole("alert")).toBeVisible();
    await expect(page).toHaveURL(/\/dang-nhap/);
  });
});
