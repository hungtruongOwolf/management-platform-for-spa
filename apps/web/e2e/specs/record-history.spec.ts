import { expect, test, type Browser, type Page } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { ownerState, runFile } from "../stack";
import { WRITE, flatService } from "../support/fixtures";
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
  const context = await browser.newContext();
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
        payments: [{ method: "cash", amount: price }],
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
    await expect(details.getByText(/lý do: Khách làm hai lần/)).toBeVisible();
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
