import type { Calendar, Service, ServiceCategory } from "@vpyd/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../lib/api";
import { categoriesQuery, servicesQuery } from "../catalog/api";
import {
  BookingSheet,
  type Draft,
  blank,
  bookingErrorText,
  companion,
  missingParts,
  noteCompanions,
} from "./BookingSheet";

/** The customer who books: a newcomer "Chị Hà" with a phone, one Gội đầu (30 phút). */
async function fillGuest(user: ReturnType<typeof userEvent.setup>) {
  // The search's "Khách mới" (the only way to a newcomer, F7).
  await user.click(screen.getByRole("button", { name: "Khách mới" }));
  await user.type(screen.getByLabelText("Tên khách"), "Chị Hà");
  await user.type(screen.getByLabelText("Số điện thoại"), "0905111222");
  await user.selectOptions(screen.getByLabelText("Thêm dịch vụ"), "s1");
}

const service = (
  id: string,
  name: string,
  duration: number | null,
): Service => ({
  id,
  category_id: "cat1",
  name,
  description: null,
  duration_min: duration,
  recommended: false,
  active: true,
  version: 1,
  prices: { all: 100_000 },
});
const GOI = service("s1", "Gội đầu", 30);
const DAI = service("s2", "Đại Thông Lạc", 90);
const AN_KIEN = service("s3", "An Kiên Lạc", 70);
const CATEGORY: ServiceCategory = {
  id: "cat1",
  name: "Dưỡng sinh",
  sort_order: 0,
};

const CAL: Calendar = {
  from: "2026-11-07",
  to: "2026-11-07",
  today: "2026-11-06",
  opens_at: "09:00",
  closes_at: "21:00",
  slot_minutes: 15,
  time_zone: "Asia/Ho_Chi_Minh",
  technicians: [{ id: "st1", full_name: "KTV Lan" }],
  max_per_slot: null,
  appointments: [],
  visits: [],
};

function renderSheet(
  cal: Calendar = CAL,
  draft: Draft = { mode: "new", day: "2026-11-07", start: 600, staffId: "" },
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  client.setQueryData(servicesQuery(false).queryKey, [GOI, DAI, AN_KIEN]);
  client.setQueryData(categoriesQuery.queryKey, [CATEGORY]);
  const fetchMock = vi.fn(() =>
    Promise.resolve(new Response("{}", { status: 500 })),
  );
  vi.stubGlobal("fetch", fetchMock);
  render(
    <QueryClientProvider client={client}>
      <BookingSheet
        cal={cal}
        draft={draft}
        onClose={() => undefined}
        onSaved={() => undefined}
      />
    </QueryClientProvider>,
  );
  return { fetchMock };
}

beforeEach(() => {
  // 2026-11-06 10:00 in the spa: the sheets' days (07/11) are tomorrow whenever the suite runs.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-06T03:00:00Z"));
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("Đặt lịch hẹn (spec appointments.md §10.2, §10.5)", () => {
  it("adds a service as soon as it is picked and the length follows the services", async () => {
    const user = userEvent.setup();
    renderSheet();
    const length = screen.getByLabelText("Thời lượng");
    expect(length).toHaveValue("60");
    await user.selectOptions(screen.getByLabelText("Thêm dịch vụ"), "s1");
    expect(
      within(screen.getByRole("list", { name: "Dịch vụ đã chọn" })).getByText(
        /Gội đầu/,
      ),
    ).toBeInTheDocument();
    expect(length).toHaveValue("30");
    await user.selectOptions(screen.getByLabelText("Thêm dịch vụ"), "s2");
    expect(length).toHaveValue("120");
    expect(screen.getByText("Theo dịch vụ: 120 phút")).toBeInTheDocument();

    // Set by hand: kept when services change, until "Dùng lại theo dịch vụ".
    await user.selectOptions(length, "45");
    await user.click(screen.getByRole("button", { name: "Bỏ Gội đầu" }));
    expect(length).toHaveValue("45");
    expect(
      screen.getByText(/Đã chỉnh tay · theo dịch vụ: 90 phút/),
    ).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Dùng lại theo dịch vụ" }),
    );
    expect(length).toHaveValue("90");
  });

  it("never ignores a click on Đặt lịch: it names what is missing and sends nothing", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet();
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Còn thiếu: khách, dịch vụ.",
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("says so when “Dùng hồ sơ này” cannot open the existing record", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet();
    fetchMock.mockImplementation(((url: string) =>
      Promise.resolve(
        url === "/api/v1/appointments"
          ? Response.json(
              { code: "customer.phone_taken", existing_id: "c9" },
              { status: 409 },
            )
          : new Response(null, { status: 500 }),
      )) as unknown as () => Promise<Response>);
    await fillGuest(user);
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    await user.click(
      await screen.findByRole("button", { name: "Dùng hồ sơ này" }),
    );
    expect(
      await screen.findByText(/Chưa mở được hồ sơ:/),
    ).toBeInTheDocument();
  });

  it("sends a companion without a service as a note (round 6 Q-U5), trimmed", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet();
    await fillGuest(user);
    await user.click(screen.getByRole("button", { name: "Người đi cùng" }));
    await user.type(
      screen.getByLabelText("Tên hoặc ghi chú · Người đi cùng 1"),
      "  Chồng của chị Hà ",
    );
    // No service: no technician, length or start to fill.
    expect(
      screen.queryByLabelText("KTV làm cho người đi cùng · Người đi cùng 1"),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/1 người đi cùng \(ghi chú\)/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [
      string,
      RequestInit,
    ];
    expect(url).toBe("/api/v1/appointments");
    const body = JSON.parse(init.body as string) as Record<string, unknown>;
    expect(body["companions"]).toEqual(["Chồng của chị Hà"]);
    expect(body["guest"]).toEqual({ full_name: "Chị Hà", phone: "0905111222" });
  });

  it("books a companion with their own service, technician and start (EM 2026-11-07)", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet();
    await fillGuest(user);
    await user.click(screen.getByRole("button", { name: "Người đi cùng" }));
    await user.type(
      screen.getByLabelText("Tên hoặc ghi chú · Người đi cùng 1"),
      "Chồng của chị Hà",
    );
    await user.selectOptions(
      screen.getByLabelText("Thêm dịch vụ · Người đi cùng 1"),
      "s2",
    );
    await user.selectOptions(
      screen.getByLabelText("KTV làm cho người đi cùng · Người đi cùng 1"),
      "st1",
    );
    await user.selectOptions(
      screen.getByLabelText("Giờ bắt đầu · Người đi cùng 1"),
      "630",
    );
    expect(screen.getByText(/10:00–12:00/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Đặt lịch 2 người" }));
    const [url, init] = fetchMock.mock.calls[0] as unknown as [
      string,
      RequestInit,
    ];
    expect(url).toBe("/api/v1/appointments/group");
    const body = JSON.parse(init.body as string) as {
      people: Record<string, unknown>[];
    };
    expect(body).not.toHaveProperty("companions");
    expect(body.people).toHaveLength(2);
    expect(body.people[1]).toEqual({
      guest: { full_name: "Chồng của chị Hà", phone: null },
      service_ids: ["s2"],
      staff_id: "st1",
      duration_min: 90,
      starts_at: "2026-11-07T03:30:00.000Z",
    });
    // The person who books keeps the group's start.
    expect(body.people[0]).not.toHaveProperty("starts_at");
  });

  it("warns when one technician serves two people at overlapping times only", async () => {
    const user = userEvent.setup();
    renderSheet();
    await fillGuest(user);
    await user.selectOptions(
      screen.getByLabelText("KTV (không bắt buộc)"),
      "st1",
    );
    await user.click(screen.getByRole("button", { name: "Người đi cùng" }));
    await user.type(
      screen.getByLabelText("Tên hoặc ghi chú · Người đi cùng 1"),
      "Bé Na",
    );
    await user.selectOptions(
      screen.getByLabelText("Thêm dịch vụ · Người đi cùng 1"),
      "s1",
    );
    const staff = screen.getByLabelText(
      "KTV làm cho người đi cùng · Người đi cùng 1",
    );
    await user.selectOptions(staff, "st1");
    expect(screen.getByRole("status")).toHaveTextContent(
      /Người đi cùng 1 \(Bé Na\) đang cùng KTV/,
    );
    // Gội đầu of the customer ends at 10:30: the same technician can take the companion then.
    await user.selectOptions(
      screen.getByLabelText("Giờ bắt đầu · Người đi cùng 1"),
      "630",
    );
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("allows five companions and names a blank one as missing", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet();
    await fillGuest(user);
    for (let i = 0; i < 5; i += 1)
      await user.click(screen.getByRole("button", { name: "Người đi cùng" }));
    expect(
      screen.queryByRole("button", { name: /^\+? ?Người đi cùng$/ }),
    ).not.toBeInTheDocument();
    for (let i = 5; i > 1; i -= 1)
      await user.click(
        screen.getByRole("button", {
          name: `Bỏ người này (Người đi cùng ${String(i)})`,
        }),
      );
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Còn thiếu: tên hoặc ghi chú của Người đi cùng 1.",
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("edits the companions of a booking: notes by PATCH, a served one joins it", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet(CAL, {
      mode: "edit",
      appointment: {
        ...CAL_APPT,
        companions: ["Bé Na"],
        services: [
          {
            service_id: "s1",
            service_name: "Gội đầu",
            category_name: "Dưỡng sinh",
            duration_min: 30,
          },
        ],
      },
    });
    fetchMock.mockImplementation(() =>
      Promise.resolve(
        new Response(JSON.stringify(CAL_APPT), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }),
      ),
    );
    expect(
      screen.getByLabelText("Tên hoặc ghi chú · Người đi cùng 1"),
    ).toHaveValue("Bé Na");
    await user.click(screen.getByRole("button", { name: "Người đi cùng" }));
    await user.type(
      screen.getByLabelText("Tên hoặc ghi chú · Người đi cùng 2"),
      "Chồng của chị Hà",
    );
    await user.selectOptions(
      screen.getByLabelText("Thêm dịch vụ · Người đi cùng 2"),
      "s1",
    );
    await user.click(screen.getByRole("button", { name: "Lưu thay đổi" }));
    await vi.waitFor(() => {
      expect(fetchMock).toHaveBeenCalledTimes(2);
    });
    const calls = fetchMock.mock.calls as unknown as [string, RequestInit][];
    const [patchUrl, patchInit] = calls[0] ?? ["", {}];
    expect(patchUrl).toBe("/api/v1/appointments/a1");
    expect(patchInit.method).toBe("PATCH");
    expect(JSON.parse(patchInit.body as string)).toMatchObject({
      companions: ["Bé Na"],
    });
    const [postUrl, postInit] = calls[1] ?? ["", {}];
    expect(postUrl).toBe("/api/v1/appointments");
    expect(JSON.parse(postInit.body as string)).toMatchObject({
      group_with: "a1",
      guest: { full_name: "Chồng của chị Hà", phone: null },
      service_ids: ["s1"],
    });
  });

  it("a retry never books a companion twice: unchanged it is skipped, edited it is updated (audit 2026-11-09)", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet(CAL, {
      mode: "edit",
      appointment: {
        ...CAL_APPT,
        services: [{ service_id: "s1", service_name: "Gội đầu", category_name: "Dưỡng sinh", duration_min: 30 }],
      },
    });
    const json = (body: unknown, status = 200) =>
      Promise.resolve(new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } }));
    let posts = 0;
    fetchMock.mockImplementation(((url: string, init: RequestInit) => {
      if (init.method === "POST") {
        posts += 1;
        // The first companion joins; the second is refused once (its KTV is busy).
        if (posts === 1) return json({ ...CAL_APPT, id: "c-1", group_id: "g1" });
        if (posts === 2) return json({ status: 409, code: "appointment.staff_busy" }, 409);
        return json({ ...CAL_APPT, id: "c-2", group_id: "g1" });
      }
      return json({ ...CAL_APPT, id: url.split("/").pop() ?? "a1", version: 2 });
    }) as unknown as () => Promise<Response>);
    for (const [i, name] of ["Chồng của chị Hà", "Con của chị Hà"].entries()) {
      await user.click(screen.getByRole("button", { name: "Người đi cùng" }));
      await user.type(screen.getByLabelText(`Tên hoặc ghi chú · Người đi cùng ${String(i + 1)}`), name);
      await user.selectOptions(screen.getByLabelText(`Thêm dịch vụ · Người đi cùng ${String(i + 1)}`), "s1");
    }
    await user.click(screen.getByRole("button", { name: "Lưu thay đổi" }));
    await vi.waitFor(() => {
      expect(posts).toBe(2);
    });
    const calls = () => fetchMock.mock.calls as unknown as [string, RequestInit][];
    const keyOf = (init: RequestInit) => new Headers(init.headers).get("Idempotency-Key");
    const firstTry = calls().filter(([, init]) => init.method === "POST");
    const [firstKey, secondKey] = firstTry.map(([, init]) => keyOf(init));

    // Unchanged: only the refused one goes again, under its own key.
    await user.click(screen.getByRole("button", { name: "Lưu thay đổi" }));
    await vi.waitFor(() => {
      expect(posts).toBe(3);
    });
    const retried = calls().filter(([, init]) => init.method === "POST").slice(2);
    expect(retried.map(([, init]) => keyOf(init))).toEqual([secondKey]);
    expect(calls().filter(([url]) => url.endsWith("/appointments/c-1"))).toEqual([]);

    // Edited after it joined: the existing booking is updated, never booked again.
    await user.selectOptions(screen.getByLabelText("Thêm dịch vụ · Người đi cùng 1"), "s2");
    const before = calls().length;
    await user.click(screen.getByRole("button", { name: "Lưu thay đổi" }));
    await vi.waitFor(() => {
      expect(calls().some(([url, init]) => url.endsWith("/appointments/c-1") && init.method === "PATCH")).toBe(true);
    });
    const third = calls().slice(before);
    expect(third.filter(([, init]) => init.method === "POST" && keyOf(init) === firstKey)).toEqual([]);
    const patch = third.find(([url]) => url.endsWith("/appointments/c-1"));
    expect(JSON.parse(String(patch?.[1].body))).toMatchObject({ service_ids: ["s1", "s2"] });
  });

  it("shows n / max of the slot and warns, without blocking, when it is full", () => {
    const booked = (id: string) => ({
      ...CAL_APPT,
      id,
    });
    renderSheet({
      ...CAL,
      max_per_slot: 2,
      appointments: [booked("a1"), booked("a2")],
    });
    expect(
      screen.getByRole("option", { name: "10:00 · 2/2 khách" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      /Khung 10:00 đã có 2\/2 khách/,
    );
    expect(screen.getByRole("button", { name: "Đặt lịch" })).toBeEnabled();
  });

  it("books the services' exact length: 70 minutes is 10:00–11:10 (decision A)", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet();
    await fillGuest(user);
    await user.click(screen.getByRole("button", { name: "Bỏ Gội đầu" }));
    await user.selectOptions(screen.getByLabelText("Thêm dịch vụ"), "s3");
    const length = screen.getByLabelText("Thời lượng");
    expect(length).toHaveValue("70");
    expect(
      screen.getByRole("option", { name: "70 phút (theo dịch vụ)" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/10:00–11:10/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    const [, init] = fetchMock.mock.calls[0] as unknown as [
      string,
      RequestInit,
    ];
    expect(JSON.parse(init.body as string)).toMatchObject({ duration_min: 70 });
  });

  it("puts what is missing by the button and focuses its field (EM 2026-11-07)", async () => {
    const user = userEvent.setup();
    renderSheet();
    await fillGuest(user);
    await user.click(screen.getByRole("button", { name: "Bỏ Gội đầu" }));
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Còn thiếu: dịch vụ.");
    // Pinned in the footer next to the button, not at the bottom of the form.
    expect(alert.closest(".sheet-footer")).not.toBeNull();
    expect(screen.getByLabelText("Thêm dịch vụ")).toHaveFocus();
  });

  it("refuses a second booking of the same customer at the same time, before sending", async () => {
    const user = userEvent.setup();
    // Chị Hà, a newcomer (0905111222), already booked 10:00–11:00 with KTV Lan.
    const held = {
      ...CAL_APPT,
      customer_id: null,
      is_guest: true,
      customer_name: "Chị Hà",
      staff_id: "st1",
      staff_name: "KTV Lan",
    };
    const { fetchMock } = renderSheet({ ...CAL, appointments: [held] });
    await fillGuest(user);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Chị Hà đã có lịch 10:00–11:00 ngày 07/11/2026 (KTV Lan). Một người không có hai lịch trùng giờ",
    );
    expect(
      screen.getByText(/Đã có lịch 10:00–11:00 cùng lúc/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    expect(fetchMock).not.toHaveBeenCalled();
    // Right after it: free.
    await user.selectOptions(screen.getByLabelText("Giờ hẹn"), "660");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("moving a booking onto the same customer's other booking is flagged too", async () => {
    const user = userEvent.setup();
    const later = {
      ...CAL_APPT,
      id: "a2",
      starts_at: "2026-11-07T06:00:00Z",
      ends_at: "2026-11-07T07:00:00Z",
    };
    const { fetchMock } = renderSheet(
      { ...CAL, appointments: [CAL_APPT, later] },
      { mode: "edit", appointment: later },
    );
    // Untouched, a booking is not judged against the other one.
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("Giờ hẹn"), "630");
    expect(screen.getByRole("alert")).toHaveTextContent(
      /Nguyễn Thị Hà đã có lịch 10:00–11:00/,
    );
    await user.click(screen.getByRole("button", { name: "Lưu thay đổi" }));
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("explains the server's customer_busy with the booking it names", () => {
    const busy = new ApiError(409, "appointment.customer_busy", [], "a1");
    expect(
      bookingErrorText(busy, { ...CAL, appointments: [CAL_APPT] }, [blank(0)]),
    ).toMatch(/^Nguyễn Thị Hà đã có lịch 10:00–11:00 ngày 07\/11\/2026/);
  });

  it("never books a slot of today already over (F12: no more “Vẫn đặt lịch”)", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet(
      { ...CAL, from: "2026-11-06", to: "2026-11-06" },
      { mode: "new", day: "2026-11-06", start: 9 * 60, staffId: "" },
    );
    await fillGuest(user);
    expect(
      screen.getByText(/Giờ này đã qua \(bây giờ 10:00\)/),
    ).toBeInTheDocument();
    // Earlier slots cannot be picked; the slot under way (10:00) can.
    expect(screen.getByRole("option", { name: "09:30 · đã qua" })).toBeDisabled();
    expect(screen.getByRole("option", { name: "10:00" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(
      /Giờ hẹn 09:00 đã qua \(bây giờ 10:00\): chọn từ 10:00 trở đi/,
    );
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    expect(fetchMock).not.toHaveBeenCalled();
    await user.selectOptions(screen.getByLabelText("Giờ hẹn"), "600");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("never books into a past day (the owner records a back-dated visit instead)", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet(
      { ...CAL, from: "2026-11-05", to: "2026-11-05" },
      { mode: "new", day: "2026-11-05", start: 10 * 60, staffId: "" },
    );
    await fillGuest(user);
    expect(screen.getByRole("alert")).toHaveTextContent(
      /05\/11\/2026 đã qua: không đặt lịch cho ngày cũ/,
    );
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

const CAL_APPT: Calendar["appointments"][number] = {
  companions: [],
  id: "a1",
  status: "booked",
  phase: "waiting",
  group_id: null,
  business_date: "2026-11-07",
  starts_at: "2026-11-07T03:00:00Z",
  ends_at: "2026-11-07T04:00:00Z",
  duration_min: 60,
  customer_id: "c1",
  customer_name: "Nguyễn Thị Hà",
  customer_phone: "+84905111222",
  customer_gender: "female",
  is_guest: false,
  staff_id: null,
  staff_name: null,
  services: [],
  note: null,
  status_reason: null,
  arrived_at: null,
  closed_at: null,
  visit_id: null,
  payment_state: "unpaid",
  paid_amount: 0,
  version: 1,
  created_at: "2026-11-06T03:00:00Z",
};

describe("Đổi lịch một người trong nhóm (EM 2026-11-07 item 5c)", () => {
  const g = (id: string, name: string, extra: Partial<typeof CAL_APPT> = {}) => ({
    ...CAL_APPT,
    id,
    customer_id: `c-${id}`,
    customer_name: name,
    group_id: "g1",
    services: [{ service_id: "s1", service_name: "Gội đầu", category_name: "Dưỡng sinh", duration_min: 30 }],
    duration_min: 30,
    ends_at: "2026-11-07T03:30:00Z",
    ...extra,
  });
  const HA = g("a1", "Nguyễn Thị Hà");
  const MAI = g("a2", "Chị Mai", { starts_at: "2026-11-07T03:30:00Z", ends_at: "2026-11-07T04:30:00Z" });
  const BINH = g("a3", "Anh Bình", { status: "arrived", phase: "in_service" as typeof CAL_APPT.phase });
  const groupCal = { ...CAL, appointments: [HA, MAI, BINH] };
  const json = (body: unknown, status = 200) =>
    Promise.resolve(new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } }));

  it("asks only once the time moves; the whole group moves first, then this booking", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet(groupCal, { mode: "edit", appointment: HA });
    expect(screen.queryByRole("group", { name: "Người cùng nhóm" })).not.toBeInTheDocument();
    fetchMock.mockImplementation(((url: string) =>
      url.endsWith("/move-group")
        ? json({ group_id: "g1", moved: [{ ...HA, version: 2 }, MAI], not_moved: [BINH] })
        : json({ ...HA, version: 3 })) as unknown as () => Promise<Response>);
    await user.selectOptions(screen.getByLabelText("Giờ hẹn"), "720");
    const choice = screen.getByRole("group", { name: "Người cùng nhóm" });
    expect(within(choice).getByRole("radio", { name: /Đổi cả nhóm \(2 người/ })).toBeChecked();
    expect(choice).toHaveTextContent("Cùng dời: Chị Mai.");
    expect(choice).toHaveTextContent("Không đổi: Anh Bình (đã đến).");
    await user.click(screen.getByRole("button", { name: "Lưu thay đổi" }));
    await vi.waitFor(() => {
      expect(fetchMock).toHaveBeenCalledTimes(2);
    });
    const calls = fetchMock.mock.calls as unknown as [string, RequestInit][];
    const [moveUrl, moveInit] = calls[0] ?? ["", {}];
    expect(moveUrl).toBe("/api/v1/appointments/a1/move-group");
    expect(moveInit.method).toBe("POST");
    expect(new Headers(moveInit.headers).get("If-Match")).toBe('"1"');
    expect(JSON.parse(moveInit.body as string)).toEqual({ starts_at: "2026-11-07T05:00:00.000Z" });
    const [patchUrl, patchInit] = calls[1] ?? ["", {}];
    expect(patchUrl).toBe("/api/v1/appointments/a1");
    expect(new Headers(patchInit.headers).get("If-Match")).toBe('"2"');
  });

  it("after a partial failure, a retry with a new time moves the group and this booking again", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet(groupCal, { mode: "edit", appointment: HA });
    let patches = 0;
    let moves = 0;
    fetchMock.mockImplementation(((url: string) => {
      if (url.endsWith("/move-group")) {
        moves += 1;
        return json({ group_id: "g1", moved: [{ ...HA, version: moves * 2 }, MAI], not_moved: [BINH] });
      }
      patches += 1;
      return patches === 1
        ? json({ code: "appointment.staff_busy" }, 422)
        : json({ ...HA, version: 9 });
    }) as unknown as () => Promise<Response>);
    await user.selectOptions(screen.getByLabelText("Giờ hẹn"), "720");
    await user.click(screen.getByRole("button", { name: "Lưu thay đổi" }));
    await vi.waitFor(() => {
      expect(fetchMock).toHaveBeenCalledTimes(2);
    });
    // The group moved to 12:00 (version 2), this booking's PATCH was refused; the desk picks 12:30.
    await user.selectOptions(screen.getByLabelText("Giờ hẹn"), "750");
    await user.click(screen.getByRole("button", { name: "Lưu thay đổi" }));
    await vi.waitFor(() => {
      expect(fetchMock).toHaveBeenCalledTimes(4);
    });
    const calls = fetchMock.mock.calls as unknown as [string, RequestInit][];
    const [moveUrl, moveInit] = calls[2] ?? ["", {}];
    expect(moveUrl).toBe("/api/v1/appointments/a1/move-group");
    expect(new Headers(moveInit.headers).get("If-Match")).toBe('"2"');
    expect(JSON.parse(moveInit.body as string)).toEqual({ starts_at: "2026-11-07T05:30:00.000Z" });
    const [patchUrl, patchInit] = calls[3] ?? ["", {}];
    expect(patchUrl).toBe("/api/v1/appointments/a1");
    expect(new Headers(patchInit.headers).get("If-Match")).toBe('"4"');
    expect(JSON.parse(patchInit.body as string)).toMatchObject({ starts_at: "2026-11-07T05:30:00.000Z" });
  });

  it("'Chỉ người này' moves only this booking", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet(groupCal, { mode: "edit", appointment: HA });
    fetchMock.mockImplementation(() => json({ ...HA, version: 2 }));
    await user.selectOptions(screen.getByLabelText("Giờ hẹn"), "720");
    await user.click(screen.getByRole("radio", { name: "Chỉ người này" }));
    await user.click(screen.getByRole("button", { name: "Lưu thay đổi" }));
    await vi.waitFor(() => {
      expect(fetchMock).toHaveBeenCalledTimes(1);
    });
    const calls = fetchMock.mock.calls as unknown as [string, RequestInit][];
    expect(calls[0]?.[0]).toBe("/api/v1/appointments/a1");
    expect(calls[0]?.[1].method).toBe("PATCH");
  });

  it("names the member a refused group move clashed on; nobody moved", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet(groupCal, { mode: "edit", appointment: HA });
    fetchMock.mockImplementation(() =>
      json(
        { code: "validation_failed", errors: [{ field: "appointments.a2", code: "appointment.staff_busy" }] },
        422,
      ));
    await user.selectOptions(screen.getByLabelText("Giờ hẹn"), "720");
    await user.click(screen.getByRole("button", { name: "Lưu thay đổi" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Không đổi được cả nhóm. Chị Mai: KTV của người này đã có lịch khác vào giờ mới.",
    );
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});

describe("booking messages", () => {
  it("lists what is missing: the customer's services, a companion's name", () => {
    const people = [
      {
        ...blank(0),
        customer: { id: "c1", full_name: "Hà", named: true, phone: "0905", gender: null },
      },
      companion(1),
      { ...blank(2), serviceIds: ["s1"] },
    ];
    expect(missingParts(people, false)).toEqual([
      "dịch vụ",
      "tên hoặc ghi chú của Người đi cùng 1",
      "khách của Người đi cùng 2",
    ]);
  });

  it("keeps companions without a service as notes: trimmed, blanks dropped, at most 5", () => {
    const people = [
      blank(0),
      companion(1, " Chồng của chị Hà "),
      { ...companion(2, "Bé Na"), serviceIds: ["s1"] },
      ...["1", "2", "3", "4", "5"].map((n, i) => companion(3 + i, n)),
    ];
    expect(noteCompanions(people)).toEqual([
      "Chồng của chị Hà",
      "1",
      "2",
      "3",
      "4",
    ]);
  });

  it("explains every server refusal in Vietnamese, naming the person", () => {
    const people = [blank(0), companion(1, "Bé Na")];
    const phone = new ApiError(422, "validation_failed", [
      { field: "people[1].guest.phone", code: "invalid" },
    ]);
    expect(bookingErrorText(phone, CAL, people)).toBe(
      "Người đi cùng 1 (Bé Na): SĐT — giá trị không hợp lệ.",
    );
    const hours = new ApiError(422, "appointment.outside_opening_hours");
    expect(bookingErrorText(hours, CAL, people)).toMatch(
      /Giờ mở cửa 09:00–21:00/,
    );
    expect(
      bookingErrorText(new TypeError("fetch failed"), CAL, people),
    ).toMatch(/Không kết nối được máy chủ/);
  });
});
