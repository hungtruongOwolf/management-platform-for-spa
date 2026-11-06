import { expect, test, type Page } from "@playwright/test";
import { ownerState } from "../stack";
import { backdateCustomer } from "../support/db";
import { expectAccessible, uniquePhone } from "../support/run";

test.use({ storageState: ownerState });

/** CustomerName reads "Tên, Nữ" (the comma is screen-reader-only text, so spacing varies). */
const nameWithGender = (name: string) => new RegExp(`^${name}\\s*,\\s*Nữ$`);

interface Service {
  id: string;
  name: string;
  prices: { all?: number | null; female?: number | null };
}

/**
 * A female customer with one paid one-off visit, moved 20 days back: she is then on the
 * "Lâu không quay lại" list. Calls go through the page's own fetch (same origin, session cookie).
 */
async function absentCustomer(page: Page, fullName: string, phone: string): Promise<string> {
  await page.goto("/khach-hang");
  const id = await page.evaluate(
    async ({ fullName, phone }) => {
      const json = async (path: string, init: { method?: string; body?: string } = {}) => {
        const r = await fetch(`/api/v1${path}`, {
          credentials: "same-origin",
          ...init,
          headers: { "content-type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        });
        if (!r.ok) throw new Error(`${path}: ${String(r.status)} ${await r.text()}`);
        return (await r.json()) as unknown;
      };
      const customer = (await json("/customers", {
        method: "POST",
        body: JSON.stringify({ full_name: fullName, phone, gender: "female" }),
      })) as { id: string };
      const services = (await json("/services")) as Service[];
      const service = services.find((s) => (s.prices.female ?? s.prices.all) != null);
      const staff = (await json("/staff")) as { id: string }[];
      if (!service || !staff[0]) throw new Error("catalog or staff missing");
      const price = service.prices.female ?? service.prices.all ?? 0;
      await json("/visits", {
        method: "POST",
        body: JSON.stringify({
          customer_id: customer.id,
          lines: [{ service_id: service.id, source: "one_off", staff_id: staff[0].id }],
          payments: [{ method: "cash", amount: price }],
        }),
      });
      return customer.id;
    },
    { fullName, phone },
  );
  await backdateCustomer(id, 20);
  return id;
}

test.describe("báo cáo (EM round 3)", () => {
  test("period picker in the URL, line chart with keyboard tooltip, ledger tab", async ({ page }) => {
    await page.goto("/bao-cao");
    await expect(page.getByRole("heading", { level: 1, name: "Báo cáo kinh doanh" })).toBeVisible();
    await page.getByRole("button", { name: "30 ngày" }).click();
    await expect(page).toHaveURL(/period=30d/);
    const chart = page.getByRole("img", { name: /^Thực thu theo ngày/ });
    await expect(chart).toBeVisible();
    await chart.focus();
    await page.keyboard.press("ArrowLeft");
    await expect(page.getByRole("tooltip")).toBeVisible();
    await expect(page.getByRole("table", { name: /Chi tiết theo ngày/ })).toBeVisible();
    await expectAccessible(page, "reports");

    await page.getByRole("button", { name: "Năm nay" }).click();
    await expect(page.getByRole("table", { name: /Chi tiết theo tháng/ })).toBeVisible();

    await page.getByRole("tab", { name: "Sổ doanh thu" }).click();
    await expect(page).toHaveURL(/tab=ledger/);
    await expect(page.getByText(/xem tối đa 3 tháng/)).toBeVisible();
    await page.getByRole("button", { name: "Tháng này" }).click();
    await expect(page.getByText("Tổng thực thu")).toBeVisible();
    await expectAccessible(page, "reports-ledger");

    // The URL is the state: a reload (or a shared link) reopens the same tab and period.
    await page.reload();
    await expect(page.getByRole("tab", { name: "Sổ doanh thu" })).toHaveAttribute("aria-selected", "true");
    await expect(page.getByRole("button", { name: "Tháng này" })).toHaveAttribute("aria-pressed", "true");
  });
});

test.describe("khách lâu không quay lại + chăm sóc lại (EM round 3)", () => {
  test("record a contact, see it scheduled, open the profile log, come back to the same filter", async ({ page }) => {
    const phone = uniquePhone();
    const name = `Khách Vắng ${phone.slice(-4)}`;
    await absentCustomer(page, name, phone);

    await page.goto(`/khach-lau-khong-quay-lai?q=${encodeURIComponent(phone)}`);
    const table = page.getByRole("table", { name: "Khách lâu chưa quay lại" });
    await expect(table.getByRole("link", { name: nameWithGender(name) })).toBeVisible();
    await expect(table.getByText("Khách lẻ")).toBeVisible();
    await expect(table.getByText("Chưa liên hệ").filter({ visible: true })).toBeVisible();
    await expectAccessible(page, "inactive");

    await table.getByRole("button", { name: `Đã liên hệ với ${name}` }).filter({ visible: true }).click();
    const form = page.getByRole("form", { name: `Ghi lần liên hệ với ${name}` });
    await expect(form.getByLabel("Ngày liên hệ", { exact: true })).toBeVisible();
    await form.getByLabel("Hẹn sẽ quay lại").check();
    await form.getByLabel(/Ghi chú/).fill("Hẹn cuối tuần");
    await expectAccessible(page, "follow-up-form");
    await form.getByRole("button", { name: "Lưu lần liên hệ" }).click();
    await expect(form).toBeHidden();
    // No longer due: gone from "Cần liên hệ", listed under "Đã hẹn" with the planned day.
    await expect(table.getByRole("link", { name: nameWithGender(name) })).toBeHidden();
    await page.getByRole("button", { name: "Đã hẹn" }).click();
    await expect(page).toHaveURL(/follow=scheduled/);
    await expect(table.getByRole("link", { name: nameWithGender(name) })).toBeVisible();
    await expect(table.getByText(/^Hẹn \d{2}\/\d{2}\/\d{4}$/).filter({ visible: true })).toBeVisible();

    await table.getByRole("link", { name: nameWithGender(name) }).click();
    const card = page.getByRole("region", { name: "Chăm sóc lại" });
    await expect(card.getByRole("list", { name: "Các lần liên hệ" })).toContainText("Hẹn sẽ quay lại");
    await expect(card).toContainText("Hẹn cuối tuần");
    await page.getByRole("link", { name: /^Quay lại/ }).click();
    await expect(page).toHaveURL(/follow=scheduled/);
    await expect(page.getByRole("button", { name: "Đã hẹn" })).toHaveAttribute("aria-pressed", "true");
  });

  test("the customer form warns about a taken phone while typing", async ({ page }) => {
    const phone = uniquePhone();
    const name = `Khách Trùng ${phone.slice(-4)}`;
    await absentCustomer(page, name, phone);
    await page.goto("/khach-hang");
    await page.getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" }).fill("Khách mới hoàn toàn");
    await page.getByRole("button", { name: "Khách mới", exact: true }).click();
    const form = page.getByRole("form", { name: "Khách mới" });
    await form.getByLabel("Số điện thoại").fill(phone);
    await expect(form.getByText(`Số này đã có hồ sơ: ${name}`)).toBeVisible();
    await form.getByRole("button", { name: "Mở hồ sơ" }).click();
    await expect(page.getByRole("heading", { level: 1, name: new RegExp(`^${name}`) })).toBeVisible();
  });

  test("the profile history is searchable and keeps its filters in the URL", async ({ page }) => {
    const phone = uniquePhone();
    const id = await absentCustomer(page, `Khách Lịch Sử ${phone.slice(-4)}`, phone);
    await page.goto(`/khach-hang/${id}`);
    const history = page.getByRole("region", { name: "Lịch sử sử dụng" });
    await expect(history.getByRole("list", { name: "Lịch sử sử dụng" })).toContainText("Lượt khách");
    await history.getByLabel("Loại").selectOption("package_cancelled");
    await expect(page).toHaveURL(/hkind=package_cancelled/);
    await expect(history.getByText("Không thấy mục nào")).toBeVisible();
    await history.getByRole("button", { name: "Xóa bộ lọc" }).click();
    await expect(history.getByRole("list", { name: "Lịch sử sử dụng" })).toContainText("Lượt khách");
    await expectAccessible(page, "customer-history");
  });
});
