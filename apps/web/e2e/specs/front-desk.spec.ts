import { expect, test, type Page } from "@playwright/test";
import { ownerState } from "../stack";
import { WRITE, createPlan } from "../support/fixtures";
import { expectAccessible, uniquePhone } from "../support/run";

test.use({ storageState: ownerState });

/** A customer who already owns a fresh "10 + 2" package (set up through the API, like S6 did). */
async function regularWithPackage(page: Page, phone: string): Promise<void> {
  const plan = await createPlan(page.request, { name: `Gói quầy ${String(Date.now())}`, price: 1_500_000 });
  const customer = await page.request.post("/api/v1/customers", {
    headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
    data: { full_name: "Lê Thị Quầy", phone, gender: "female" },
  });
  expect(customer.status()).toBe(201);
  const staff = (await (await page.request.get("/api/v1/staff")).json()) as {
    id: string;
  }[];
  const sale = await page.request.post("/api/v1/sales", {
    headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
    data: {
      customer_id: ((await customer.json()) as { id: string }).id,
      closer_staff_id: staff[0]?.id,
      lines: [{ package_plan_id: plan.id }],
      payments: [{ method: "cash", amount: 1_500_000 }],
    },
  });
  expect(sale.status()).toBe(201);
}

test.describe("màn hình quầy (US4)", () => {
  test("fast path: a regular's session is recorded in 4 actions", async ({
    page,
  }) => {
    const phone = uniquePhone();
    await regularWithPackage(page, phone);
    await page.goto("/quay");
    const search = page.getByRole("combobox", {
      name: "Tìm khách theo SĐT hoặc tên",
    });
    await expect(search).toBeFocused();
    await expectAccessible(page, "quay-empty");

    // 1. Type the phone, Enter picks the first hit; focus jumps to "Trừ 1 buổi".
    await search.pressSequentially(phone);
    await expect(
      page.getByRole("option", { name: /Lê Thị Quầy/ }),
    ).toBeVisible();
    await search.press("Enter");
    const deduct = page.getByRole("button", { name: /^Trừ 1 buổi Gói quầy/ });
    await expect(deduct).toBeFocused();
    await expect(page.getByText("12/12")).toBeVisible();

    // 2. Enter on the focused button adds the line; focus jumps to its technician picker.
    await page.keyboard.press("Enter");
    const tech = page.getByRole("combobox", { name: "KTV" });
    await expect(tech).toBeFocused();
    await expect(page.getByText("11/12")).toBeVisible();
    // The one-off service picker once widened the cart column past the viewport (long options).
    expect(
      await page.evaluate(
        () =>
          document.documentElement.scrollWidth <=
          document.documentElement.clientWidth,
      ),
      "no horizontal scroll with a line in the cart",
    ).toBe(true);
    // EM 2026-11-06: "Lưu lượt khách" was clipped against its right edge. Full width, never clipped.
    const saveButton = page.getByRole("button", { name: "Lưu lượt khách" });
    expect(
      await saveButton.evaluate((b) => b.scrollWidth <= b.clientWidth),
      "the save label fits its button",
    ).toBe(true);
    await expectAccessible(page, "quay-cart");

    // 3. Pick the technician.
    await tech.selectOption({ label: "KTV E2E" });

    // 4. Ctrl+Enter saves.
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("status")).toContainText(
      "Đã lưu lượt của Lê Thị Quầy",
    );
    await expect(page.getByRole("status")).toContainText("còn 11/12");
    const today = page.getByRole("table", { name: "Lượt khách hôm nay" });
    await expect(today).toContainText("Lê Thị Quầy");
    await expect(today).toContainText("KTV KTV E2E");
    await expect(search).toBeFocused();
    await expectAccessible(page, "quay-saved");
  });
});
