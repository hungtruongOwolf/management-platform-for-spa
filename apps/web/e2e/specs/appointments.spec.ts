import { expect, test, type Locator, type Page } from "@playwright/test";
import { ownerState, stack } from "../stack";
import { expectAccessible, uniquePhone } from "../support/run";

test.use({ storageState: ownerState });

const write = { Origin: stack.webOrigin };

interface Service {
  id: string;
  name: string;
  active: boolean;
  duration_min: number | null;
  prices: { all?: number | null };
}

function spaDay(offsetDays = 0): string {
  const now = new Date(Date.now() + 7 * 3_600_000 + offsetDays * 86_400_000);
  return now.toISOString().slice(0, 10);
}

/** A customer with a fresh "10 + 2" package; returns ids for API bookings. */
async function regular(page: Page, name: string) {
  const services = (await (
    await page.request.get("/api/v1/services")
  ).json()) as Service[];
  const service = services.find((s) => s.active && s.prices.all != null);
  expect(service, "seeded menu has a flat-priced service").toBeDefined();
  const plan = await page.request.post("/api/v1/package-plans", {
    headers: write,
    data: {
      name: `Gói lịch hẹn ${String(Date.now())}`,
      price: 1_000_000,
      validity_months: 6,
      gift_note: null,
      active: true,
      items: [{ service_id: service?.id, sessions: 10, bonus_sessions: 2 }],
    },
  });
  expect(plan.status()).toBe(201);
  const phone = uniquePhone();
  const created = await page.request.post("/api/v1/customers", {
    headers: write,
    data: { full_name: name, phone, gender: "female" },
  });
  expect(created.status()).toBe(201);
  const customer = ((await created.json()) as { id: string }).id;
  const staff = (await (await page.request.get("/api/v1/staff")).json()) as {
    id: string;
    full_name: string;
  }[];
  const sale = await page.request.post("/api/v1/sales", {
    headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
    data: {
      customer_id: customer,
      closer_staff_id: staff[0]?.id,
      lines: [{ package_plan_id: ((await plan.json()) as { id: string }).id }],
      payments: [{ method: "cash", amount: 1_000_000 }],
    },
  });
  expect(sale.status()).toBe(201);
  return {
    customer,
    phone,
    service: service as Service,
    staff: staff[0] as { id: string; full_name: string },
  };
}

test.describe("lịch hẹn (spec appointments.md)", () => {
  test("book on the day grid, reject a double booking, then arrive and complete into a visit", async ({
    page,
  }) => {
    const r = await regular(page, "Trần Thị Lịch Hẹn");

    // Book tomorrow 10:00 through the UI.
    await page.goto(`/quay?view=lich&ngay=${spaDay(1)}`);
    await expect(page.getByRole("heading", { name: /Lịch hẹn/ })).toBeVisible();
    await expect(
      page.getByRole("group", { name: /Lịch theo KTV/ }),
    ).toBeVisible();
    await expectAccessible(page, "lich-ngay");
    await page.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    const sheet = page.getByRole("dialog", { name: "Đặt lịch hẹn" });
    await sheet
      .getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" })
      .pressSequentially(r.phone);
    await sheet.getByRole("option", { name: /Trần Thị Lịch Hẹn/ }).click();
    // Picking a service adds it (round 5: no separate "Thêm" click).
    await sheet.getByLabel("Thêm dịch vụ").selectOption(r.service.id);
    await sheet.getByLabel("Giờ hẹn").selectOption({ value: String(10 * 60) });
    await sheet.getByLabel("KTV (không bắt buộc)").selectOption(r.staff.id);
    await expectAccessible(page, "lich-dat");
    await sheet.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    const opened = page.getByRole("dialog", {
      name: /10:00.*Trần Thị Lịch Hẹn/,
    });
    await expect(opened).toContainText("Chưa đến");
    await expect(opened).toContainText("Chưa thanh toán");
    await page.keyboard.press("Escape");
    await expect(opened).toBeHidden();
    await expect(
      page.getByRole("button", { name: /10:00.*Trần Thị Lịch Hẹn, Chưa đến/ }),
    ).toBeVisible();

    // The server refuses the same technician in an overlapping slot and names the booking.
    const clash = await page.request.post("/api/v1/appointments", {
      headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: r.customer,
        service_ids: [r.service.id],
        staff_id: r.staff.id,
        starts_at: `${spaDay(1)}T10:15:00+07:00`,
      },
    });
    expect(clash.status()).toBe(409);
    expect(((await clash.json()) as { code: string }).code).toBe(
      "appointment.staff_busy",
    );

    // Today 09:00: arrive, complete → the visit cart is filled; saving records the visit.
    const today = await page.request.post("/api/v1/appointments", {
      headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: r.customer,
        service_ids: [r.service.id],
        starts_at: `${spaDay()}T09:00:00+07:00`,
      },
    });
    expect(today.status()).toBe(201);
    await page.goto("/quay?view=lich");
    await page
      .getByRole("button", { name: /09:00.*Trần Thị Lịch Hẹn, Chưa đến/ })
      .click();
    const panel = page.getByRole("dialog", {
      name: /09:00.*Trần Thị Lịch Hẹn/,
    });
    // Picking the technician is a plain update, saved at once (EM 2026-11-07).
    await panel.getByLabel("KTV làm").selectOption(r.staff.id);
    await expect(panel.getByText("Đã lưu KTV.")).toBeVisible();
    await panel.getByRole("button", { name: "Chỉ báo đã đến" }).click();
    await expect(panel).toContainText("Đang làm");
    await panel.getByRole("button", { name: "Ghi lượt", exact: true }).click();
    await expect(page.getByText(/Đang ghi cho lịch hẹn 09:00/)).toBeVisible();
    const cart = page.getByRole("list", { name: "Dịch vụ trong lượt" });
    await expect(cart).toContainText("Trừ liệu trình");
    // The booked technician is filled in, so the line is already folded (EM 2026-11-07).
    await expect(cart).toContainText(`KTV ${r.staff.full_name}`);
    await expect(
      cart.getByRole("button", { name: /^Sửa dòng / }),
    ).toBeVisible();
    await expectAccessible(page, "quay-tu-lich-hen");
    await page.getByRole("button", { name: "Lưu lượt khách" }).click();
    await expect(
      page.getByRole("status").filter({ hasText: "Đã lưu lượt" }),
    ).toContainText("còn 11/12");

    await page.getByRole("button", { name: "Lịch hẹn", exact: true }).click();
    await expect(
      page.getByRole("button", {
        name: /09:00.*Trần Thị Lịch Hẹn, (Đang làm|Xong)/,
      }),
    ).toBeVisible();
  });

  test("regression (round 5 item 7): the real booking form books, and every refusal is said", async ({
    page,
  }) => {
    const r = await regular(page, "Phạm Thị Đặt Lịch");
    await page.goto(`/quay?view=lich&ngay=${spaDay(2)}`);
    await page.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    const sheet = page.getByRole("dialog", { name: "Đặt lịch hẹn" });

    // Pressing "Đặt lịch" too early is never a silent no-op.
    await sheet.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    await expect(sheet.getByRole("alert")).toHaveText(
      "Còn thiếu: khách, dịch vụ.",
    );

    await sheet
      .getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" })
      .pressSequentially(r.phone);
    await sheet.getByRole("option", { name: /Phạm Thị Đặt Lịch/ }).click();
    // What the EM did: pick the service in the list, nothing else.
    await sheet.getByLabel("Thêm dịch vụ").selectOption(r.service.id);
    // Exact, not rounded to the 15-minute grid (EM 2026-11-07, decision A).
    const minutes = String(r.service.duration_min ?? 60);
    await expect(sheet.getByLabel("Thời lượng")).toHaveValue(minutes);

    // The server's 422 (outside opening hours) is shown in Vietnamese.
    await sheet
      .getByLabel("Giờ hẹn")
      .selectOption({ value: String(20 * 60 + 45) });
    await sheet.getByLabel("Thời lượng").selectOption("60");
    await sheet.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    await expect(sheet.getByRole("alert")).toContainText(
      "Lịch hẹn phải nằm trọn trong giờ mở cửa",
    );

    // Fixed: it books and opens the appointment.
    await sheet.getByRole("button", { name: "Dùng lại theo dịch vụ" }).click();
    await sheet.getByLabel("Giờ hẹn").selectOption({ value: String(15 * 60) });
    await sheet.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    await expect(
      page.getByRole("dialog", { name: /15:00.*Phạm Thị Đặt Lịch/ }),
    ).toContainText("Chưa đến");
  });

  test("a family is booked in one go, each person checked in on their own; walk-ins are on the timeline", async ({
    page,
  }) => {
    const mother = await regular(page, "Đỗ Thị Mẹ");
    const child = await regular(page, "Đỗ Văn Con");

    // Book both through the form: the child is a companion with a customer record.
    await page.goto(`/quay?view=lich&ngay=${spaDay(3)}`);
    await page.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    const sheet = page.getByRole("dialog", { name: "Đặt lịch hẹn" });
    await sheet
      .getByRole("button", { name: "Người đi cùng", exact: true })
      .click();
    for (const [legend, who, name] of [
      ["Khách", mother, "Đỗ Thị Mẹ"],
      ["Người đi cùng 1", child, "Đỗ Văn Con"],
    ] as const) {
      const person = sheet.getByRole("group", {
        name: new RegExp(`^${legend}`),
      });
      if (legend !== "Khách")
        await person
          .getByRole("button", { name: "Khách có hồ sơ", exact: true })
          .click();
      await person
        .getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" })
        .pressSequentially(who.phone);
      await person.getByRole("option", { name: new RegExp(name) }).click();
      await person.getByLabel(/Thêm dịch vụ/).selectOption(who.service.id);
    }
    await sheet.getByLabel(/Giờ hẹn/).selectOption({ value: String(11 * 60) });
    await expectAccessible(page, "lich-dat-nhom");
    await sheet.getByRole("button", { name: "Đặt lịch 2 người" }).click();
    await expect(
      page.getByRole("dialog", { name: /11:00.*Đỗ Thị Mẹ/ }),
    ).toContainText("Cùng nhóm · 2 người");
    await page.keyboard.press("Escape");
    await expect(
      page.getByRole("button", {
        name: /Đỗ Văn Con, Chưa đến, Nhóm · 2 người/,
      }),
    ).toBeVisible();

    // Today: a group booked through the API (mother, child, a friend known by name only). A group
    // only says they come together: each person is checked in from their own sheet, like a single
    // booking (EM 2026-11-07: no "Khách đến cả nhóm" step) → one visit and one payment each.
    const group = await page.request.post("/api/v1/appointments/group", {
      headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
      data: {
        starts_at: `${spaDay()}T09:00:00+07:00`,
        people: [
          { customer_id: mother.customer, service_ids: [mother.service.id] },
          { customer_id: child.customer, service_ids: [child.service.id] },
          {
            guest: { full_name: "Bạn của chị Mẹ", phone: null },
            service_ids: [mother.service.id],
          },
        ],
      },
    });
    expect(group.status(), await group.text()).toBe(201);
    const checkIn = async (name: string, before?: (s: Locator) => Promise<void>) => {
      await page.goto("/quay?view=lich&kieu=ds");
      await page
        .getByRole("list", { name: /Lịch hẹn ngày/ })
        .getByRole("button", { name: new RegExp(name) })
        .click();
      await expect(
        page.getByRole("dialog", { name: new RegExp(`09:00.*${name}`) }),
      ).toBeVisible();
      // By role only: a walk-in profile renames the sheet to "Khách vãng lai".
      const sheet = page.getByRole("dialog");
      await expect(sheet).toContainText("Cùng nhóm · 3 người");
      await expect(sheet.getByText(/cả nhóm/)).toHaveCount(0);
      if (before) await before(sheet);
      // The technician is a plain update on this person's booking, saved when picked.
      await sheet.getByLabel("KTV làm").selectOption(mother.staff.id);
      await expect(sheet.getByText("Đã lưu KTV.")).toBeVisible();
      await sheet.getByRole("button", { name: "Khách đến · ghi lượt" }).click();
      await expect(page.getByText(/Đang ghi cho lịch hẹn 09:00/)).toBeVisible();
      await expect(page.getByRole("combobox", { name: "KTV" })).toHaveValue(
        mother.staff.id,
      );
      await page.getByRole("button", { name: "Lưu lượt khách" }).click();
      await expect(page.locator(".toast")).toContainText("Đã lưu lượt");
    };
    // The mother first; the others still wait ("Chưa đến"), nobody is blocked by her.
    await checkIn("Đỗ Thị Mẹ", async (sheet) => {
      await expectAccessible(page, "lich-nhom-tung-nguoi");
      const members = sheet.getByRole("list");
      await expect(members.getByRole("listitem")).toHaveCount(3);
    });
    // Same technician later on is fine: the mother's booking is done.
    await checkIn("Đỗ Văn Con");
    // The friend has no profile: one tap records them as an anonymous walk-in (ADR-0011).
    await checkIn("Bạn của chị Mẹ", async (sheet) => {
      await expect(sheet).toContainText("Chưa có SĐT");
      const profile = sheet.getByRole("group", { name: "Hồ sơ khách" });
      await expectAccessible(page, "lich-nguoi-di-cung-chua-ho-so");
      await profile
        .getByRole("button", { name: "Ghi là khách vãng lai" })
        .click();
      await expect(profile).toBeHidden();
      await expect(sheet.getByRole("link", { name: "Mở hồ sơ" })).toBeVisible();
    });

    // A walk-in recorded at the desk shows up on the timeline without a booking.
    const walkIn = await page.request.post("/api/v1/visits", {
      headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: child.customer,
        lines: [
          {
            service_id: child.service.id,
            source: "one_off",
            staff_id: child.staff.id,
          },
        ],
        payments: [{ method: "cash", amount: child.service.prices.all ?? 0 }],
      },
    });
    expect(walkIn.status()).toBe(201);
    await page.goto("/quay?view=lich&kieu=ds");
    const list = page.getByRole("list", { name: /Lịch hẹn ngày/ });
    await expect(list.getByText(/Vãng lai/).first()).toBeVisible();
    await expect(
      list.getByRole("button", { name: /Đỗ Thị Mẹ.*(Đang làm|Xong)/ }),
    ).toBeVisible();
  });

  test("a companion gets their own service and technician, and their own block (EM 2026-11-07)", async ({
    page,
  }) => {
    const host = await regular(page, "Lê Thị Chủ Lịch");
    const day = spaDay(4);
    const cal = (await (
      await page.request.get(`/api/v1/appointments?from=${day}`)
    ).json()) as { technicians: { id: string; full_name: string }[] };
    const tech = cal.technicians.at(-1) ?? host.staff;

    await page.goto(`/quay?view=lich&ngay=${day}`);
    await page.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    const sheet = page.getByRole("dialog", { name: "Đặt lịch hẹn" });
    await sheet
      .getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" })
      .pressSequentially(host.phone);
    await sheet.getByRole("option", { name: /Lê Thị Chủ Lịch/ }).click();
    await sheet
      .getByLabel("Thêm dịch vụ", { exact: true })
      .selectOption(host.service.id);
    await sheet.getByLabel("Giờ hẹn").selectOption({ value: String(10 * 60) });

    // The companion: a name only, a service, the technician who serves them, a later start.
    await sheet
      .getByRole("button", { name: "Người đi cùng", exact: true })
      .click();
    await sheet
      .getByLabel("Tên hoặc ghi chú · Người đi cùng 1")
      .fill("Chồng của chị Chủ Lịch");
    await sheet
      .getByLabel("Thêm dịch vụ · Người đi cùng 1")
      .selectOption(host.service.id);
    await sheet
      .getByLabel("KTV làm cho người đi cùng · Người đi cùng 1")
      .selectOption(tech.id);
    await sheet
      .getByLabel("Giờ bắt đầu · Người đi cùng 1")
      .selectOption({ value: String(14 * 60) });
    // A second companion stays a note.
    await sheet
      .getByRole("button", { name: "Người đi cùng", exact: true })
      .click();
    await sheet.getByLabel("Tên hoặc ghi chú · Người đi cùng 2").fill("Bé Bin");
    await expect(sheet.getByText(/1 người đi cùng \(ghi chú\)/)).toBeVisible();
    await expectAccessible(page, "lich-dat-nguoi-di-cung");
    await sheet.getByRole("button", { name: "Đặt lịch 2 người" }).click();

    const opened = page.getByRole("dialog", { name: /10:00.*Lê Thị Chủ Lịch/ });
    await expect(opened).toContainText("Cùng nhóm · 2 người");
    await expect(opened).toContainText("Bé Bin");
    await expect(opened).toContainText(tech.full_name);
    await page.keyboard.press("Escape");

    // Their block sits in the technician's column.
    await page.getByLabel("Xem theo KTV").selectOption(tech.id);
    const grid = page.getByRole("group", { name: /Lịch theo KTV/ });
    const block = grid.getByRole("button", {
      name: /^14:00.*Chồng của chị Chủ Lịch, Chưa đến/,
    });
    await expect(block).toBeVisible();
    await expect(grid.locator(".cal-head:not(.cal-corner)")).toHaveText(
      tech.full_name,
    );

    // Their sheet: no phone yet; one tap makes a profile from the booked name (no banner, no
    // form), and the technician stays on their booking.
    await block.click();
    const theirs = page.getByRole("dialog", {
      name: /14:00.*Chồng của chị Chủ Lịch/,
    });
    await expect(theirs).toContainText("Chưa có SĐT");
    await expect(theirs.getByLabel("KTV làm")).toHaveValue(tech.id);
    const profile = theirs.getByRole("group", { name: "Hồ sơ khách" });
    await expect(profile).toContainText("Chưa có hồ sơ khách.");
    await profile.getByRole("button", { name: "Tạo hồ sơ" }).click();
    await expect(profile).toBeHidden();
    await expect(theirs.getByRole("link", { name: "Mở hồ sơ" })).toBeVisible();
    await expect(theirs.getByLabel("KTV làm")).toHaveValue(tech.id);
  });

  test("no hard-coded capacity: the owner sets an optional max per slot", async ({
    page,
  }) => {
    await page.goto(`/quay?view=lich&ngay=${spaDay(4)}`);
    const grid = page.getByRole("group", { name: /Lịch theo KTV/ });
    await expect(grid).toBeVisible();
    // No "còn N chỗ" on every slot any more.
    await expect(grid.getByText(/^Kín$|^\d+$/)).toHaveCount(0);
    await page.getByRole("button", { name: /^Số khách tối đa/ }).click();
    const limit = page.getByRole("dialog", {
      name: "Số khách tối đa mỗi khung giờ",
    });
    await limit.getByLabel(/Số khách tối đa/).fill("3");
    await limit.getByRole("button", { name: "Lưu" }).click();
    await expect(
      page.getByRole("button", { name: "Số khách tối đa: 3" }),
    ).toBeVisible();
    await expect(grid.getByText("0/3").first()).toBeVisible();
    // Back to no limit (other specs share this spa).
    await page.getByRole("button", { name: "Số khách tối đa: 3" }).click();
    await limit.getByLabel(/Số khách tối đa/).fill("");
    await limit.getByRole("button", { name: "Lưu" }).click();
    await expect(
      page.getByRole("button", { name: "Số khách tối đa", exact: true }),
    ).toBeVisible();
  });

  test("regression (round 6): cards float over the grid at their real length; free slots stay bookable", async ({
    page,
  }) => {
    const r = await regular(page, "Bùi Thị Lớp Lịch");
    const services = (await (
      await page.request.get("/api/v1/services")
    ).json()) as Service[];
    const short = services.find(
      (s) => s.active && s.duration_min === 30 && s.prices.all != null,
    );
    // An add-on without a duration ("Mặt nạ thuốc bắc"): it used to add 60 minutes each.
    const addOn = services.find(
      (s) => s.active && s.duration_min === null && s.prices.all != null,
    );
    expect(
      short && addOn,
      "seeded menu: a 30-minute service and an add-on",
    ).toBeTruthy();
    if (!short || !addOn) return;
    const day = spaDay(5);
    const booked = await page.request.post("/api/v1/appointments", {
      headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: r.customer,
        service_ids: [short.id, addOn.id],
        staff_id: r.staff.id,
        starts_at: `${day}T10:00:00+07:00`,
      },
    });
    expect(booked.status()).toBe(201);
    expect(
      ((await booked.json()) as { duration_min: number }).duration_min,
    ).toBe(30);

    // A walk-in recorded yesterday 14:00 (owner backdate) with the same two services.
    const yesterday = spaDay(-1);
    const walkIn = await page.request.post("/api/v1/visits", {
      headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: r.customer,
        check_in_at: `${yesterday}T14:00:00+07:00`,
        lines: [short, addOn].map((s) => ({
          service_id: s.id,
          source: "one_off",
          staff_id: r.staff.id,
        })),
        payments: [
          {
            method: "cash",
            amount: (short.prices.all ?? 0) + (addOn.prices.all ?? 0),
          },
        ],
      },
    });
    expect(walkIn.status(), await walkIn.text()).toBe(201);

    const grid = page.getByRole("group", { name: /Lịch theo KTV/ });
    const slotHeight = async () =>
      (await grid.locator(".cal-cell").first().boundingBox())?.height ?? 0;

    // Yesterday's walk-in: 30 minutes = 2 slots tall (±2 px per slot).
    await page.goto(`/quay?view=lich&ngay=${yesterday}`);
    const visit = grid.getByRole("button", {
      name: /^14:00–14:30 Bùi Thị Lớp Lịch, Xong, Vãng lai/,
    });
    await expect(visit).toBeVisible();
    const slot = await slotHeight();
    expect(slot).toBeGreaterThan(10);
    const visitBox = await visit.boundingBox();
    expect(Math.abs((visitBox?.height ?? 0) - 2 * slot)).toBeLessThanOrEqual(4);

    await page.goto(`/quay?view=lich&ngay=${day}`);
    const card = grid.getByRole("button", {
      name: /^10:00–10:30 Bùi Thị Lớp Lịch, Chưa đến/,
    });
    await expect(card).toBeVisible();
    const box = await card.boundingBox();
    if (!box) throw new Error("no card box");
    expect(Math.abs(box.height - 2 * slot)).toBeLessThanOrEqual(4);

    // The slot cells under the card are still there (they draw the lines) and the free time
    // right below it in the same column is a slot, not the card.
    const inColumn = async (name: RegExp) => {
      for (const cell of await grid.getByRole("button", { name }).all()) {
        const b = await cell.boundingBox();
        if (
          b &&
          b.x <= box.x + box.width / 2 &&
          box.x + box.width / 2 <= b.x + b.width
        )
          return { cell, b };
      }
      throw new Error(`no ${String(name)} cell in the card's column`);
    };
    const under = await inColumn(/^Đặt lịch 10:15/);
    // The card stays inside its own column, with a small inset on both sides.
    expect(box.x).toBeGreaterThan(under.b.x);
    expect(box.x + box.width).toBeLessThan(under.b.x + under.b.width);
    expect(box.width).toBeGreaterThan(under.b.width - 12);
    expect(under.b.y).toBeGreaterThanOrEqual(box.y - 1);
    expect(under.b.y + under.b.height).toBeLessThanOrEqual(
      box.y + box.height + 2,
    );
    const hits = await page.evaluate(
      ({ x, inside, below }) => {
        const at = (y: number) => document.elementFromPoint(x, y);
        return {
          inside: Boolean(at(inside)?.closest(".cal-event")),
          below: at(below)?.classList.contains("cal-cell") ?? false,
        };
      },
      {
        x: box.x + box.width / 2,
        inside: box.y + box.height / 2,
        below: box.y + box.height + slot / 2,
      },
    );
    expect(hits).toEqual({ inside: true, below: true });

    // Free space in the same column books at its time; the card opens its own sheet.
    const after = await inColumn(/^Đặt lịch 10:30/);
    await after.cell.click();
    const sheet = page.getByRole("dialog", { name: "Đặt lịch hẹn" });
    await expect(sheet.getByLabel("Giờ hẹn")).toHaveValue(String(10 * 60 + 30));
    await page.keyboard.press("Escape");
    await expect(sheet).toBeHidden();
    await card.click();
    await expect(
      page.getByRole("dialog", { name: /10:00.*Bùi Thị Lớp Lịch/ }),
    ).toBeVisible();
    await page.keyboard.press("Escape");

    // Alignment (EM round 6): equal columns, first header whole, toolbar controls one height.
    await page.setViewportSize({ width: 1440, height: 900 });
    const heads = await grid.locator(".cal-head:not(.cal-corner)").all();
    const widths = await Promise.all(
      heads.map(async (h) => (await h.boundingBox())?.width ?? 0),
    );
    for (const w of widths)
      expect(Math.abs(w - (widths[0] ?? 0))).toBeLessThanOrEqual(1);
    const corner = await grid.locator(".cal-corner").boundingBox();
    const first = await heads[0]?.boundingBox();
    expect(first && corner && first.x >= corner.x + corner.width - 1).toBe(
      true,
    );
    const name = heads[0]?.locator("span");
    expect(
      await name?.evaluate((el) => {
        const own = el.getBoundingClientRect();
        const cell = (el.parentElement as HTMLElement).getBoundingClientRect();
        return own.left >= cell.left - 0.5 && own.right <= cell.right + 0.5;
      }),
      "the first column's name is not cut at its left edge",
    ).toBe(true);
    const controls = await page
      .locator(".cal-toolbar")
      .locator(
        ":scope > button, :scope > .segmented, :scope > label select, .date-field",
      )
      .all();
    const boxes = (
      await Promise.all(controls.map((c) => c.boundingBox()))
    ).filter((b): b is NonNullable<typeof b> => b !== null);
    expect(boxes.length).toBeGreaterThanOrEqual(4);
    for (const b of boxes) {
      expect(
        Math.abs(b.height - 44),
        `toolbar control ${JSON.stringify(b)}`,
      ).toBeLessThanOrEqual(1);
      // Controls on the same line share their top and bottom.
      for (const o of boxes)
        if (o.y < b.y + b.height && b.y < o.y + o.height)
          expect(Math.abs(o.y - b.y)).toBeLessThanOrEqual(1);
    }
    // "Đặt lịch" is the toolbar's primary action, above the grid (no bottom bar on desktop).
    const bookButton = page.getByRole("button", {
      name: "Đặt lịch",
      exact: true,
    });
    await expect(bookButton).toHaveCount(1);
    expect(
      ((await bookButton.boundingBox())?.y ?? 0) <
        ((await grid.boundingBox())?.y ?? 0),
    ).toBe(true);
    await expectAccessible(page, "lich-lop-su-kien");
  });

  test("view one technician's day and week (EM round 6)", async ({ page }) => {
    const r = await regular(page, "Mai Thị Theo Người");
    const day = spaDay(6);
    const booked = await page.request.post("/api/v1/appointments", {
      headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: r.customer,
        service_ids: [r.service.id],
        staff_id: null,
        starts_at: `${day}T11:00:00+07:00`,
      },
    });
    expect(booked.status()).toBe(201);
    await page.goto(`/quay?view=lich&ngay=${day}`);
    const grid = page.getByRole("group", { name: /Lịch theo KTV/ });
    const picker = page.getByLabel("Xem theo KTV");
    await expect(picker.locator("option").first()).toHaveText("Tất cả KTV");
    const options = await picker.locator("option").allTextContents();
    expect(options[0]).toBe("Tất cả KTV");
    expect(options.at(-1)).toBe("Chưa gán KTV");
    const all = await grid.locator(".cal-head:not(.cal-corner)").count();
    expect(all).toBe(options.length - 1);

    // One technician: one full-width column, kept in the URL.
    const person = options[1] ?? "";
    await picker.selectOption({ label: person });
    await expect(page).toHaveURL(/ktv=/);
    const heads = grid.locator(".cal-head:not(.cal-corner)");
    await expect(heads).toHaveCount(1);
    await expect(heads).toHaveText(person);
    await expect(grid.getByText("Mai Thị Theo Người")).toHaveCount(0);
    await page.reload();
    await expect(picker.locator("option:checked")).toHaveText(person);

    // Unassigned only: the booking without a technician is there.
    await picker.selectOption("none");
    await expect(heads).toHaveText("Chưa gán KTV");
    await expect(
      grid.getByRole("button", { name: /11:00.*Mai Thị Theo Người/ }),
    ).toBeVisible();

    // The week of that filter: seven days, only its bookings.
    await page.getByRole("button", { name: "Tuần", exact: true }).click();
    await expect(page.locator(".cal-day")).toHaveCount(7);
    await expect(page.getByText("Mai Thị Theo Người")).toBeVisible();
    await picker.selectOption({ label: person });
    await expect(page.getByText("Mai Thị Theo Người")).toHaveCount(0);
    await picker.selectOption("");
    await expect(page).not.toHaveURL(/ktv=/);
  });

  test("EM 2026-11-07: exact length, one booking per person at a time, real buttons, no guessed KTV", async ({
    page,
  }) => {
    const r = await regular(page, "Nguyễn Thị Lan Anh");
    const services = (await (
      await page.request.get("/api/v1/services")
    ).json()) as Service[];
    const seventy = services.find((s) => s.active && s.duration_min === 70);
    expect(seventy, "seeded menu: a 70-minute service").toBeDefined();
    if (!seventy) return;
    const day = spaDay(7);
    const book = (starts: string, staff: string | null) =>
      page.request.post("/api/v1/appointments", {
        headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
        data: {
          customer_id: r.customer,
          service_ids: [seventy.id],
          staff_id: staff,
          starts_at: starts,
        },
      });
    const booked = await book(`${day}T09:00:00+07:00`, r.staff.id);
    expect(booked.status()).toBe(201);
    expect(
      ((await booked.json()) as { duration_min: number }).duration_min,
    ).toBe(70);

    // Decision A: 09:00–10:10 on the grid, 70/15 slots tall; 10:15 right below stays free.
    await page.goto(`/quay?view=lich&ngay=${day}`);
    const grid = page.getByRole("group", { name: /Lịch theo KTV/ });
    const card = grid.getByRole("button", {
      name: /^09:00–10:10 Nguyễn Thị Lan Anh, Chưa đến/,
    });
    await expect(card).toBeVisible();
    const slot =
      (await grid.locator(".cal-cell").first().boundingBox())?.height ?? 0;
    const box = await card.boundingBox();
    expect(slot).toBeGreaterThan(10);
    expect(Math.abs((box?.height ?? 0) - (70 / 15) * slot)).toBeLessThanOrEqual(
      4,
    );

    // The same customer again at 09:30: the server refuses, and the form says so by its button
    // before sending anything (EM: "Lan Anh 09:00 ×2").
    const twice = await book(`${day}T09:30:00+07:00`, null);
    expect(twice.status()).toBe(409);
    expect(((await twice.json()) as { code: string }).code).toBe(
      "appointment.customer_busy",
    );
    await page.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    const sheet = page.getByRole("dialog", { name: "Đặt lịch hẹn" });
    await sheet
      .getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" })
      .pressSequentially(r.phone);
    await sheet.getByRole("option", { name: /Nguyễn Thị Lan Anh/ }).click();
    await sheet.getByLabel("Thêm dịch vụ").selectOption(seventy.id);
    await sheet
      .getByLabel("Giờ hẹn")
      .selectOption({ value: String(9 * 60 + 30) });
    const alert = sheet.getByRole("alert");
    await expect(alert).toContainText(
      "Nguyễn Thị Lan Anh đã có lịch 09:00–10:10",
    );
    // Pinned by the button: on screen even though the form is long.
    await expect(alert).toBeInViewport();
    await sheet.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    await expect(sheet).toBeVisible();
    await sheet.getByRole("button", { name: "Thôi" }).click();

    // Today, no technician booked: "KTV làm" starts empty and asks for a choice.
    const today = await book(`${spaDay()}T09:00:00+07:00`, null);
    expect(today.status()).toBe(201);
    await page.goto("/quay?view=lich");
    await page
      .getByRole("button", { name: /09:00.*Nguyễn Thị Lan Anh, Chưa đến/ })
      .click();
    const panel = page.getByRole("dialog", {
      name: /09:00.*Nguyễn Thị Lan Anh/,
    });
    await expect(panel.getByLabel("KTV làm")).toHaveValue("");
    await panel.getByRole("button", { name: "Chỉ báo đã đến" }).click();
    await expect(panel).toContainText(
      "Chọn KTV làm rồi bấm lại “Chỉ báo đã đến”.",
    );
    await expect(panel).toContainText("Chưa đến");

    // "Hủy lịch" is a real button of the control scale, at 1280 and at 390 px.
    const cancel = panel.getByRole("button", { name: "Hủy lịch" });
    const edit = panel.getByRole("button", { name: "Đổi lịch" });
    for (const width of [1280, 390]) {
      await page.setViewportSize({ width, height: 844 });
      await cancel.scrollIntoViewIfNeeded();
      const [c, e] = await Promise.all([
        cancel.boundingBox(),
        edit.boundingBox(),
      ]);
      expect(
        c?.height ?? 0,
        `Hủy lịch at ${String(width)}`,
      ).toBeGreaterThanOrEqual(43);
      expect(Math.abs((c?.height ?? 0) - (e?.height ?? 0))).toBeLessThanOrEqual(
        1,
      );
      expect(
        await cancel.evaluate((el) => getComputedStyle(el).borderTopStyle),
      ).toBe("solid");
    }
    await expectAccessible(page, "lich-hen-thao-tac");
    await page.setViewportSize({ width: 1280, height: 800 });
    await cancel.click();
    await panel.getByLabel("Lý do hủy lịch (bắt buộc)").fill("Khách đặt trùng");
    await panel.getByRole("button", { name: "Hủy lịch" }).click();
    await expect(panel).toContainText("Đã hủy");
  });

  test("phones get the list, the week overview works", async ({ page }) => {
    const r = await regular(page, "Lê Thị Điện Thoại");
    const booked = await page.request.post("/api/v1/appointments", {
      headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: r.customer,
        service_ids: [r.service.id],
        starts_at: `${spaDay(1)}T14:00:00+07:00`,
      },
    });
    expect(booked.status()).toBe(201);
    await page.setViewportSize({ width: 360, height: 780 });
    await page.goto(`/quay?view=lich&ngay=${spaDay(1)}`);
    const list = page.getByRole("list", { name: /Lịch hẹn ngày/ });
    await expect(list).toContainText("Lê Thị Điện Thoại");
    expect(
      await page.evaluate(
        () =>
          document.documentElement.scrollWidth <=
          document.documentElement.clientWidth,
      ),
      "no horizontal scroll on a phone",
    ).toBe(true);
    await expectAccessible(page, "lich-dien-thoai");
    await page.getByRole("button", { name: "Tuần" }).click();
    await expect(page.getByText("Lê Thị Điện Thoại")).toBeVisible();
  });
});

/**
 * Review screenshots (round 4): set R4_SHOTS_DIR to an absolute folder. Skipped otherwise.
 * 360/768/1280/1440 × light/dark of the record tab (with a cart and a discount) and the calendar.
 */
test("round-4 screenshots", async ({ page }) => {
  const dir = process.env["R4_SHOTS_DIR"];
  test.skip(!dir, "set R4_SHOTS_DIR to capture");
  test.setTimeout(5 * 60_000);
  const r = await regular(page, "Nguyễn Thị Ảnh Chụp");
  for (const [h, staff] of [
    ["09:00", r.staff.id],
    ["10:30", null],
    ["13:00", r.staff.id],
  ] as const) {
    await page.request.post("/api/v1/appointments", {
      headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
      data: {
        customer_id: r.customer,
        service_ids: [r.service.id],
        staff_id: staff,
        starts_at: `${spaDay(1)}T${h}:00+07:00`,
      },
    });
  }
  for (const scheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    for (const width of [360, 768, 1280, 1440]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/quay");
      const search = page.getByRole("combobox", {
        name: "Tìm khách theo SĐT hoặc tên",
      });
      await search.pressSequentially(r.phone);
      await page.getByRole("option", { name: /Nguyễn Thị Ảnh Chụp/ }).click();
      await page
        .getByRole("button", { name: /^Trừ 1 buổi/ })
        .first()
        .click();
      // Picking a service adds it (no separate "Thêm").
      await page.getByLabel("Thêm dịch vụ lẻ").selectOption(r.service.id);
      await page.getByRole("button", { name: "Thêm giảm giá" }).click();
      await page.getByLabel("Mức giảm (%)").fill("10");
      await page.getByLabel("Lý do (bắt buộc)").fill("Khách quen");
      await page.screenshot({
        path: `${dir}/quay-ghi-luot-${String(width)}-${scheme}.png`,
        fullPage: true,
      });
      for (const kieu of width < 640
        ? ["ngay", "tuan"]
        : ["ngay", "tuan", "ds"]) {
        await page.goto(
          `/quay?view=lich&ngay=${spaDay(1)}${kieu === "ngay" ? "" : `&kieu=${kieu}`}`,
        );
        await expect(
          page.getByRole("heading", { name: /Lịch hẹn/ }),
        ).toBeVisible();
        await page.waitForTimeout(300);
        await page.screenshot({
          path: `${dir}/quay-lich-${kieu}-${String(width)}-${scheme}.png`,
          fullPage: true,
        });
      }
    }
  }
});

test("companion screenshots (EM 2026-11-07)", async ({ page }) => {
  const dir = process.env["COMPANION_SHOTS_DIR"];
  test.skip(!dir, "set COMPANION_SHOTS_DIR to capture");
  test.setTimeout(5 * 60_000);
  const host = await regular(page, "Nguyễn Thị Hà Ảnh");
  // Today: the sheets show arrival, the technician and the primary action.
  const day = spaDay();
  const cal = (await (
    await page.request.get(`/api/v1/appointments?from=${day}`)
  ).json()) as { technicians: { id: string; full_name: string }[] };
  const tech = cal.technicians.at(-1) ?? host.staff;
  const booked = await page.request.post("/api/v1/appointments/group", {
    headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
    data: {
      starts_at: `${day}T10:00:00+07:00`,
      companions: ["Bé Na"],
      people: [
        {
          customer_id: host.customer,
          service_ids: [host.service.id],
          staff_id: null,
        },
        {
          guest: { full_name: "Chồng của chị Hà", phone: null },
          service_ids: [host.service.id],
          staff_id: tech.id,
          starts_at: `${day}T10:30:00+07:00`,
        },
      ],
    },
  });
  expect(booked.status(), await booked.text()).toBe(201);
  for (const scheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    for (const width of [390, 1280]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(`/quay?view=lich&ngay=${day}`);
      await expect(page.getByText("Chồng của chị Hà").first()).toBeVisible();
      await page.waitForTimeout(300);
      await page.screenshot({
        path: `${dir}/lich-${String(width)}-${scheme}.png`,
        fullPage: true,
      });
      await page
        .getByRole("button", { name: "Đặt lịch", exact: true })
        .first()
        .click();
      const sheet = page.getByRole("dialog", { name: "Đặt lịch hẹn" });
      await sheet
        .getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" })
        .pressSequentially(host.phone);
      await sheet.getByRole("option", { name: /Nguyễn Thị Hà Ảnh/ }).click();
      await sheet
        .getByLabel("Thêm dịch vụ", { exact: true })
        .selectOption(host.service.id);
      await sheet
        .getByRole("button", { name: "Người đi cùng", exact: true })
        .click();
      await sheet
        .getByLabel("Tên hoặc ghi chú · Người đi cùng 1")
        .fill("Chồng của chị Hà");
      await sheet
        .getByLabel("Thêm dịch vụ · Người đi cùng 1")
        .selectOption(host.service.id);
      await sheet
        .getByLabel("KTV làm cho người đi cùng · Người đi cùng 1")
        .selectOption(tech.id);
      await sheet
        .getByRole("button", { name: "Người đi cùng", exact: true })
        .click();
      await sheet
        .getByLabel("Tên hoặc ghi chú · Người đi cùng 2")
        .fill("Bé Na");
      await page.waitForTimeout(300);
      await sheet.screenshot({
        path: `${dir}/dat-lich-${String(width)}-${scheme}.png`,
      });
      await sheet
        .getByRole("button", { name: "Người đi cùng 1" })
        .first()
        .scrollIntoViewIfNeeded()
        .catch(() => undefined);
      await sheet
        .getByLabel("KTV làm cho người đi cùng · Người đi cùng 1")
        .scrollIntoViewIfNeeded();
      await sheet.screenshot({
        path: `${dir}/dat-lich-nguoi-di-cung-${String(width)}-${scheme}.png`,
      });
      await page.keyboard.press("Escape");
      const card = page
        .getByRole("button", { name: /Nguyễn Thị Hà Ảnh/ })
        .first();
      await card.click();
      const opened = page.getByRole("dialog", { name: /Nguyễn Thị Hà Ảnh/ });
      await expect(opened).toContainText("Cùng nhóm");
      await page.waitForTimeout(300);
      await opened.screenshot({
        path: `${dir}/sheet-${String(width)}-${scheme}.png`,
      });
      // The companion's own sheet (no profile yet), opened from the group row.
      await opened
        .getByRole("button", { name: /Chồng của chị Hà/ })
        .first()
        .click();
      const companion = page.getByRole("dialog", { name: /Chồng của chị Hà/ });
      await expect(companion).toContainText("Chưa có hồ sơ khách.");
      await page.waitForTimeout(300);
      await companion.screenshot({
        path: `${dir}/sheet-nguoi-di-cung-${String(width)}-${scheme}.png`,
      });
      await page.keyboard.press("Escape");
    }
  }
});
