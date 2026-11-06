import { expect, test, type Page } from "@playwright/test";
import { ownerState, stack } from "../stack";
import { expectAccessible, uniquePhone } from "../support/run";

test.use({ storageState: ownerState });

interface Service {
  id: string;
  active: boolean;
  prices: { all?: number };
}

/** A fresh "10 + 2 bonus at 1.500.000đ" plan through the owner's API (catalog UI is covered elsewhere). */
async function createPlan(page: Page): Promise<string> {
  const services = (await (
    await page.request.get("/api/v1/services")
  ).json()) as Service[];
  const service = services.find((s) => s.active && s.prices.all != null);
  expect(service, "seeded menu has a flat-priced service").toBeDefined();
  const reply = await page.request.post("/api/v1/package-plans", {
    headers: { Origin: stack.webOrigin },
    data: {
      name: `Gói E2E ${String(Date.now())}`,
      price: 1_500_000,
      validity_months: 6,
      gift_note: "Tặng dầu gội",
      active: true,
      items: [{ service_id: service?.id, sessions: 10, bonus_sessions: 2 }],
    },
  });
  expect(reply.status()).toBe(201);
  return ((await reply.json()) as { id: string }).id;
}

test.describe("bán liệu trình (US3)", () => {
  test("sell a 10 + 2 package, paid half cash half transfer → 12/12 on the profile", async ({
    page,
  }) => {
    const planId = await createPlan(page);
    await page.goto("/khach-hang");
    await page.getByRole("button", { name: "Khách mới", exact: true }).click();
    const form = page.getByRole("form", { name: "Khách mới" });
    await form.getByLabel("Họ tên").fill("Phạm Thị Mai");
    await form.getByLabel("Số điện thoại").fill(uniquePhone());
    await form.getByRole("button", { name: "Lưu khách" }).click();
    await expect(page).toHaveURL(/\/khach-hang\/[0-9a-f-]{36}$/);
    await expect(page.getByText("Chưa có gói liệu trình nào.")).toBeVisible();

    await page.getByRole("button", { name: "Bán gói liệu trình" }).click();
    const dialog = page.getByRole("dialog", { name: "Bán gói liệu trình" });
    await dialog.getByRole("combobox", { name: "Gói liệu trình", exact: true }).selectOption(planId);
    await expect(dialog.getByLabel("Quà tặng hiện vật")).toHaveValue(
      "Tặng dầu gội",
    );
    await dialog
      .getByLabel("Người tư vấn chốt")
      .selectOption({ label: "KTV E2E (Kỹ thuật viên)" });
    await dialog
      .getByRole("radio", { name: "Tiền mặt + chuyển khoản" })
      .check();
    await dialog.getByLabel(/^Tiền mặt \(đ\)/).fill("500000");
    await expect(dialog.getByText("Chuyển khoản: 1.000.000đ")).toBeVisible();
    await expect(dialog.getByText("Phải trả: 1.500.000đ")).toBeVisible();
    await expectAccessible(page, "sell-package");

    await dialog.getByRole("button", { name: "Lưu & thu tiền" }).dblclick();
    await expect(page.getByRole("status").filter({ hasText: "còn 12/12 buổi" })).toBeVisible();
    const list = page.getByRole("list", { name: "Gói liệu trình của khách" });
    await expect(list.getByRole("listitem")).toHaveCount(1);
    await expect(list).toContainText("Còn 12/12 buổi");
    await expect(list).toContainText("Đang dùng");
  });
});
