import type { Calendar, Service, ServiceCategory } from "@vpyd/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../lib/api";
import { categoriesQuery, servicesQuery } from "../catalog/api";
import {
  BookingSheet,
  blank,
  bookingErrorText,
  missingParts,
} from "./BookingSheet";

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

function renderSheet(cal: Calendar = CAL) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  client.setQueryData(servicesQuery(false).queryKey, [GOI, DAI]);
  client.setQueryData(categoriesQuery.queryKey, [CATEGORY]);
  const fetchMock = vi.fn(() =>
    Promise.resolve(new Response("{}", { status: 500 })),
  );
  vi.stubGlobal("fetch", fetchMock);
  render(
    <QueryClientProvider client={client}>
      <BookingSheet
        cal={cal}
        draft={{ mode: "new", day: "2026-11-07", start: 600, staffId: "" }}
        onClose={() => undefined}
        onSaved={() => undefined}
      />
    </QueryClientProvider>,
  );
  return { fetchMock };
}

afterEach(() => {
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

  it("sends companions as notes (round 6 Q-U5): trimmed, blanks dropped, at most 5", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet();
    await user.click(
      within(screen.getByRole("group", { name: "Loại khách Khách" })).getByRole(
        "button",
        { name: "Khách mới" },
      ),
    );
    await user.type(screen.getByLabelText("Tên khách"), "Chị Hà");
    await user.type(screen.getByLabelText("Số điện thoại"), "0905111222");
    await user.selectOptions(screen.getByLabelText("Thêm dịch vụ"), "s1");
    const add = () =>
      user.click(
        screen.getByRole("button", { name: /Người đi cùng \(ghi chú\)/ }),
      );
    await add();
    await user.type(
      screen.getByLabelText("Người đi cùng 1"),
      "  Chồng của chị Hà ",
    );
    await add();
    for (let i = 0; i < 3; i += 1) await add();
    // Five notes at most: the add button is gone.
    expect(
      screen.queryByRole("button", { name: /Người đi cùng \(ghi chú\)/ }),
    ).not.toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Bỏ người đi cùng 5" }),
    );
    expect(screen.getByText(/1 người đi cùng/)).toBeInTheDocument();

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

  it("sends no companions field when none is noted", async () => {
    const user = userEvent.setup();
    const { fetchMock } = renderSheet();
    await user.click(
      within(screen.getByRole("group", { name: "Loại khách Khách" })).getByRole(
        "button",
        { name: "Khách mới" },
      ),
    );
    await user.type(screen.getByLabelText("Tên khách"), "Chị Hà");
    await user.type(screen.getByLabelText("Số điện thoại"), "0905111222");
    await user.selectOptions(screen.getByLabelText("Thêm dịch vụ"), "s1");
    await user.click(
      screen.getByRole("button", { name: /Người đi cùng \(ghi chú\)/ }),
    );
    await user.click(screen.getByRole("button", { name: "Đặt lịch" }));
    const init = (
      fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    )[1];
    expect(JSON.parse(init.body as string)).not.toHaveProperty("companions");
  });

  it("books a family: + Thêm người, one form per person", async () => {
    const user = userEvent.setup();
    renderSheet();
    await user.click(screen.getByRole("button", { name: /Thêm người/ }));
    expect(screen.getByRole("group", { name: /^Người 1/ })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: /^Người 2/ })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Đặt lịch 2 người" }),
    ).toBeInTheDocument();
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

describe("booking messages", () => {
  it("lists what is missing per person of a group", () => {
    const people = [
      {
        ...blank(0),
        serviceIds: ["s1"],
        customer: { id: "c1", full_name: "Hà", phone: "0905", gender: null },
      },
      { ...blank(1), who: "guest" as const, guestName: "Bé Na" },
    ];
    expect(missingParts(people, false)).toEqual([
      "SĐT khách mới của Người 2 (Bé Na)",
      "dịch vụ của Người 2 (Bé Na)",
    ]);
  });

  it("explains every server refusal in Vietnamese, naming the person", () => {
    const people = [
      blank(0),
      { ...blank(1), who: "guest" as const, guestName: "Bé Na" },
    ];
    const phone = new ApiError(422, "validation_failed", [
      { field: "people[1].guest.phone", code: "invalid" },
    ]);
    expect(bookingErrorText(phone, CAL, people)).toBe(
      "Người 2 (Bé Na): SĐT — giá trị không hợp lệ.",
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
