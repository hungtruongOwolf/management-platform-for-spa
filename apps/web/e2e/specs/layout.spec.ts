/**
 * Layout audit (EM 2026-11-05, rounds 2 and 3):
 * - no hover/press effect moves a neighbour;
 * - every screen holds from 320 to 1920 px, swept in 80 px steps, plus 1280×800 at 200 % zoom
 *   (= a 640×400 viewport): no sideways scroll, no overlapping siblings, no clipped text;
 * - no focus ring covers other text (at desktop and phone widths).
 * Screenshots of every screen at 360 / 768 / 1280 / 1440 are attached to the report.
 */
import { expect, test, type Page } from "@playwright/test";
import { ownerState, stack } from "../stack";
import { interactiveSample, shiftsOnInteraction } from "../support/layout";
import {
  clippedText,
  focusRingsOverText,
  overlappingSiblings,
  sidewaysOverflow,
} from "../support/responsive";
import { uniquePhone } from "../support/run";

test.use({ storageState: ownerState, reducedMotion: "reduce" });

const SCREENS: [string, string][] = [
  ["tong-quan", "/"],
  ["quay", "/quay"],
  ["quay-lich", "/quay?view=lich"],
  ["khach-hang", "/khach-hang"],
  ["bao-cao", "/bao-cao"],
  ["so-doanh-thu", "/bao-cao?tab=ledger"],
  ["goi-lieu-trinh", "/goi-lieu-trinh"],
  ["khach-lau-khong-quay-lai", "/khach-lau-khong-quay-lai"],
  ["nhat-ky-ghi-chu", "/nhat-ky-ghi-chu"],
  ["bang-gia", "/bang-gia"],
  ["bang-gia-goi", "/bang-gia/goi"],
  ["cai-dat", "/cai-dat"],
];

/** 320, 400, …, 1920: every layout step in between the breakpoints gets hit. */
const SWEEP = Array.from({ length: 21 }, (_, i) => 320 + i * 80);
const SHOTS = new Set([360, 400, 768, 1280, 1440]);
/** 1280×800 at 200 % browser zoom lays out like a 640×400 viewport. */
const ZOOM_200 = { width: 640, height: 400 };

let profilePath = "";
test.beforeAll(async ({ browser }) => {
  // A real profile page (breadcrumbs, gender chip, two columns) joins the sweep.
  const ctx = await browser.newContext({
    storageState: ownerState,
    baseURL: stack.webOrigin,
  });
  const res = await ctx.request.post("/api/v1/customers", {
    headers: { Origin: stack.webOrigin },
    data: {
      full_name: "Nguyễn Thị Bố Cục Rất Dài Tên Để Thử Xuống Dòng",
      phone: uniquePhone(),
      gender: "female",
    },
  });
  expect(res.status()).toBe(201);
  profilePath = `/khach-hang/${((await res.json()) as { id: string }).id}`;
  await ctx.close();
});

async function open(page: Page, path: string) {
  await page.goto(path);
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await page.waitForLoadState("networkidle");
}

const screens = () => [...SCREENS, ["ho-so", profilePath] as [string, string]];

for (const [name, path] of SCREENS) {
  test(`hover and press move nothing else on ${name}`, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page, path);
    const controls = await interactiveSample(page);
    expect(controls.length).toBeGreaterThan(0);
    for (const control of controls) {
      const label =
        (await control.getAttribute("aria-label")) ??
        (await control.innerText()).slice(0, 30);
      const shifts = await shiftsOnInteraction(page, control);
      expect(shifts, `hover/press on "${label}" moved other elements`).toEqual(
        [],
      );
    }
  });
}

test("every screen holds from 320 to 1920 px and at 200 % zoom", async ({
  page,
}, testInfo) => {
  test.setTimeout(10 * 60_000);
  const problems: string[] = [];
  for (const [name, path] of screens()) {
    const sizes = [...SWEEP.map((width) => ({ width, height: 900 })), ZOOM_200];
    let first = true;
    for (const size of sizes) {
      await page.setViewportSize(size);
      // Load once per screen, then resize like a user dragging the window (plus one reload at 200 %).
      if (first || size === ZOOM_200) await open(page, path);
      else await page.waitForTimeout(60);
      first = false;
      const at = `${name} @ ${String(size.width)}×${String(size.height)}`;
      const overflow = await sidewaysOverflow(page);
      if (overflow > 0)
        problems.push(`${at}: scrolls sideways by ${String(overflow)}px`);
      for (const o of await overlappingSiblings(page))
        problems.push(`${at}: overlap ${o}`);
      for (const c of await clippedText(page))
        problems.push(`${at}: clipped ${c}`);
      if (SHOTS.has(size.width) && size.height === 900) {
        await testInfo.attach(`${name}-${String(size.width)}`, {
          body: await page.screenshot({ fullPage: true }),
          contentType: "image/png",
        });
      }
    }
  }
  expect(problems).toEqual([]);
});

/**
 * EM round 5 item 2: "navbar bên trái … bị mất chữ" — every label whole, no sideways scroll. EM
 * round 6: and on ONE line — the open sidebar is wide enough for the longest label.
 */
test("sidebar labels are whole and on one line, open or folded, at 1024 / 1280 / 1440", async ({
  page,
}) => {
  const cut = () =>
    page.evaluate(() => {
      const side = document.querySelector<HTMLElement>(".sidebar");
      if (!side) return ["no sidebar"];
      const out: string[] = [];
      if (side.scrollWidth > side.clientWidth)
        out.push(`sidebar scrolls sideways by ${String(side.scrollWidth - side.clientWidth)}px`);
      const edge = side.getBoundingClientRect().right;
      for (const label of Array.from(side.querySelectorAll<HTMLElement>(".nav-label, .nav-group-label"))) {
        const r = label.getBoundingClientRect();
        if (r.width <= 1) continue; // the folded rail keeps labels for screen readers only
        const text = label.textContent.trim();
        if (label.scrollWidth > label.clientWidth + 1 || r.right > edge) out.push(`cut: ${text}`);
        const line = parseFloat(getComputedStyle(label).lineHeight);
        if (r.height > line * 1.5) out.push(`wraps: ${text} (${String(Math.round(r.height))}px tall)`);
      }
      return out;
    });
  for (const width of [1024, 1280, 1440]) {
    await page.setViewportSize({ width, height: 800 });
    await open(page, "/");
    expect(await cut(), `open sidebar @ ${String(width)}px`).toEqual([]);
    await expect(page.locator(".sidebar .nav-label").filter({ hasText: "Khách lâu chưa quay lại" })).toBeVisible();
  }
  await page.getByRole("button", { name: "Thu gọn thanh bên" }).click();
  expect(await cut(), "folded rail @ 1440px").toEqual([]);
  await page.getByRole("button", { name: "Mở rộng thanh bên" }).click();
  await page.setViewportSize({ width: 900, height: 800 });
  expect(await cut(), "tablet rail @ 900px").toEqual([]);
});

/**
 * EM round 6 item 1 (/quay scrolled down): the sidebar stopped half way with the page background
 * under it, a phantom empty area followed the content, and the visits table scrolled inside its
 * own card. With a full day of visits, scrolled to the bottom of every screen: the sidebar spans
 * the viewport, the document is no taller than the shell, and no table scrolls vertically.
 */
test("the page scrolls as one: full-height sidebar, no phantom area, no nested table scroll", async ({
  page,
}) => {
  test.setTimeout(3 * 60_000);
  const services = (await (await page.request.get("/api/v1/services")).json()) as {
    id: string;
    active: boolean;
    prices: { all?: number | null; female?: number | null };
  }[];
  const service = services.find((s) => s.active && (s.prices.female ?? s.prices.all) != null);
  const staff = (await (await page.request.get("/api/v1/staff")).json()) as { id: string }[];
  expect(service && staff[0], "seeded menu and a technician").toBeTruthy();
  const price = service?.prices.female ?? service?.prices.all ?? 0;
  for (let i = 0; i < 12; i++) {
    const customer = await page.request.post("/api/v1/customers", {
      headers: { Origin: stack.webOrigin },
      data: { full_name: `Khách Cuộn Trang ${String(i + 1)}`, phone: uniquePhone(), gender: "female" },
    });
    expect(customer.status()).toBe(201);
    const visit = await page.request.post("/api/v1/visits", {
      headers: { Origin: stack.webOrigin, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: ((await customer.json()) as { id: string }).id,
        lines: [{ service_id: service?.id, source: "one_off", staff_id: staff[0]?.id }],
        payments: [{ method: "cash", amount: price }],
      },
    });
    expect(visit.status()).toBe(201);
  }

  const problems: string[] = [];
  for (const size of [
    { width: 1280, height: 800 },
    { width: 1920, height: 965 },
  ]) {
    await page.setViewportSize(size);
    for (const [name, path] of screens()) {
      await open(page, path);
      if (path === "/quay") await expect(page.locator("main tbody tr")).not.toHaveCount(0);
      const at = `${name} @ ${String(size.width)}×${String(size.height)}`;
      const found = await page.evaluate(async () => {
        const out: string[] = [];
        const root = document.documentElement;
        window.scrollTo(0, root.scrollHeight);
        await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
        const shell = document.querySelector<HTMLElement>(".shell");
        const side = document.querySelector<HTMLElement>(".sidebar");
        if (!shell || !side) return ["no shell or sidebar"];
        const extra = root.scrollHeight - shell.offsetHeight;
        if (extra > 2) out.push(`document ${String(extra)}px taller than the shell`);
        const r = side.getBoundingClientRect();
        if (r.top > 0.5 || r.bottom < window.innerHeight - 0.5)
          out.push(`sidebar spans ${String(Math.round(r.top))}→${String(Math.round(r.bottom))} of ${String(window.innerHeight)}`);
        for (const el of Array.from(document.querySelectorAll<HTMLElement>("main *"))) {
          const style = getComputedStyle(el);
          if (!/auto|scroll/.test(style.overflowY)) continue;
          if (el.scrollHeight > el.clientHeight + 1)
            out.push(`<${el.tagName.toLowerCase()} class="${el.className}"> scrolls vertically (${String(el.scrollHeight)} > ${String(el.clientHeight)})`);
        }
        return out;
      });
      for (const p of found) problems.push(`${at}: ${p}`);
    }
  }
  expect(problems).toEqual([]);
});

/**
 * Phones get their own shapes (EM round 4 item 3, design-system § Mobile patterns): a 5-tab bar,
 * filters behind "Bộ lọc" in a bottom sheet glued to the bottom edge, cards instead of tables,
 * and 44 px targets for every control the thumb uses.
 */
test("phones: tab bar, filter sheet, cards instead of tables, 44 px targets", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await open(page, "/khach-lau-khong-quay-lai?follow=all&days=7");

  const tabs = page.getByRole("navigation", { name: "Thanh điều hướng" });
  await expect(tabs.getByRole("link")).toHaveText(
    // The owner books appointments: "Lịch hẹn" takes the home tab (EM round 4); home is under "Thêm".
    ["Lịch hẹn", "Quầy", "Khách", "Báo cáo"],
  );
  await expect(
    tabs.getByRole("button", { name: "Thêm màn hình" }),
  ).toBeVisible();

  // Search stays on the page; the other filters move into a sheet.
  const filters = page.getByRole("search", { name: "Lọc khách vắng" });
  await expect(filters.getByRole("searchbox")).toBeVisible();
  await expect(page.getByLabel("Vắng từ")).toHaveCount(0);
  await filters.getByRole("button", { name: /^Bộ lọc/ }).click();
  const sheet = page.getByRole("dialog", { name: "Lọc khách vắng" });
  await expect(sheet.getByLabel("Vắng từ")).toBeVisible();
  const box = await sheet.boundingBox();
  expect(box && Math.round(box.y + box.height)).toBe(844);
  expect(box?.width).toBe(390);
  await sheet.getByLabel("Vắng từ").selectOption("14");
  await expect(
    filters.getByRole("button", { name: "Bộ lọc, 1 đang chọn" }),
  ).toBeAttached();
  await sheet.getByRole("button", { name: "Xóa bộ lọc" }).click();
  await sheet.getByRole("button", { name: "Xem kết quả" }).click();
  await expect(sheet).toBeHidden();
  await expect(page).not.toHaveURL(/days=14/);

  // No wide table anywhere on the management lists: cards (a named list) instead.
  for (const path of [
    "/khach-lau-khong-quay-lai?follow=all",
    "/goi-lieu-trinh?status=all",
    "/bao-cao?tab=ledger",
    "/bao-cao",
  ]) {
    await open(page, path);
    await expect(
      page.getByRole("table"),
      `${path} shows a table on a phone`,
    ).toHaveCount(0);
  }

  // Thumb controls are at least 44 px tall (WCAG 2.5.5, HIG 44 pt).
  for (const [name, path] of screens()) {
    await open(page, path);
    const small = await page.evaluate(() =>
      Array.from(
        document.querySelectorAll<HTMLElement>(
          ".btn, .tab, .filter-trigger, .icon-btn, main select, main input:not([type=checkbox], [type=radio], [type=hidden], [type=date])",
        ),
      )
        .filter((el) => el.offsetParent !== null)
        .map(
          (el) =>
            [
              el.getAttribute("aria-label") ??
                el.textContent.trim().slice(0, 30),
              Math.round(el.getBoundingClientRect().height),
            ] as const,
        )
        .filter(([, h]) => h < 44)
        .map(([label, h]) => `${label} (${String(h)}px)`),
    );
    expect(small, `${name}: controls under 44 px`).toEqual([]);
  }
});

// EM 2026-11-07 (image 51): a textarea inside `.field` lost to the one-line field rule: 44 px tall,
// no top padding. Every textarea is the field box with room for three lines and resizes only
// vertically.
test("textareas are three-line fields with the control corners", async ({ page }) => {
  await open(page, "/khach-hang");
  await page.getByRole("button", { name: "Khách mới", exact: true }).click();
  const note = page.getByRole("form", { name: "Khách mới" }).getByLabel(/Lưu ý sức khỏe/);
  await expect(note).toBeVisible();
  const box = await note.evaluate((el) => {
    const s = getComputedStyle(el);
    return {
      height: el.getBoundingClientRect().height,
      radius: s.borderTopLeftRadius,
      paddingTop: s.paddingTop,
      resize: s.resize,
    };
  });
  expect(box.height).toBeGreaterThanOrEqual(88);
  expect(box).toMatchObject({ radius: "10px", paddingTop: "10px", resize: "vertical" });
});

test("focus rings never cover neighbouring text", async ({ page }) => {
  test.setTimeout(5 * 60_000);
  const problems: string[] = [];
  for (const width of [1280, 360]) {
    await page.setViewportSize({ width, height: 900 });
    for (const [name, path] of screens()) {
      await open(page, path);
      for (const p of await focusRingsOverText(page))
        problems.push(`${name} @ ${String(width)}: ${p}`);
    }
  }
  expect(problems).toEqual([]);
});
