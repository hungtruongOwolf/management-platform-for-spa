import { expect, test } from "@playwright/test";
import { ownerState, stack } from "../stack";
import { sidewaysOverflow } from "../support/responsive";
import { expectAccessible, uniquePhone } from "../support/run";

test.use({ storageState: ownerState });

const write = { Origin: stack.webOrigin };

/** "Nhật ký ghi chú" (spec management-views §10) on the real API: notes written elsewhere read back here. */
test.describe("nhật ký ghi chú", () => {
  test("a profile note and a health note read back, filtered, with a link to the source", async ({ page }) => {
    const stamp = String(Date.now()).slice(-6);
    const name = `Khách ghi chú ${stamp}`;
    const created = await page.request.post("/api/v1/customers", {
      headers: write,
      data: { full_name: name, phone: uniquePhone(), gender: "female", note: `Thích trà gừng ${stamp}`, health_note: `Dị ứng tinh dầu ${stamp}` },
    });
    expect(created.status()).toBe(201);
    const customer = ((await created.json()) as { id: string }).id;

    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto("/nhat-ky-ghi-chu");
    await expect(page.getByRole("heading", { level: 1, name: "Nhật ký ghi chú" })).toBeVisible();
    await expect(page.getByText(`Thích trà gừng ${stamp}`)).toBeVisible();
    // The owner holds customer.health_notes.read: the encrypted note is decrypted for them.
    await expect(page.getByText(`Dị ứng tinh dầu ${stamp}`)).toBeVisible();
    await expectAccessible(page, "notes-1280");
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "notes-1280-dark");
    await page.emulateMedia({ colorScheme: "light" });

    // Search reaches the encrypted note too; the filter is in the URL.
    await page.getByRole("searchbox", { name: "Tìm trong ghi chú" }).fill(`tinh dau ${stamp}`);
    await expect(page).toHaveURL(/q=tinh/);
    await expect(page.getByText(`Dị ứng tinh dầu ${stamp}`)).toBeVisible();
    await expect(page.getByText(`Thích trà gừng ${stamp}`)).toBeHidden();
    await page.getByRole("searchbox", { name: "Tìm trong ghi chú" }).fill("");
    await expect(page).not.toHaveURL(/q=/);

    // One customer only, then back to the source.
    const row = page.getByRole("listitem").filter({ hasText: `Thích trà gừng ${stamp}` });
    await row.getByRole("button", { name: "Chỉ xem khách này" }).click();
    await expect(page).toHaveURL(new RegExp(`customer=${customer}`));
    await expect(page.getByRole("button", { name: `Bỏ lọc theo khách ${name}` })).toBeVisible();
    await page.getByRole("listitem").filter({ hasText: `Thích trà gừng ${stamp}` }).getByRole("link", { name: "Mở hồ sơ khách" }).click();
    await expect(page).toHaveURL(new RegExp(`/khach-hang/${customer}`));
  });

  test("phones: the list fits 390 px, light and dark", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/nhat-ky-ghi-chu");
    await expect(page.getByRole("heading", { level: 1, name: "Nhật ký ghi chú" })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Bộ lọc/ })).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "notes-390");
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "notes-390-dark");
  });
});
