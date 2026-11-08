import { expect, test, type Browser, type Page } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { ownerState, runFile, stack } from "../stack";
import { sidewaysOverflow } from "../support/responsive";
import { credentials, expectAccessible, uniquePhone } from "../support/run";

/**
 * A technician's own pages (EM 2026-11-07, spec staff-management §12): "Công và lương của tôi"
 * (summary, calendar, a day's detail) and "Khách của tôi" (own calendar, read-only), signed in as
 * a real technician whose profile the owner links here. Never a phone number on screen.
 */

const write = { Origin: stack.webOrigin };
const techState = join(dirname(runFile), "tech.json");
const CUSTOMER = "Phạm Thị Ngày Công";

function spaDay(offsetDays = 0): string {
  return new Date(Date.now() + 7 * 3_600_000 + offsetDays * 86_400_000).toISOString().slice(0, 10);
}

interface Service {
  id: string;
  name: string;
  active: boolean;
  prices: { all?: number | null };
}

let phone = "";
let serviceName = "";

test.describe.configure({ mode: "serial" });

/** The owner links the technician login to a profile with pay terms, then records their day. */
async function seedTechnicianDay(page: Page): Promise<void> {
  const accounts = (await (await page.request.get("/api/v1/staff/accounts")).json()) as {
    user_id: string;
    full_name: string;
    roles: string[];
    staff_id: string | null;
  }[];
  const login = accounts.find((a) => a.full_name === "KTV Đăng Nhập E2E");
  expect(login, "setup provisioned the technician login").toBeDefined();
  let staffId = login?.staff_id ?? null;
  if (!staffId) {
    const created = await page.request.post("/api/v1/staff", {
      headers: write,
      data: { full_name: "KTV Thảo E2E", position: "Kỹ thuật viên", user_id: login?.user_id },
    });
    expect(created.status(), await created.text()).toBe(201);
    staffId = ((await created.json()) as { id: string }).id;
  }
  const pay = await page.request.put(`/api/v1/staff/${staffId}/pay`, {
    headers: write,
    data: { base_salary: 6_000_000, commission_rate_bp: 500, loyalty_bonus_monthly: 0, meal_allowance: 0 },
  });
  expect(pay.status(), await pay.text()).toBe(200);
  const attendance = await page.request.put("/api/v1/attendance", {
    headers: write,
    data: { entries: [{ staff_id: staffId, date: spaDay(), status: "present", check_in: "09:14" }] },
  });
  expect(attendance.status(), await attendance.text()).toBe(200);

  phone = uniquePhone();
  const customer = await page.request.post("/api/v1/customers", {
    headers: write,
    data: { full_name: CUSTOMER, phone, gender: "female" },
  });
  expect(customer.status(), await customer.text()).toBe(201);
  const customerId = ((await customer.json()) as { id: string }).id;
  const services = (await (await page.request.get("/api/v1/services")).json()) as Service[];
  const service = services.find((s) => s.active && s.prices.all != null);
  expect(service, "seeded menu has a flat-priced service").toBeDefined();
  serviceName = service?.name ?? "";
  // The walk-in is recorded "now", whatever the hour: the technician's grid widens to show work
  // outside opening hours (CI 2026-11-08 00:03 ICT hid a visit before 09:00; the old 20:00+
  // workaround moved it to 09:30 instead of fixing the grid).
  const visit = await page.request.post("/api/v1/visits", {
    headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
    data: {
      customer_id: customerId,
      lines: [{ service_id: service?.id, source: "one_off", staff_id: staffId, qty: 1 }],
      payments: [{ method: "cash", amount: service?.prices.all ?? 0 }],
    },
  });
  expect(visit.status(), await visit.text()).toBe(201);
  const booking = await page.request.post("/api/v1/appointments", {
    headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
    data: {
      customer_id: customerId,
      service_ids: [service?.id],
      staff_id: staffId,
      starts_at: `${spaDay(1)}T10:00:00+07:00`,
    },
  });
  // A retried worker reruns this seed: tomorrow 10:00 is then already this technician's booking
  // (same customer name), which the first attempt created. Anything else is a real failure.
  if (booking.status() === 409) {
    expect(((await booking.json()) as { code?: string }).code, "only our own earlier booking").toBe("appointment.staff_busy");
    return;
  }
  expect(booking.status(), await booking.text()).toBe(201);
}

async function signInAsTechnician(browser: Browser): Promise<void> {
  const run = credentials();
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto("/dang-nhap");
  await page.getByLabel("Số điện thoại").fill(run.techPhone ?? "");
  await page.getByLabel("Mật khẩu").fill(run.techPassword ?? "");
  await page.getByRole("button", { name: "Đăng nhập" }).click();
  await expect(page).not.toHaveURL(/\/dang-nhap/);
  mkdirSync(dirname(techState), { recursive: true });
  await context.storageState({ path: techState });
  await context.close();
}

test.describe("KTV: màn của tôi", () => {
  test.beforeAll(async ({ browser }) => {
    const context = await browser.newContext({ storageState: ownerState });
    await seedTechnicianDay(await context.newPage());
    await context.close();
    await signInAsTechnician(browser);
  });

  test("Công và lương của tôi: summary first, a calm calendar, a day opens its detail", async ({ browser }) => {
    const context = await browser.newContext({ storageState: techState });
    const page = await context.newPage();
    await page.goto("/nhan-vien");
    await expect(page.getByRole("heading", { level: 1, name: "Công và lương của tôi" })).toBeVisible();
    const summary = page.getByRole("region", { name: "Tổng quan tháng" });
    await expect(summary.getByText("Chưa đánh giá")).toBeVisible();
    // The work table ends in the payslip's base and its commission.
    const work = page.getByRole("region", { name: "Dịch vụ đã làm" });
    await expect(work.getByRole("row", { name: /^Tổng tính hoa hồng/ })).toBeVisible();
    await expect(work.getByRole("row", { name: /^Thành hoa hồng làm 5%/ })).toBeVisible();
    await expectAccessible(page, "my-month");

    const calendar = page.getByRole("list", { name: /Lịch công/ });
    const todayCell = calendar.getByRole("button", { name: /Đi làm, muộn \d+ phút/ });
    await expect(todayCell).toContainText("′");
    await todayCell.click();
    const sheet = page.getByRole("dialog", { name: /^Ngày / });
    // A retried worker reruns the seed: the same customer may then have two visits that day.
    await expect(sheet.getByText(CUSTOMER).first()).toBeVisible();
    // The row header is the service name, then its type ("Dịch vụ lẻ") on a second line.
    await expect(sheet.getByRole("rowheader", { name: `${serviceName} Dịch vụ lẻ` })).toBeVisible();
    await expect(sheet.getByText("Dịch vụ lẻ")).toBeVisible();
    await expect(sheet.getByText(/^Muộn \d+ phút$/)).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`day=${spaDay()}`));
    await expectAccessible(page, "my-day-sheet");
    await expect(sheet.getByRole("button", { name: /Ngày sau/ })).toBeDisabled();
    await sheet.getByRole("button", { name: /Ngày trước/ }).click();
    await expect(page).toHaveURL(new RegExp(`day=${spaDay(-1)}`));
    await expect(sheet.getByText(CUSTOMER)).toHaveCount(0);
    await page.keyboard.press("Escape");
    await expect(sheet).toBeHidden();
    await expect(page.getByText(phone)).toHaveCount(0);

    await page.setViewportSize({ width: 390, height: 844 });
    await expect(calendar).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    await context.close();
  });

  test("Khách của tôi: own blocks on a week calendar, read-only, no phone", async ({ browser }) => {
    const context = await browser.newContext({ storageState: techState });
    const page = await context.newPage();
    await page.goto(`/khach-cua-toi?ngay=${spaDay(1)}`);
    await expect(page.getByRole("heading", { level: 1, name: "Khách của tôi" })).toBeVisible();
    const tomorrow = page.getByRole("button", { name: new RegExp(`^10:00–\\d{2}:\\d{2} ${CUSTOMER}, Chưa đến`) });
    await expect(tomorrow).toBeVisible();
    await expect(page.getByRole("button", { name: /Đặt lịch/ })).toHaveCount(0);
    await tomorrow.click();
    const detail = page.getByRole("dialog", { name: new RegExp(`^10:00–.* · ${CUSTOMER}$`) });
    await expect(detail.getByText("Quầy đặt cho bạn")).toBeVisible();
    await expectAccessible(page, "my-calendar-entry");
    await page.keyboard.press("Escape");
    await expect(detail).toBeHidden();

    // Today's visit sits on today's column (it may share the week with tomorrow), whatever the
    // hour: the grid widens past opening hours for the technician's own work (gridSpan), so a run
    // after midnight in Vietnam still finds the 00:xx visit on the grid.
    await page.goto("/khach-cua-toi?kieu=ngay");
    await expect(page.getByRole("group", { name: /Lịch của tôi ngày/ })).toBeVisible();
    await expect(page.getByRole("button", { name: new RegExp(`${CUSTOMER}, (Đang làm|Xong), Vãng lai`) })).toBeVisible();
    await expectAccessible(page, "my-calendar-day");
    await expect(page.getByText(phone)).toHaveCount(0);
    await expect(page.locator("body")).not.toContainText("+84");

    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/khach-cua-toi");
    await expect(page.getByRole("group", { name: /Lịch của tôi ngày/ })).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    await page.getByRole("button", { name: "Danh sách tuần" }).click();
    await expect(page.getByRole("button", { name: new RegExp(CUSTOMER) }).first()).toBeVisible();
    expect(await sidewaysOverflow(page)).toBe(0);
    await context.close();
  });

  // EM 2026-11-08: their own evaluation history, from their own path only.
  test("Lịch sử đánh giá: own months only, read-only, 390 dark", async ({ browser }) => {
    const context = await browser.newContext({ storageState: techState, viewport: { width: 390, height: 844 } });
    const page = await context.newPage();
    await page.goto("/nhan-vien");
    await expect(page.getByRole("heading", { level: 1, name: "Công và lương của tôi" })).toBeVisible();
    const fromMe = page.waitForResponse((r) => r.url().includes("/api/v1/me/evaluations/history"));
    await page.getByRole("button", { name: "Xem 12 tháng gần nhất" }).click();
    expect((await fromMe).status()).toBe(200);
    const list = page.getByRole("list", { name: "Các tháng của bạn" });
    await expect(list).toBeVisible();
    await expect(list.getByRole("link", { name: /^Chấm/ })).toHaveCount(0);
    await expect(page.getByRole("link", { name: /Xem mọi ghi chú/ })).toHaveCount(0);
    expect(await sidewaysOverflow(page)).toBe(0);
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "my-evaluation-history-390-dark");
    // The managers' route stays closed to them.
    expect((await page.request.get("/api/v1/evaluations/history?staff_id=00000000-0000-0000-0000-000000000000")).status()).toBe(403);
    await context.close();
  });

  test("screenshots 1280 / 390, light / dark (TECH_SHOTS_DIR)", async ({ browser }) => {
    const dir = process.env["TECH_SHOTS_DIR"];
    test.skip(!dir, "set TECH_SHOTS_DIR to capture");
    test.setTimeout(5 * 60_000);
    for (const scheme of ["light", "dark"] as const) {
      for (const width of [1280, 390]) {
        const context = await browser.newContext({
          storageState: techState,
          colorScheme: scheme,
          viewport: { width, height: width > 600 ? 900 : 844 },
        });
        const page = await context.newPage();
        const shot = async (name: string) => {
          await page.waitForTimeout(300);
          await page.screenshot({ path: join(dir ?? "", `${name}-${String(width)}-${scheme}.png`), fullPage: true });
        };
        await page.goto("/nhan-vien");
        await expect(page.getByRole("region", { name: "Tổng quan tháng" })).toBeVisible();
        await shot("month");
        await page.goto(`/nhan-vien?day=${spaDay()}`);
        await expect(page.getByRole("dialog", { name: /^Ngày / }).getByText(CUSTOMER)).toBeVisible();
        await page.screenshot({ path: join(dir ?? "", `day-${String(width)}-${scheme}.png`) });
        await page.goto(`/khach-cua-toi?ngay=${spaDay(1)}`);
        await expect(page.getByRole("button", { name: new RegExp(CUSTOMER) }).first()).toBeVisible();
        await shot("calendar");
        if (width > 600) {
          await page.goto(`/khach-cua-toi?ngay=${spaDay(1)}&kieu=ds`);
        } else {
          await page.getByRole("button", { name: "Danh sách tuần" }).click();
        }
        await expect(page.getByRole("button", { name: new RegExp(CUSTOMER) }).first()).toBeVisible();
        await shot("calendar-list");
        await context.close();
      }
    }
  });
});
