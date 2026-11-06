/**
 * S10: axe (WCAG 2.2 AA) on the screens no journey spec covers — home, the package-plan editor and
 * the service form. Login, MFA, price list, customers, profile, front desk, sell, voids and the
 * paper-book import are audited inside their own journey specs.
 */
import { expect, test } from "@playwright/test";
import { ownerState } from "../stack";
import { expectAccessible } from "../support/run";

test.use({ storageState: ownerState });

test("home", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toContainText(
    "Xin chào",
  );
  await expectAccessible(page, "home");
});

test("package plans and the plan form", async ({ page }) => {
  await page.goto("/bang-gia/goi");
  await expect(
    page.getByRole("heading", { name: "Mẫu gói liệu trình" }),
  ).toBeVisible();
  await expectAccessible(page, "plans");
  await page.getByRole("button", { name: "Tạo mẫu gói" }).click();
  await expect(
    page.getByRole("form", { name: "Tạo mẫu gói liệu trình" }),
  ).toBeVisible();
  await expectAccessible(page, "plan-form");
});

test("service form on the price list", async ({ page }) => {
  await page.goto("/bang-gia");
  await page.getByRole("button", { name: "Thêm dịch vụ", exact: true }).first().click();
  await expect(page.getByRole("form", { name: "Thêm dịch vụ" })).toBeVisible();
  await expectAccessible(page, "service-form");
});
