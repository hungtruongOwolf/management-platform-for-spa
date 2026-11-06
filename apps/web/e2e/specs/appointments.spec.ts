import { expect, test, type Page } from "@playwright/test";
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
    await panel.getByLabel("KTV làm").selectOption(r.staff.id);
    await panel
      .getByRole("button", { name: "Chỉ báo đã đến (ghi lượt sau)" })
      .click();
    await expect(panel).toContainText("Đang làm");
    await panel.getByRole("button", { name: "Ghi lượt", exact: true }).click();
    await expect(page.getByText(/Hoàn tất lịch hẹn 09:00/)).toBeVisible();
    const cart = page.getByRole("list", { name: "Dịch vụ trong lượt" });
    await expect(cart).toContainText("Trừ liệu trình");
    await expect(page.getByRole("combobox", { name: "KTV" })).toHaveValue(
      r.staff.id,
    );
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
    const minutes = String(
      Math.max(15, Math.ceil((r.service.duration_min ?? 60) / 15) * 15),
    );
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

  test("a family is booked in one go and checked in together; walk-ins are on the timeline", async ({
    page,
  }) => {
    const mother = await regular(page, "Đỗ Thị Mẹ");
    const child = await regular(page, "Đỗ Văn Con");

    // Book both through the form: "+ Thêm người".
    await page.goto(`/quay?view=lich&ngay=${spaDay(3)}`);
    await page.getByRole("button", { name: "Đặt lịch", exact: true }).click();
    const sheet = page.getByRole("dialog", { name: "Đặt lịch hẹn" });
    await sheet.getByRole("button", { name: /Thêm người/ }).click();
    for (const [i, who, name] of [
      [1, mother, "Đỗ Thị Mẹ"],
      [2, child, "Đỗ Văn Con"],
    ] as const) {
      const person = sheet.getByRole("group", {
        name: new RegExp(`^Người ${String(i)}`),
      });
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
    ).toContainText("Cùng nhóm (2 người)");
    await page.keyboard.press("Escape");
    await expect(
      page.getByRole("button", {
        name: /Đỗ Văn Con, Chưa đến, Nhóm · 2 người/,
      }),
    ).toBeVisible();

    // Today: a group booked through the API, checked in together → two visits, two payments.
    const group = await page.request.post("/api/v1/appointments/group", {
      headers: { ...write, "Idempotency-Key": crypto.randomUUID() },
      data: {
        starts_at: `${spaDay()}T09:00:00+07:00`,
        people: [
          { customer_id: mother.customer, service_ids: [mother.service.id] },
          { customer_id: child.customer, service_ids: [child.service.id] },
        ],
      },
    });
    expect(group.status()).toBe(201);
    await page.goto("/quay?view=lich&kieu=ds");
    await page
      .getByRole("button", { name: /Đỗ Thị Mẹ/ })
      .first()
      .click();
    await page
      .getByRole("button", { name: "Khách đến cả nhóm · ghi 2 lượt" })
      .click();
    const checkIn = page.getByRole("dialog", { name: /Khách đến cả nhóm/ });
    for (const name of ["Đỗ Thị Mẹ", "Đỗ Văn Con"])
      await checkIn
        .getByRole("listitem", { name })
        .getByLabel("KTV làm")
        .selectOption(mother.staff.id);
    await expectAccessible(page, "lich-nhom-den");
    // Same technician for both is allowed here: visits are not bookings.
    await checkIn.getByRole("button", { name: "Ghi 2 lượt" }).click();
    await expect(checkIn.getByText("Đã ghi lượt")).toHaveCount(2);
    await checkIn.getByRole("button", { name: "Xong" }).click();

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
    await page.reload();
    const list = page.getByRole("list", { name: /Lịch hẹn ngày/ });
    await expect(list.getByText(/Vãng lai/)).toBeVisible();
    await expect(
      list.getByRole("button", { name: /Đỗ Thị Mẹ.*(Đang làm|Xong)/ }),
    ).toBeVisible();
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
      await page.getByLabel("Dịch vụ lẻ").selectOption(r.service.id);
      await page.getByRole("button", { name: "Thêm", exact: true }).click();
      await page.getByRole("button", { name: "+ Giảm giá dịch vụ lẻ" }).click();
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
