import { expect, test, type Browser, type Page } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { ownerState, runFile } from "../stack";
import { WRITE, flatService, spaDay } from "../support/fixtures";
import { sidewaysOverflow } from "../support/responsive";
import { credentials, expectAccessible, uniquePhone } from "../support/run";

/**
 * "Ai ghi, ai sửa" on each record (ADR-0022, spec record-history, wave 1) on the real API: the
 * owner reads who recorded a visit and what changed; a manager by default sees none of it.
 */

test.use({ storageState: ownerState });
test.describe.configure({ mode: "serial" });

const managerState = join(dirname(runFile), "manager.json");

interface VisitDto {
  id: string;
  version: number;
  lines: { id: string }[];
  edited?: boolean;
}

const stamp = String(Date.now()).slice(-6);
const NAME = `Lê Thị Lịch ${stamp}`;
let visit: VisitDto;
let staffA = "";
let staffB = "";
let price = 0;
let serviceId = "";

/** A customer with one paid one-off visit by technician A; a second technician B to switch to. */
async function seedVisit(page: Page): Promise<void> {
  const service = await flatService(page.request);
  serviceId = service.id;
  price = service.prices.all ?? 0;
  const staff = (await (await page.request.get("/api/v1/staff")).json()) as { id: string; full_name: string }[];
  staffA = staff[0]?.id ?? "";
  const created = await page.request.post("/api/v1/staff", {
    headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
    data: { full_name: `KTV Đổi ${stamp}`, position: "Kỹ thuật viên" },
  });
  expect(created.status(), await created.text()).toBe(201);
  staffB = ((await created.json()) as { id: string }).id;
  const customer = await page.request.post("/api/v1/customers", {
    headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
    data: { full_name: NAME, phone: uniquePhone(), gender: "female" },
  });
  expect(customer.status()).toBe(201);
  const reply = await page.request.post("/api/v1/visits", {
    headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
    data: {
      customer_id: ((await customer.json()) as { id: string }).id,
      lines: [{ service_id: service.id, source: "one_off", staff_id: staffA }],
      payments: [{ method: "cash", amount: price }],
    },
  });
  expect(reply.status(), await reply.text()).toBe(201);
  visit = (await reply.json()) as VisitDto;
}

/** Opens the visit's details at /quay and returns them. */
async function openVisit(page: Page, which: "only" | "current" = "only") {
  await page.goto("/quay");
  const rows = page.getByRole(page.viewportSize()?.width === 1280 ? "row" : "listitem").filter({ hasText: NAME });
  const row = which === "current" ? rows.filter({ hasNotText: "Đã thay" }) : rows;
  await row.getByRole("button", { name: new RegExp(`^${NAME}`) }).click();
  return page.getByTestId("visit-details");
}

async function signInAsManager(browser: Browser): Promise<void> {
  const run = credentials();
  // Signed out: this file's default storage state is the owner's.
  const context = await browser.newContext({ storageState: { cookies: [], origins: [] } });
  const page = await context.newPage();
  await page.goto("/dang-nhap");
  await page.getByLabel("Số điện thoại").fill(run.managerPhone ?? "");
  await page.getByLabel("Mật khẩu").fill(run.managerPassword ?? "");
  await page.getByRole("button", { name: "Đăng nhập" }).click();
  await expect(page).not.toHaveURL(/\/dang-nhap/);
  mkdirSync(dirname(managerState), { recursive: true });
  await context.storageState({ path: managerState });
  await context.close();
}

test.describe("lịch sử trên từng bản ghi (ADR-0022)", () => {
  test.beforeAll(async ({ browser }) => {
    const context = await browser.newContext({ storageState: ownerState });
    await seedVisit(await context.newPage());
    await context.close();
  });

  test("the owner opens a visit and reads 'Ghi bởi …'; a technician change makes it 'Đã sửa'", async ({ page }) => {
    const owner = credentials().fullName;
    await page.setViewportSize({ width: 1280, height: 900 });
    let details = await openVisit(page);
    const trace = details.getByRole("button", { name: new RegExp(`^Ghi bởi ${owner} · \\d{2}:\\d{2}$`) });
    await expect(trace).toBeVisible();
    await expect(trace).toHaveAttribute("aria-expanded", "false");
    await trace.click();
    await expect(trace).toHaveAttribute("aria-expanded", "true");
    await expect(details.getByRole("list", { name: /^Lịch sử lượt / }).getByText(new RegExp(`^${owner} ghi lượt · Thu `))).toBeVisible();
    // Reading the history never folds the row it sits in.
    await details.getByText(new RegExp(`^${owner} ghi lượt`)).click();
    await expect(details).toBeVisible();
    await expectAccessible(page, "record-history-quay-1280");
    await page.emulateMedia({ colorScheme: "dark" });
    await expectAccessible(page, "record-history-quay-1280-dark");
    await page.emulateMedia({ colorScheme: "light" });

    // "Sửa giờ / KTV" (visit.update): another technician counts as an edit (D9).
    const patched = await page.request.patch(`/api/v1/visits/${visit.id}`, {
      headers: { ...WRITE, "If-Match": `"${String(visit.version)}"` },
      data: { lines: [{ id: visit.lines[0]?.id, staff_id: staffB }] },
    });
    expect(patched.status(), await patched.text()).toBe(200);
    visit = (await patched.json()) as VisitDto;
    expect(visit.edited).toBe(true);

    details = await openVisit(page);
    await expect(page.getByRole("row").filter({ hasText: NAME }).getByText("Đã sửa", { exact: true })).toBeVisible();
    const edited = details.getByRole("button", { name: new RegExp(`^Ghi bởi ${owner} · \\d{2}:\\d{2} · đã sửa 1 lần$`) });
    await edited.click();
    await expect(details.getByText(`KTV: `, { exact: false }).filter({ hasText: `→ KTV Đổi ${stamp}` })).toBeVisible();
  });

  test("Sửa lượt: the old visit reads 'Đã thay', the new one 'Đã sửa', one story from either", async ({ page }) => {
    const corrected = await page.request.post(`/api/v1/visits/${visit.id}/correct`, {
      headers: { ...WRITE, "If-Match": `"${String(visit.version)}"`, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: (await (await page.request.get(`/api/v1/visits/${visit.id}`)).json() as { customer_id: string }).customer_id,
        lines: [{ service_id: serviceId, source: "one_off", staff_id: staffB, qty: 2 }],
        // The visit as it should have been: its whole payment (the server works out the difference).
        payments: [{ method: "cash", amount: 2 * price }],
        reason: "Khách làm hai lần",
      },
    });
    expect(corrected.status(), await corrected.text()).toBe(201);

    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto("/quay");
    const rows = page.getByRole("row").filter({ hasText: NAME });
    await expect(rows).toHaveCount(2);
    await expect(rows.filter({ hasText: "Đã thay" })).toHaveCount(1);
    await expect(rows.filter({ hasText: "Đã thay" }).getByText("Đã sửa", { exact: true })).toHaveCount(0);
    await expect(rows.filter({ hasNotText: "Đã thay" }).getByText("Đã sửa", { exact: true })).toBeVisible();

    const details = await openVisit(page, "current");
    const trace = details.getByRole("button", { name: /^Ghi bởi .+ · đã sửa 2 lần$/ });
    await trace.click();
    await expect(details.getByText(/sửa lượt \(tính lại tiền\) · Chênh lệch \+/)).toBeVisible();
    await expect(details.getByText(/lý do: (Sửa lượt: )?Khách làm hai lần/)).toBeVisible();
  });

  test("phones (360 px): the history line is a 44 px button at the end of the card, no sideways scroll", async ({ page }) => {
    await page.setViewportSize({ width: 360, height: 800 });
    const details = await openVisit(page, "current");
    const trace = details.getByRole("button", { name: /^Ghi bởi / });
    await expect(trace).toBeVisible();
    expect((await trace.boundingBox())?.height ?? 0).toBeGreaterThanOrEqual(44);
    await trace.click();
    await expect(details.getByRole("list", { name: /^Lịch sử lượt / })).toBeVisible();
    // Still open: a tap in the history is no tap on the card.
    await details.getByRole("list", { name: /^Lịch sử lượt / }).locator("li").first().click();
    await expect(trace).toHaveAttribute("aria-expanded", "true");
    expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, "record-history-quay-360");
  });

  test("Sổ doanh thu: the time opens the sale's history in a sheet, at 1280 and 360 px", async ({ page }) => {
    for (const width of [1280, 360]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/bao-cao?tab=ledger");
      const time = page.getByRole("button", { name: new RegExp(`^\\d{2}:\\d{2}, xem hóa đơn của chị ${NAME}$`) }).first();
      await time.click();
      const sheet = page.getByRole("dialog", { name: `Hóa đơn dịch vụ · chị ${NAME}` });
      await expect(sheet).toBeVisible();
      await expect(sheet.getByRole("button", { name: /^Ghi bởi / })).toHaveAttribute("aria-expanded", "true");
      await expect(sheet.getByRole("list", { name: /^Lịch sử hóa đơn dịch vụ / })).toBeVisible();
      if (width === 360) expect(await sidewaysOverflow(page)).toBe(0);
      await expectAccessible(page, `record-history-ledger-${String(width)}`);
      await sheet.getByRole("button", { name: "Đóng" }).last().click();
      await expect(sheet).toBeHidden();
    }
  });

  test("the customer profile: 'Tạo bởi …' and the old and new phone for the owner", async ({ page }) => {
    const owner = credentials().fullName;
    const name = `Khách Đổi Số ${stamp}`;
    const created = await page.request.post("/api/v1/customers", {
      headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
      data: { full_name: name, phone: uniquePhone(), gender: "female" },
    });
    expect(created.status()).toBe(201);
    const customer = (await created.json()) as { id: string; version: number };
    const updated = await page.request.put(`/api/v1/customers/${customer.id}`, {
      headers: { ...WRITE, "If-Match": `"${String(customer.version)}"` },
      data: { full_name: name, phone: uniquePhone(), gender: "female" },
    });
    expect(updated.status()).toBe(200);

    for (const width of [1280, 360]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(`/khach-hang/${customer.id}`);
      const info = page.getByRole("article", { name: "Thông tin khách" });
      const trace = info.getByRole("button", { name: new RegExp(`^Tạo bởi ${owner} · \\d{2}/\\d{2}/\\d{4} · đã sửa 1 lần$`) });
      await trace.click();
      await expect(info.getByText(/^SĐT: 0\d{3} \d{3} \d{3} → 0\d{3} \d{3} \d{3}$/)).toBeVisible();
      if (width === 360) expect(await sidewaysOverflow(page)).toBe(0);
      await expectAccessible(page, `record-history-profile-${String(width)}`);
    }
  });

  test("a manager by default sees no history: no line, no 'Đã sửa', the API says 403", async ({ browser }) => {
    await signInAsManager(browser);
    const context = await browser.newContext({ storageState: managerState, viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    const refused = await page.request.get(`/api/v1/record-history/visit/${visit.id}`);
    expect(refused.status()).toBe(403);
    const day = (await (await page.request.get("/api/v1/visits?limit=100")).json()) as Record<string, unknown>[];
    expect(day.length).toBeGreaterThan(0);
    expect(day.every((v) => !("edited" in v))).toBe(true);

    await page.goto("/quay");
    const rows = page.getByRole("row").filter({ hasText: NAME });
    await expect(rows).toHaveCount(2);
    // "Đã thay" is a state everyone reads; "Đã sửa" is the history right's.
    await expect(rows.filter({ hasText: "Đã thay" })).toHaveCount(1);
    await expect(rows.getByText("Đã sửa", { exact: true })).toHaveCount(0);
    await rows.filter({ hasNotText: "Đã thay" }).getByRole("button", { name: new RegExp(`^${NAME}`) }).click();
    const details = page.getByTestId("visit-details");
    await expect(details).toBeVisible();
    await expect(details.getByRole("button", { name: /^Ghi bởi|^Đang tải ai ghi/ })).toHaveCount(0);
    await expectAccessible(page, "record-history-quay-manager");
    await context.close();
  });
});

/**
 * Wave 2 (design record-history §2.7–2.9): a booking's sheet and the list view's "Đã sửa"; a
 * person-day sheet's two books; the bonus / penalty log's ⋯. Owner sees them at 1280 and 360 px;
 * a manager by default does not, and the API refuses them.
 */
test.describe("lịch sử trên từng bản ghi, đợt 2", () => {
  const PERSON = `KTV Sổ ${stamp}`;
  const GUEST = `Lê Thị Hẹn ${stamp}`;
  let person = "";
  let booking = "";
  let bonus = "";

  test.beforeAll(async ({ browser }) => {
    const context = await browser.newContext({ storageState: ownerState });
    const request = (await context.newPage()).request;
    const service = await flatService(request);
    const customer = await request.post("/api/v1/customers", {
      headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
      data: { full_name: GUEST, phone: uniquePhone(), gender: "female" },
    });
    expect(customer.status()).toBe(201);
    // Booked today for tomorrow 10:00, then moved to 11:00: an edit (D9).
    const booked = await request.post("/api/v1/appointments", {
      headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
      data: { customer_id: ((await customer.json()) as { id: string }).id, service_ids: [service.id], starts_at: `${spaDay(1)}T10:00:00+07:00` },
    });
    expect(booked.status(), await booked.text()).toBe(201);
    const made = (await booked.json()) as { id: string; version: number; edited?: boolean };
    expect(made.edited).toBe(false);
    booking = made.id;
    const moved = await request.patch(`/api/v1/appointments/${booking}`, {
      headers: { ...WRITE, "If-Match": `"${String(made.version)}"` },
      data: { starts_at: `${spaDay(1)}T11:00:00+07:00` },
    });
    expect(moved.status(), await moved.text()).toBe(200);
    expect(((await moved.json()) as { edited?: boolean }).edited).toBe(true);
    // A person whose day is written twice and whose bonus is edited once.
    const created = await request.post("/api/v1/staff", {
      headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
      data: { full_name: PERSON, position: "Kỹ thuật viên" },
    });
    expect(created.status(), await created.text()).toBe(201);
    person = ((await created.json()) as { id: string }).id;
    for (const check_in of ["09:05", "09:30"]) {
      const saved = await request.put("/api/v1/attendance", {
        headers: WRITE,
        data: { entries: [{ staff_id: person, date: spaDay(), status: "present", check_in }] },
      });
      expect(saved.status(), await saved.text()).toBe(200);
    }
    const first = await request.post("/api/v1/adjustments", {
      headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
      data: { staff_id: person, kind: "bonus", amount: 100_000, reason: "Khách khen đích danh", occurred_on: spaDay() },
    });
    expect(first.status(), await first.text()).toBe(201);
    const edited = await request.put(`/api/v1/adjustments/${((await first.json()) as { id: string }).id}`, {
      headers: { ...WRITE, "Idempotency-Key": crypto.randomUUID() },
      data: { staff_id: person, kind: "bonus", amount: 150_000, reason: "Khách khen đích danh", occurred_on: spaDay() },
    });
    expect(edited.status(), await edited.text()).toBe(200);
    bonus = ((await edited.json()) as { id: string }).id;
    await context.close();
  });

  test("the booking: 'Đã sửa' in the list view, 'Ghi bởi …' at the end of its sheet", async ({ page }) => {
    const owner = credentials().fullName;
    for (const width of [1280, 360]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(`/quay?view=lich&kieu=ds&ngay=${spaDay(1)}`);
      const row = page.getByRole("list", { name: /^Lịch hẹn ngày/ }).getByRole("button", { name: new RegExp(GUEST) });
      await expect(row.getByText("Đã sửa", { exact: true })).toBeVisible();
      await row.click();
      const sheet = page.getByRole("dialog", { name: new RegExp(GUEST) });
      // Booked today for tomorrow: the meta says the day it was made (with the year on 31/12).
      const trace = sheet.getByRole("button", { name: new RegExp(`^Ghi bởi ${owner} · \\d{2}:\\d{2} \\d{2}/\\d{2}(/\\d{4})? · đã sửa 1 lần$`) });
      await expect(trace).toHaveAttribute("aria-expanded", "false");
      await trace.click();
      const lines = sheet.getByRole("list", { name: /^Lịch sử lịch hẹn / });
      await expect(lines.getByText(`${owner} đổi lịch`)).toBeVisible();
      await expect(lines.getByText(/^Giờ hẹn: /)).toBeVisible();
      if (width === 360) expect(await sidewaysOverflow(page)).toBe(0);
      await expectAccessible(page, `record-history-booking-${String(width)}`);
      await sheet.getByRole("button", { name: "Đóng" }).first().click();
      await expect(sheet).toBeHidden();
    }
  });

  test("the person-day sheet: each book's cell ends its fieldset; written twice = 'đã sửa 1 lần'", async ({ page }) => {
    const owner = credentials().fullName;
    for (const width of [1280, 360]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/nhan-vien");
      const card = page.getByRole("listitem").filter({ has: page.getByText(PERSON, { exact: true }) });
      await card.getByRole("button", { name: "Giờ & ghi chú" }).click();
      const sheet = page.getByRole("dialog", { name: PERSON });
      const attendance = sheet.getByRole("group", { name: "Chấm công" });
      const trace = attendance.getByRole("button", { name: new RegExp(`^Ghi bởi ${owner} · \\d{2}:\\d{2} · đã sửa 1 lần$`) });
      await trace.click();
      await expect(attendance.getByText("Giờ vào: 09:05 → 09:30")).toBeVisible();
      // The praise / complaints cell nobody wrote.
      await expect(sheet.getByRole("group", { name: /^Khen, phàn nàn/ }).getByRole("button", { name: "Chưa có ai ghi" })).toBeVisible();
      if (width === 360) expect(await sidewaysOverflow(page)).toBe(0);
      await expectAccessible(page, `record-history-person-day-${String(width)}`);
      await sheet.getByRole("button", { name: "Đóng" }).first().click();
      await expect(sheet).toBeHidden();
    }
  });

  test("the bonus log: ⋯ → the entry's history under its row, its edit one line", async ({ page }) => {
    const owner = credentials().fullName;
    for (const width of [1280, 360]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/nhan-vien?tab=month");
      await page.getByRole("button", { name: `Thao tác Thưởng 150.000đ của ${PERSON}` }).click();
      await page.getByRole("menuitem", { name: "Xem ai ghi, ai sửa" }).click();
      const trace = page.getByRole("button", { name: new RegExp(`^Ghi bởi ${owner} · \\d{2}:\\d{2} · đã sửa 1 lần$`) });
      await expect(trace).toHaveAttribute("aria-expanded", "true");
      await expect(trace).toBeFocused();
      const lines = page.getByRole("list", { name: `Lịch sử Thưởng 150.000đ của ${PERSON}` });
      await expect(lines.getByText(`${owner} sửa khoản thưởng`)).toBeVisible();
      await expect(lines.getByText("Số tiền: 100.000đ → 150.000đ")).toBeVisible();
      if (width === 360) expect(await sidewaysOverflow(page)).toBe(0);
      await expectAccessible(page, `record-history-adjustment-${String(width)}`);
    }
  });

  test("a manager by default: no history on these screens, the API refuses it", async ({ browser }) => {
    await signInAsManager(browser);
    const context = await browser.newContext({ storageState: managerState, viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    for (const path of [
      `/api/v1/record-history/appointment/${booking}`,
      `/api/v1/record-history/staff-day/${person}/${spaDay()}?book=attendance`,
      `/api/v1/record-history/adjustment/${bonus}`,
    ]) {
      expect((await page.request.get(path)).status(), path).toBe(403);
    }
    const listed = (await (await page.request.get(`/api/v1/appointments?from=${spaDay(1)}&to=${spaDay(1)}&marks=true`)).json()) as {
      appointments: Record<string, unknown>[];
    };
    expect(listed.appointments.every((a) => !("edited" in a))).toBe(true);
    await page.goto(`/quay?view=lich&kieu=ds&ngay=${spaDay(1)}`);
    const row = page.getByRole("list", { name: /^Lịch hẹn ngày/ }).getByRole("button", { name: new RegExp(GUEST) });
    await expect(row).toBeVisible();
    await expect(row.getByText("Đã sửa", { exact: true })).toHaveCount(0);
    await row.click();
    const sheet = page.getByRole("dialog", { name: new RegExp(GUEST) });
    await expect(sheet).toBeVisible();
    await expect(sheet.getByRole("button", { name: /^Ghi bởi|^Đang tải ai ghi/ })).toHaveCount(0);
    await expectAccessible(page, "record-history-booking-manager");
    await context.close();
  });
});

/**
 * Wave 3 (design record-history §2.10): the last element of a service's and a plan's edit form, of
 * "Quy tắc lương" and of "Số khách tối đa", above their buttons: "Sửa lần cuối bởi …". The owner
 * at 1280 and 360 px; a manager cannot open these screens, sends no request, and the API refuses.
 * Other specs share this spa: the service and the plan are made "ngừng bán", the rules and the
 * limit are put back as they were.
 */
test.describe("lịch sử trên từng bản ghi, đợt 3", () => {
  const SERVICE = `Gội thử lịch sử ${stamp}`;
  const PLAN = `Gói thử lịch sử ${stamp}`;
  const today = spaDay().split("-").reverse().join("/");
  let service = "";
  let plan = "";
  let grace = 0;

  test.beforeAll(async ({ browser }) => {
    const context = await browser.newContext({ storageState: ownerState });
    const request = (await context.newPage()).request;
    const categories = (await (await request.get("/api/v1/service-categories")).json()) as { id: string }[];
    const write = (female: number) => ({
      category_id: categories[0]?.id,
      name: SERVICE,
      description: null,
      duration_min: 45,
      recommended: false,
      active: false,
      prices: { female, male: 150_000 },
    });
    const made = await request.post("/api/v1/services", { headers: WRITE, data: write(150_000) });
    expect(made.status(), await made.text()).toBe(201);
    service = ((await made.json()) as { id: string }).id;
    const edited = await request.put(`/api/v1/services/${service}`, { headers: { ...WRITE, "If-Match": '"1"' }, data: write(170_000) });
    expect(edited.status(), await edited.text()).toBe(200);

    const flat = await flatService(request);
    const planBody = (price: number) => ({
      name: PLAN,
      price,
      validity_months: 6,
      gift_note: null,
      active: false,
      items: [{ service_id: flat.id, sessions: 10, bonus_sessions: 0 }],
    });
    const planned = await request.post("/api/v1/package-plans", { headers: WRITE, data: planBody(1_500_000) });
    expect(planned.status(), await planned.text()).toBe(201);
    plan = ((await planned.json()) as { id: string }).id;
    const repriced = await request.put(`/api/v1/package-plans/${plan}`, { headers: { ...WRITE, "If-Match": '"1"' }, data: planBody(1_800_000) });
    expect(repriced.status(), await repriced.text()).toBe(200);

    // The rules: one more minute of grace, then back (two saves, the rules unchanged after).
    const read = (await (await request.get("/api/v1/payroll/rules")).json()) as { version: number; rules: { late_grace_minutes: number } };
    grace = read.rules.late_grace_minutes;
    let version = read.version;
    for (const minutes of [grace + 1, grace]) {
      const saved = await request.put("/api/v1/payroll/rules", {
        headers: { ...WRITE, "If-Match": `"${String(version)}"` },
        data: { ...read.rules, late_grace_minutes: minutes },
      });
      expect(saved.status(), await saved.text()).toBe(200);
      version = ((await saved.json()) as { version: number }).version;
    }
    // "Số khách tối đa": 4, 6, then no limit again.
    for (const max of [4, 6, null]) {
      const saved = await request.put("/api/v1/appointment-settings", { headers: WRITE, data: { max_per_slot: max } });
      expect(saved.status(), await saved.text()).toBe(200);
    }
    await context.close();
  });

  test("a service's edit form: 'Sửa lần cuối bởi …' above its buttons, Giá Nữ old → new", async ({ page }) => {
    const owner = credentials().fullName;
    for (const width of [1280, 360]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/bang-gia");
      await page.getByRole("button", { name: `Sửa ${SERVICE}` }).click();
      const form = page.getByRole("form", { name: `Sửa: ${SERVICE}` });
      const trace = form.getByRole("button", { name: `Sửa lần cuối bởi ${owner} · ${today} · đã sửa 1 lần` });
      await expect(trace).toHaveAttribute("aria-expanded", "false");
      await trace.click();
      const lines = form.getByRole("list", { name: `Lịch sử dịch vụ ${SERVICE}` });
      await expect(lines.getByText(`${owner} thêm dịch vụ`)).toBeVisible();
      await expect(lines.getByText("Giá Nữ: 150.000đ → 170.000đ")).toBeVisible();
      if (width === 360) expect(await sidewaysOverflow(page)).toBe(0);
      await expectAccessible(page, `record-history-service-${String(width)}`);
      await form.getByRole("button", { name: "Hủy" }).click();
    }
  });

  test("a plan's edit form: its price old → new", async ({ page }) => {
    const owner = credentials().fullName;
    for (const width of [1280, 360]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/bang-gia/goi");
      await page.getByRole("button", { name: `Sửa mẫu gói ${PLAN}` }).click();
      const form = page.getByRole("form", { name: `Sửa mẫu gói: ${PLAN}` });
      const trace = form.getByRole("button", { name: `Sửa lần cuối bởi ${owner} · ${today} · đã sửa 1 lần` });
      await trace.click();
      await expect(form.getByRole("list", { name: `Lịch sử mẫu gói ${PLAN}` }).getByText("Giá: 1.500.000đ → 1.800.000đ")).toBeVisible();
      if (width === 360) expect(await sidewaysOverflow(page)).toBe(0);
      await expectAccessible(page, `record-history-plan-${String(width)}`);
      await form.getByRole("button", { name: "Hủy" }).click();
    }
  });

  test("'Quy tắc lương' and 'Số khách tối đa': every save after the first is an edit", async ({ page }) => {
    const owner = credentials().fullName;
    const edited = new RegExp(`^Sửa lần cuối bởi ${owner} · ${today} · đã sửa \\d+ lần$`);
    for (const width of [1280, 360]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/nhan-vien?tab=payroll");
      await page.getByRole("button", { name: "Quy tắc lương" }).click();
      const rules = page.getByRole("dialog", { name: "Quy tắc lương của spa" });
      await rules.getByRole("button", { name: edited }).click();
      const lines = rules.getByRole("list", { name: "Lịch sử quy tắc lương" });
      await expect(lines.getByText(`Số phút đi muộn bỏ qua: ${String(grace + 1)} → ${String(grace)}`)).toBeVisible();
      if (width === 360) expect(await sidewaysOverflow(page)).toBe(0);
      await expectAccessible(page, `record-history-payroll-rules-${String(width)}`);
      await rules.getByRole("button", { name: "Đóng" }).first().click();
      await expect(rules).toBeHidden();

      await page.goto("/quay?view=lich");
      await page.getByRole("button", { name: "Số khách tối đa", exact: true }).click();
      const limit = page.getByRole("dialog", { name: "Số khách tối đa mỗi khung giờ" });
      await limit.getByRole("button", { name: edited }).click();
      const saves = limit.getByRole("list", { name: "Lịch sử số khách tối đa" });
      await expect(saves.getByText("Số khách tối đa mỗi khung giờ: 4 → 6")).toBeVisible();
      if (width === 360) expect(await sidewaysOverflow(page)).toBe(0);
      await expectAccessible(page, `record-history-slot-limit-${String(width)}`);
      await limit.getByRole("button", { name: "Thôi" }).click();
      await expect(limit).toBeHidden();
    }
  });

  test("a manager: none of these screens open, no history request is sent, the API refuses", async ({ browser }) => {
    await signInAsManager(browser);
    const context = await browser.newContext({ storageState: managerState, viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    for (const path of [
      `/api/v1/record-history/service/${service}`,
      `/api/v1/record-history/package-plan/${plan}`,
      "/api/v1/record-history/settings/payroll-rules",
      "/api/v1/record-history/settings/appointment-settings",
    ]) {
      expect((await page.request.get(path)).status(), path).toBe(403);
    }
    const asked: string[] = [];
    page.on("request", (r) => {
      // The API's history routes, not the dev server's `src/features/record-history/*` modules.
      if (r.url().includes("/api/v1/record-history/")) asked.push(r.url());
    });
    await page.goto("/bang-gia");
    await expect(page.getByRole("heading", { name: "Bảng giá dịch vụ" })).toBeVisible();
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("button", { name: `Sửa ${SERVICE}` })).toHaveCount(0);
    await page.goto("/bang-gia/goi");
    await expect(page.getByRole("heading", { name: "Mẫu gói liệu trình" })).toBeVisible();
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("button", { name: `Sửa mẫu gói ${PLAN}` })).toHaveCount(0);
    const calendar = page.waitForResponse((r) => r.url().includes("/api/v1/appointments?"));
    await page.goto("/quay?view=lich");
    await calendar;
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("button", { name: /^Số khách tối đa/ })).toHaveCount(0);
    await page.goto("/nhan-vien");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("button", { name: "Quy tắc lương" })).toHaveCount(0);
    expect(asked).toEqual([]);
    await context.close();
  });
});
