import { expect, test, type Locator, type Page } from "@playwright/test";
import { ownerState } from "../stack";
import { WRITE, createPlan, flatService } from "../support/fixtures";
import { expectAccessible, uniquePhone } from "../support/run";

// Round 6 at /quay (spec R1-1 §10, plan r1-6-upsell B6): sell a package in the cart and use
// today's session from it ("Làm 1 buổi hôm nay"), the unsaved cart after a reload, converting and
// adding to a saved visit ("Thêm dịch vụ / bán gói"), and the visit details sub-row.
test.use({ storageState: ownerState });

interface Fixture {
  name: string;
  phone: string;
  customerId: string;
  serviceId: string;
  servicePrice: number;
  planId: string;
  planName: string;
  staffId: string;
}

/** A new customer, a flat-priced service and a "10 + 2" plan of it (through the API). */
async function fixture(page: Page, tag: string): Promise<Fixture> {
  const service = await flatService(page.request);
  const plan = await createPlan(page.request, {
    name: `Gói upsell ${tag} ${String(Date.now())}`,
    price: 1_200_000,
    serviceId: service.id,
  });
  const phone = uniquePhone();
  const name = `Khách ${tag} ${phone.slice(-4)}`;
  const customer = await page.request.post("/api/v1/customers", {
    headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
    data: { full_name: name, phone, gender: "female" },
  });
  expect(customer.status()).toBe(201);
  const staff = (await (await page.request.get("/api/v1/staff")).json()) as {
    id: string;
    full_name: string;
  }[];
  const ktv = staff.find((s) => s.full_name === "KTV E2E") ?? staff[0];
  return {
    name,
    phone,
    customerId: ((await customer.json()) as { id: string }).id,
    serviceId: service.id,
    servicePrice: service.prices.all ?? 0,
    planId: plan.id,
    planName: plan.name,
    staffId: ktv?.id ?? "",
  };
}

async function pick(page: Page, f: Fixture): Promise<void> {
  const search = page.getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" });
  await search.pressSequentially(f.phone);
  await expect(page.getByRole("option", { name: new RegExp(f.name) })).toBeVisible();
  await search.press("Enter");
  await expect(page.getByRole("heading", { name: new RegExp(f.name) })).toBeVisible();
}

async function addOneOff(page: Page, f: Fixture): Promise<void> {
  const picker = page.getByLabel("Thêm dịch vụ lẻ");
  await expect(picker.locator(`option[value="${f.serviceId}"]`)).toHaveCount(1);
  await picker.selectOption(f.serviceId);
  await page.getByRole("combobox", { name: "KTV" }).last().selectOption(f.staffId);
}

const toast = (page: Page) => page.locator(".toast");

/**
 * Opens a row's ⋯ menu from the keyboard and runs one item. The menu closes on scroll (it is a
 * fixed popover), so the button is scrolled into view before it opens, and nothing scrolls after.
 */
async function rowAction(page: Page, button: Locator, item: string): Promise<void> {
  await button.scrollIntoViewIfNeeded();
  await button.focus();
  await page.keyboard.press("Enter");
  const entry = page.getByRole("menu").getByRole("menuitem", { name: item });
  await expect(entry).toBeVisible();
  await entry.dispatchEvent("click");
}

test.describe("vòng 6: bán gói trong giỏ, giỏ nháp, chuyển sang gói", () => {
  test("sells a package in the cart and uses today's session from it", async ({ page }) => {
    const f = await fixture(page, "giỏ");
    await page.goto("/quay");
    await pick(page, f);
    await addOneOff(page, f);
    await page.getByRole("button", { name: "Bán gói liệu trình", exact: true }).click();
    await page.getByRole("combobox", { name: "Mẫu gói" }).selectOption(f.planId);
    await expect(
      page.getByRole("checkbox", { name: /Dùng gói này cho buổi hôm nay/ }),
    ).toBeChecked();
    await expect(page.getByText(/Trừ gói mua hôm nay/)).toBeVisible();
    const checkout = page.getByRole("group", { name: "Lưu lượt" });
    await expect(checkout).toContainText("1.200.000đ");
    await expectAccessible(page, "quay-sell-package");
    await checkout.getByRole("button", { name: "Lưu lượt khách" }).click();
    await expect(toast(page)).toContainText(`Đã lưu lượt của ${f.name}`);
    await expect(toast(page)).toContainText("còn 11/12");
    await expect(toast(page)).toContainText(`bán ${f.planName}`);

    // The visit lists the package it sold, its amount under "Số tiền".
    const today = page.getByRole("table", { name: "Lượt khách hôm nay" });
    const row = today.getByRole("row").filter({ hasText: f.name }).first();
    await expect(row).toContainText("1.200.000đ");
    await row.getByRole("button", { name: new RegExp(`^${f.name}`) }).click();
    const details = today.getByTestId("visit-details");
    await expect(details).toContainText(f.planName);
    await expect(details).toContainText("Người chốt: KTV E2E");
    await expect(details).toContainText("Trừ buổi · còn 11/12");

    // EM 2026-11-06: the open row and its details must not look glued together.
    const rowBottom = await row
      .locator("td")
      .nth(2)
      .locator(":scope > span")
      .evaluate((el) => el.getBoundingClientRect().bottom);
    const detailsTop = await details.evaluate((el) => el.getBoundingClientRect().top);
    expect(detailsTop, "details start ≥ 8px below the row's content").toBeGreaterThanOrEqual(
      rowBottom + 8,
    );
    // Amounts line up with the "Số tiền" column.
    const header = await today
      .getByRole("columnheader", { name: "Số tiền" })
      .evaluate((el) => el.getBoundingClientRect().right);
    const amount = await details
      .locator(".vd-amount")
      .first()
      .evaluate((el) => el.getBoundingClientRect().right);
    expect(Math.abs(header - amount), "detail amounts sit in the Số tiền column").toBeLessThan(2);
    await expectAccessible(page, "quay-visit-details");
  });

  test("chị Lan: two packages, no service, one for her mother: one save, one payment (EM 2026-11-07 item 4)", async ({ page }) => {
    const f = await fixture(page, "hai gói");
    const motherPhone = uniquePhone();
    const motherName = `Mẹ của ${f.name}`;
    const mother = await page.request.post("/api/v1/customers", {
      headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
      data: { full_name: motherName, phone: motherPhone, gender: "female" },
    });
    expect(mother.status()).toBe(201);
    const motherId = ((await mother.json()) as { id: string }).id;

    await page.goto("/quay");
    await pick(page, f);
    await page.getByRole("button", { name: "Bán gói liệu trình", exact: true }).click();
    await page.getByRole("combobox", { name: "Mẫu gói" }).selectOption(f.planId);
    // A second package fits without a service; it is a gift for her mother.
    await page.getByRole("button", { name: "Bán gói liệu trình", exact: true }).click();
    await page.getByRole("combobox", { name: "Mẫu gói" }).selectOption(f.planId);
    await page.getByRole("checkbox", { name: /Mua cho người khác/ }).last().check();
    const who = page.getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" }).last();
    await who.pressSequentially(motherPhone);
    await page.getByRole("option", { name: new RegExp(motherName) }).click();
    const checkout = page.getByRole("group", { name: "Lưu lượt" });
    await expect(checkout).toContainText("2.400.000đ");
    await expectAccessible(page, "quay-two-packages");
    await checkout.getByRole("button", { name: "Lưu hóa đơn gói" }).click();
    await expect(toast(page)).toContainText(motherName);

    // One package each, both paid by the payer in the one step.
    for (const id of [f.customerId, motherId]) {
      const packages = (await (await page.request.get(`/api/v1/customers/${id}/packages`)).json()) as unknown[];
      expect(packages, id).toHaveLength(1);
    }
  });

  test("keeps the unsaved cart across a reload; “Bỏ giỏ” drops it", async ({ page }) => {
    const f = await fixture(page, "nháp");
    await page.goto("/quay");
    await pick(page, f);
    await addOneOff(page, f);
    await page.reload();
    await expect(page.getByText(`Đã khôi phục giỏ chưa lưu của ${f.name}.`)).toBeVisible();
    await expect(page.getByRole("list", { name: "Dịch vụ trong lượt" })).toBeVisible();
    await expectAccessible(page, "quay-restored-draft");
    await page.getByRole("button", { name: "Bỏ giỏ" }).click();
    await expect(page.getByRole("list", { name: "Dịch vụ trong lượt" })).toHaveCount(0);
    await page.reload();
    await expect(page.getByText(/Đã khôi phục giỏ chưa lưu/)).toHaveCount(0);
  });

  test("converts a paid one-off visit to a package, then adds a service to it", async ({ page }) => {
    const f = await fixture(page, "chuyển");
    // Today's one-off visit, paid in cash.
    const visit = await page.request.post("/api/v1/visits", {
      headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: f.customerId,
        lines: [{ service_id: f.serviceId, source: "one_off", staff_id: f.staffId }],
        payments: [{ method: "cash", amount: f.servicePrice }],
      },
    });
    expect(visit.status()).toBe(201);
    await page.goto("/quay");
    const today = page.getByRole("table", { name: "Lượt khách hôm nay" });
    await rowAction(
      page,
      today.getByRole("button", { name: new RegExp(`^Thao tác lượt .* của ${f.name}$`) }),
      "Thêm dịch vụ / bán gói",
    );
    // EM 2026-11-07: one ⋯ entry; converting is the "Tính buổi hôm nay vào gói" choice inside.
    await page.getByRole("button", { name: "Bán gói liệu trình", exact: true }).click();
    await page.getByRole("combobox", { name: "Mẫu gói" }).selectOption(f.planId);
    const count = page.getByRole("checkbox", { name: /Tính buổi hôm nay vào gói/ });
    await expect(count).not.toBeChecked();
    await count.check();
    await expect(page.getByText("Khách đưa thêm")).toBeVisible();
    await expectAccessible(page, "quay-convert");
    await page.getByRole("button", { name: "Chuyển sang gói" }).click();
    await expect(page.getByText(`Đã chuyển lượt của ${f.name}`)).toBeVisible();

    // The corrected visit is the current one: add a service to it (its own sale, no reason).
    // Wait for the refetched day: the old visit now reads "Đã sửa".
    await expect(
      today.getByRole("row").filter({ hasText: f.name }).filter({ hasText: "Đã sửa" }),
    ).toHaveCount(1);
    const current = today
      .getByRole("row")
      .filter({ hasText: f.name })
      .filter({ hasNotText: "Đã sửa" })
      .first();
    await page.waitForLoadState("networkidle");
    await rowAction(
      page,
      current.getByRole("button", { name: /^Thao tác lượt/ }),
      "Thêm dịch vụ / bán gói",
    );
    const extra = page.getByLabel("Thêm dịch vụ vào lượt");
    await expect(extra.locator(`option[value="${f.serviceId}"]`)).toHaveCount(1);
    await extra.selectOption(f.serviceId);
    await page.getByRole("button", { name: "Lưu phần thêm" }).click();
    await expect(page.getByText(`Đã thêm vào lượt của ${f.name}`)).toBeVisible();
  });
});

test.describe("EM 2026-11-07: Quầy flow review", () => {
  test("buy a package and do today's session from it in one save; lines fold", async ({ page }) => {
    const f = await fixture(page, "buổi");
    await page.goto("/quay");
    await pick(page, f);
    // No one-off first: the package line itself offers today's session.
    await page.getByRole("button", { name: "Bán gói liệu trình", exact: true }).click();
    await page.getByRole("combobox", { name: "Mẫu gói" }).selectOption(f.planId);
    await expect(page.getByText(/Bán thêm gói/)).toHaveCount(0);
    await page.getByRole("button", { name: "Làm 1 buổi hôm nay" }).click();
    const ktv = page.getByRole("combobox", { name: "KTV" });
    await expect(ktv).toBeFocused();
    await ktv.selectOption(f.staffId);
    await expect(page.getByRole("button", { name: `Sửa gói ${f.planName}` })).toBeVisible();
    await page.getByLabel("Ghi chú buổi (không bắt buộc)").fill("Mua gói, làm buổi 1");
    await expectAccessible(page, "quay-package-today-session");
    const checkout = page.getByRole("group", { name: "Lưu lượt" });
    await expect(checkout).toContainText("1.200.000đ");
    await checkout.getByRole("button", { name: "Lưu lượt khách" }).click();
    await expect(toast(page)).toContainText(`Đã lưu lượt của ${f.name}`);
    await expect(toast(page)).toContainText("còn 11/12");
    await expect(toast(page)).toContainText(`bán ${f.planName}`);
    // The note is on the day list and in the details.
    const today = page.getByRole("table", { name: "Lượt khách hôm nay" });
    const row = today.getByRole("row").filter({ hasText: f.name }).first();
    await expect(row).toContainText("Ghi chú: Mua gói, làm buổi 1");
  });

  test("a walk-in gets a note and their details later, never a raw null", async ({ page }) => {
    const services = (await (await page.request.get("/api/v1/services")).json()) as {
      id: string;
      active: boolean;
      prices: { all?: number | null };
    }[];
    const service = services.find((s) => s.active && s.prices.all != null);
    await page.goto("/quay");
    await page.getByRole("button", { name: "Khách vãng lai" }).click();
    await expect(page.getByRole("heading", { name: /Khách vãng lai/ })).toBeVisible();
    await page.getByLabel("Thêm dịch vụ lẻ").selectOption(service?.id ?? "");
    const staff = page.getByRole("combobox", { name: "KTV" });
    await staff.selectOption({ index: 1 });
    await page.getByLabel("Ghi chú buổi (không bắt buộc)").fill("Khách vãng lai, da nhạy cảm");
    await page.getByRole("button", { name: "Thêm thông tin khách" }).click();
    const form = page.getByRole("form", { name: "Sửa hồ sơ khách" });
    const phone = uniquePhone();
    await form.getByLabel("Họ tên").fill(`Chị Vãng ${phone.slice(-4)}`);
    await form.getByLabel("Số điện thoại").fill(phone);
    await expectAccessible(page, "quay-walk-in-info");
    await form.getByRole("button", { name: "Lưu thay đổi" }).click();
    await expect(
      page.getByRole("heading", { name: new RegExp(`Chị Vãng ${phone.slice(-4)}`) }),
    ).toBeVisible();
    // Same cart, same profile.
    await expect(page.getByRole("list", { name: "Dịch vụ trong lượt" })).toBeVisible();
    await page.getByRole("group", { name: "Lưu lượt" }).getByRole("button", { name: "Lưu lượt khách" }).click();
    await expect(toast(page)).toContainText(`Chị Vãng ${phone.slice(-4)}`);
    const today = page.getByRole("table", { name: "Lượt khách hôm nay" });
    const row = today.getByRole("row").filter({ hasText: `Chị Vãng ${phone.slice(-4)}` }).first();
    await expect(row).toContainText("Ghi chú: Khách vãng lai, da nhạy cảm");
    await row.getByRole("button", { name: /^Chị Vãng/ }).click();
    const details = today.getByTestId("visit-details");
    await expect(details).toContainText("Ghi chú buổi");
    await expect(today).not.toContainText("null");
  });
});
