import { expect, test } from "@playwright/test";
import { ownerState } from "../stack";
import { switchFeatureOn } from "../support/db";
import { WRITE } from "../support/fixtures";
import { sidewaysOverflow } from "../support/responsive";
import { credentials, expectAccessible, uniquePhone } from "../support/run";

test.use({ storageState: ownerState });

// The screen is off by default (ADR-0022 D5): the operator switched it on for this spa.
test.beforeAll(async () => {
  await switchFeatureOn(credentials().slug, "activity_log");
});

/** "Nhật ký thao tác" (ADR-0021, spec management-views §11) on the real API: what the owner did reads back as a sentence. */
test.describe("nhật ký thao tác", () => {
  test("a renamed customer reads back with the old and new name and phone, filtered by group", async ({ page }) => {
    const stamp = String(Date.now()).slice(-6);
    const first = `Khách nhật ký ${stamp}`;
    const renamed = `${first} B`;
    const phone = uniquePhone();
    const created = await page.request.post("/api/v1/customers", {
      headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
      data: { full_name: first, phone, gender: "female" },
    });
    expect(created.status()).toBe(201);
    const customer = (await created.json()) as { id: string; version: number };
    const newPhone = uniquePhone();
    const updated = await page.request.put(`/api/v1/customers/${customer.id}`, {
      headers: { ...WRITE, "If-Match": `"${String(customer.version)}"` },
      data: { full_name: renamed, phone: newPhone, gender: "female" },
    });
    expect(updated.status()).toBe(200);

    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto("/nhat-ky-thao-tac");
    await expect(page.getByRole("heading", { level: 1, name: "Nhật ký thao tác" })).toBeVisible();
    await page.getByRole("combobox", { name: "Loại việc" }).selectOption("customers");
    await expect(page).toHaveURL(/kind=customers/);
    const row = page.getByRole("listitem").filter({ hasText: `sửa hồ sơ của chị ${renamed}` });
    await expect(row).toBeVisible();
    await expect(row.getByText(`Tên: ${first} → ${renamed}`)).toBeVisible();
    await expect(row.getByText(/^SĐT: 0\d{3} \d{3} \d{3} → 0\d{3} \d{3} \d{3}$/)).toBeVisible();
    await expectAccessible(page, "activity-1280");
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "activity-1280-dark");
    await page.emulateMedia({ colorScheme: "light" });

    await row.getByRole("link", { name: `chị ${renamed}` }).click();
    await expect(page).toHaveURL(new RegExp(`/khach-hang/${customer.id}`));
  });

  test("phones: the list fits 390 px, light and dark", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/nhat-ky-thao-tac");
    await expect(page.getByRole("heading", { level: 1, name: "Nhật ký thao tác" })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Bộ lọc/ })).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "activity-390");
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "activity-390-dark");
  });
});
