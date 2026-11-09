import { expect, test, type Page } from "@playwright/test";
import { ownerState } from "../stack";
import { ageRecentAuth } from "../support/db";
import { WRITE, createPlan, flatService, type Service } from "../support/fixtures";
import { credentials, expectAccessible, uniquePhone } from "../support/run";

test.use({ storageState: ownerState });

interface Pkg {
  id: string;
  sessions_remaining: number;
  items: { id: string }[];
}

/** A customer with a fresh "10 + 2" package and one recorded visit that took a session (11/12). */
async function customerWithVisit(
  page: Page,
  phone: string,
  fullName: string,
): Promise<string> {
  const service = await flatService(page.request);
  const plan = await createPlan(page.request, {
    name: `Gói hủy ${String(Date.now())}`,
    price: 1_500_000,
    serviceId: service.id,
  });
  const customer = await page.request.post("/api/v1/customers", {
    headers: WRITE,
    data: { full_name: fullName, phone, gender: "female" },
  });
  expect(customer.status()).toBe(201);
  const customerId = ((await customer.json()) as { id: string }).id;
  const staff = (await (await page.request.get("/api/v1/staff")).json()) as {
    id: string;
  }[];
  const sale = await page.request.post("/api/v1/sales", {
    headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
    data: {
      customer_id: customerId,
      closer_staff_id: staff[0]?.id,
      lines: [{ package_plan_id: plan.id }],
      payments: [{ method: "cash", amount: 1_500_000 }],
    },
  });
  expect(sale.status()).toBe(201);
  const pkg = ((await sale.json()) as { customer_packages: Pkg[] })
    .customer_packages[0];
  const visit = await page.request.post("/api/v1/visits", {
    headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
    data: {
      customer_id: customerId,
      lines: [
        {
          service_id: service.id,
          source: "package",
          customer_package_item_id: pkg?.items[0]?.id,
          staff_id: staff[0]?.id,
        },
      ],
    },
  });
  expect(visit.status()).toBe(201);
  return customerId;
}

async function remaining(page: Page, customerId: string): Promise<number> {
  const list = (await (
    await page.request.get(`/api/v1/customers/${customerId}/packages`)
  ).json()) as Pkg[];
  return list[0]?.sessions_remaining ?? -1;
}

test.describe("hủy / sửa sai & vòng đời liệu trình (US6, US7)", () => {
  test("Sửa lượt: a package session corrected into a one-off, money difference shown first (US6b)", async ({
    page,
  }) => {
    const customerId = await customerWithVisit(
      page,
      uniquePhone(),
      "Lý Thị Sửa",
    );
    expect(await remaining(page, customerId)).toBe(11);
    const services = (await (
      await page.request.get("/api/v1/services")
    ).json()) as Service[];
    const oneOff = services.find((s) => s.active && s.prices.all != null);

    await page.goto("/quay");
    const today = page.getByRole("table", { name: "Lượt khách hôm nay" });
    await today
      .getByRole("button", { name: /^Thao tác lượt .* của Lý Thị Sửa$/ })
      .click();
    await page.getByRole("menuitem", { name: "Sửa lượt" }).click();
    await expect(
      page.getByRole("heading", { name: "Sửa lượt khách" }),
    ).toBeVisible();
    await expect(page.getByText(/Đang sửa lượt .* của Lý Thị Sửa/)).toBeVisible();
    await expect(
      page.getByText("Chưa có gì thay đổi so với lượt đã ghi."),
    ).toBeVisible();

    // The session should have been a paid one-off.
    const cart = page.getByRole("list", { name: "Dịch vụ trong lượt" });
    await cart.getByRole("button", { name: /^Bỏ dòng / }).click();
    // Picking a service adds it (no separate "Thêm").
    await page.getByLabel("Thêm dịch vụ lẻ").selectOption(oneOff?.id ?? "");
    await cart.getByRole("combobox", { name: "KTV" }).selectOption({ index: 1 });
    await expect(page.getByTestId("correction-difference")).toContainText(
      "Thu thêm",
    );
    // The amount to collect is the difference; the new total is secondary (EM live test).
    await expect(page.getByText(/^Tổng lượt sau sửa .* · đã thu /)).toBeVisible();
    await page
      .getByRole("textbox", { name: /^Lý do sửa/ })
      .fill("Khách trả tiền lẻ, không trừ gói");
    // The correction cart is taller than the window: wherever it is scrolled, the sticky top bar or
    // the sticky checkout cuts some control, and axe target-size counts that cut. Audit the whole
    // screen laid out in a window as tall as the page, then go back.
    const size = page.viewportSize();
    const tall = await page.evaluate(() => document.documentElement.scrollHeight);
    if (size) await page.setViewportSize({ width: size.width, height: tall });
    await expectAccessible(page, "quay-correction");
    if (size) await page.setViewportSize(size);
    await page.getByRole("button", { name: "Lưu sửa lượt" }).click();

    await expect(
      page.getByRole("status").filter({ hasText: /Đã sửa lượt của Lý Thị Sửa · Thu thêm/ }),
    ).toBeVisible();
    const rows = today.getByRole("row").filter({ hasText: "Lý Thị Sửa" });
    await expect(rows).toHaveCount(2);
    await expect(rows.filter({ hasText: "Đã sửa" })).toHaveCount(1);
    expect(await remaining(page, customerId)).toBe(12);
    // The corrected visit is the one offered for further edits; the old one is not.
    await rows
      .filter({ hasText: "Đã sửa" })
      .getByRole("button", { name: /^Thao tác lượt/ })
      .click();
    await expect(page.getByRole("menuitem", { name: "Sửa lượt" })).toHaveCount(0);
    await page.keyboard.press("Escape");
    await expectAccessible(page, "quay-corrected");
  });

  test("void a visit from the day list: the session comes back, the visit stays as voided", async ({
    page,
  }) => {
    const customerId = await customerWithVisit(
      page,
      uniquePhone(),
      "Đặng Thị Hủy",
    );
    expect(await remaining(page, customerId)).toBe(11);

    await page.goto("/quay");
    const today = page.getByRole("table", { name: "Lượt khách hôm nay" });
    // EM round 5: "Hủy lượt" lives in the row's ⋯ menu.
    await today
      .getByRole("button", { name: /^Thao tác lượt .* của Đặng Thị Hủy$/ })
      .click();
    await expectAccessible(page, "quay-row-menu");
    await page.getByRole("menuitem", { name: "Hủy lượt" }).click();
    const dialog = page.getByRole("dialog", {
      name: /Hủy lượt .* Đặng Thị Hủy/,
    });
    await expect(dialog.getByLabel("Lý do hủy")).toBeFocused();
    await dialog.getByRole("button", { name: "Hủy lượt" }).click();
    await expect(dialog.getByText("Bắt buộc ghi lý do.")).toBeVisible();
    await expectAccessible(page, "quay-void-dialog");

    await dialog.getByLabel("Lý do hủy").fill("Ghi nhầm khách");
    await dialog.getByRole("button", { name: "Hủy lượt" }).click();
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "Đã hủy lượt của Đặng Thị Hủy." }),
    ).toBeVisible();
    const row = today.getByRole("row").filter({ hasText: "Đặng Thị Hủy" }).first();
    await expect(row).toContainText("Đã hủy");
    // The details stay open on the cancelled visit, with its reason.
    await expect(today).toContainText("Lý do hủy: Ghi nhầm khách");
    await row.getByRole("button", { name: /^Thao tác lượt/ }).click();
    await expect(page.getByRole("menuitem", { name: "Hủy lượt" })).toHaveCount(0);
    await page.keyboard.press("Escape");
    expect(await remaining(page, customerId)).toBe(12);
    await expectAccessible(page, "quay-voided");
  });

  test("owner extends a package, then cancels it with a refund (no password prompt)", async ({
    page,
  }) => {
    const customerId = await customerWithVisit(
      page,
      uniquePhone(),
      "Võ Thị Gia Hạn",
    );
    await page.goto(`/khach-hang/${customerId}`);
    const packages = page.getByRole("list", { name: "Gói liệu trình của khách" });

    // Two or more actions on a row sit in its "⋯" menu (design-system.md "Quy tắc" 8).
    const actions = packages.getByRole("button", { name: /^Thao tác gói/ }).first();
    await actions.click();
    await page.getByRole("menuitem", { name: "Gia hạn" }).click();
    const extend = page.getByRole("dialog", { name: /Gia hạn/ });
    await extend.getByRole("button", { name: "+2 tháng" }).click();
    await extend.getByLabel("Lý do gia hạn").fill("Khách đi công tác dài ngày");
    await expectAccessible(page, "profile-extend");
    await extend.getByRole("button", { name: "Gia hạn" }).click();
    // A review step (old → new expiry, reason) before anything is saved.
    const review = extend.getByRole("group", { name: "Xác nhận gia hạn" });
    await expect(review).toContainText("Khách đi công tác dài ngày");
    await expectAccessible(page, "profile-extend-review");
    await review.getByRole("button", { name: "Xác nhận gia hạn" }).click();
    await expect(
      page.getByRole("status").filter({ hasText: "Đã gia hạn" }),
    ).toBeVisible();

    await actions.click();
    await page.getByRole("menuitem", { name: "Hủy gói và hoàn tiền" }).click();
    const cancel = page.getByRole("dialog", { name: /Hủy gói/ });
    await cancel.getByLabel(/Số tiền hoàn/).fill("300.000");
    await cancel.getByLabel("Lý do hủy").fill("Khách chuyển nhà");
    await expectAccessible(page, "profile-cancel");
    // EM decision 2026-11-05: the owner is never asked for the password on a business action.
    // Age the last password confirmation past the old 5-minute window to prove it.
    await ageRecentAuth(credentials().phone);
    await cancel.getByRole("button", { name: /Hủy và hoàn 300\.000/ }).click();

    await expect(
      page
        .getByRole("status")
        .filter({ hasText: /Đã hủy .* và hoàn 300\.000/ }),
    ).toBeVisible();
    await expect(
      page.getByRole("dialog", { name: "Xác nhận lại mật khẩu" }),
    ).toHaveCount(0);
    // A cancelled package leaves the open list for the "Gói đã hết / đã hủy" disclosure.
    await page.getByText(/Gói đã hết \/ đã hủy \(\d+\)/).click();
    const row = page.getByRole("list", { name: "Gói đã hết hoặc đã hủy" }).getByRole("listitem").first();
    await expect(row).toContainText("Đã hủy");
    await expect(row).toContainText("Đã hủy: Khách chuyển nhà");
    await expect(row).toContainText("Còn 0/12 buổi");
    await expect(row.getByRole("button")).toHaveCount(0);
  });
});
