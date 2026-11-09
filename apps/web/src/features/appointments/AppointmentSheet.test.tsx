import type { Appointment, Calendar } from "@vpyd/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  RouterProvider,
  createMemoryHistory,
  createRootRoute,
  createRouter,
} from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { activeStaffQuery } from "../sales/api";
import { AppointmentSheet } from "./AppointmentSheet";

const base: Appointment = {
  id: "a1",
  business_date: "2026-11-07",
  starts_at: "2026-11-07T10:15:00Z",
  ends_at: "2026-11-07T11:00:00Z",
  duration_min: 45,
  companions: [],
  created_at: "2026-11-07T01:00:00Z",
  customer_id: "c1",
  customer_name: "Hung Truong",
  customer_phone: "+84905111222",
  is_guest: false,
  group_id: "g1",
  paid_amount: 0,
  payment_state: "unpaid",
  phase: "waiting",
  services: [
    {
      service_id: "s1",
      service_name: "Thư Giãn",
      category_name: "Gội đầu",
      duration_min: 45,
    },
  ],
  staff_id: null,
  staff_name: null,
  status: "booked",
  version: 1,
};
const FRIEND: Appointment = {
  ...base,
  id: "a2",
  customer_id: null,
  customer_name: "bạn của Hung",
  customer_phone: null,
  is_guest: true,
};
const cal = (appointments: Appointment[]): Calendar => ({
  from: "2026-11-07",
  to: "2026-11-07",
  today: "2026-11-07",
  opens_at: "09:00",
  closes_at: "21:00",
  slot_minutes: 15,
  time_zone: "Asia/Ho_Chi_Minh",
  technicians: [
    { id: "st1", full_name: "Nguyễn Thị Lan" },
    { id: "st2", full_name: "Võ Thị Hằng" },
  ],
  max_per_slot: null,
  appointments,
  visits: [],
});

interface Sent {
  method: string;
  url: string;
  body: unknown;
}

function renderSheet(
  a: Appointment,
  reply: (s: Sent) => Response | undefined = () => undefined,
  extra: Partial<{
    onComplete: () => void;
    onOpenOther: (id: string) => void;
  }> = {},
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  client.setQueryData(activeStaffQuery.queryKey, [
    {
      id: "st1",
      full_name: "Nguyễn Thị Lan",
      position: "KTV",
      status: "active",
    },
    { id: "st2", full_name: "Võ Thị Hằng", position: "KTV", status: "active" },
  ]);
  const sent: Sent[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const s: Sent = {
        method: init?.method ?? "GET",
        url:
          typeof input === "string"
            ? input
            : input instanceof URL
              ? input.href
              : input.url,
        body: typeof init?.body === "string" ? JSON.parse(init.body) : null,
      };
      sent.push(s);
      return Promise.resolve(reply(s) ?? new Response("{}", { status: 500 }));
    }),
  );
  const onComplete = extra.onComplete ?? vi.fn();
  const onOpenOther = extra.onOpenOther ?? vi.fn();
  const router = createRouter({
    routeTree: createRootRoute({
      component: () => (
        <AppointmentSheet
          a={a}
          cal={cal([base, FRIEND])}
          locked={false}
          canCreateCustomer
          onClose={() => undefined}
          onEdit={() => undefined}
          onComplete={onComplete}
          onOpenOther={onOpenOther}
        />
      ),
    }),
    history: createMemoryHistory(),
  });
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { sent, onComplete, onOpenOther };
}

beforeEach(() => {
  // 2026-11-07 15:00 in the spa: the booking (17:15) is today, not started yet.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-07T08:00:00Z"));
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("Sheet lịch hẹn (EM 2026-11-07: mỗi người một lịch, không bước thừa)", () => {
  it("lists the group as rows that open each person's own sheet; no group check-in", async () => {
    const user = userEvent.setup();
    const { onOpenOther } = renderSheet(base);
    const group = await screen.findByRole("list");
    const rows = within(group).getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("Hung Truong");
    expect(rows[1]).toHaveTextContent("bạn của Hung");
    expect(rows[1]).toHaveTextContent("Chưa gán KTV");
    await user.click(within(rows[1] as HTMLElement).getByRole("button"));
    expect(onOpenOther).toHaveBeenCalledWith("a2");
    // No group check-in: the one arrival button is this person's own.
    expect(
      screen.getAllByRole("button", { name: /Khách đến/ }),
    ).toHaveLength(1);
  });

  it("saves the technician as a plain update when picked, and says so", async () => {
    const user = userEvent.setup();
    const { sent } = renderSheet(base, (s) =>
      s.method === "PATCH"
        ? Response.json({
            ...base,
            staff_id: "st2",
            staff_name: "Võ Thị Hằng",
            version: 2,
          })
        : undefined,
    );
    const pick = await screen.findByLabelText("KTV làm");
    expect(pick).toHaveValue("");
    expect(
      screen.getByText(/Chưa gán KTV: chọn khi biết ai làm/),
    ).toBeInTheDocument();
    await user.selectOptions(pick, "st2");
    expect(await screen.findByText("Đã lưu KTV.")).toBeInTheDocument();
    const patch = sent.find((s) => s.method === "PATCH");
    expect(patch?.url).toContain("/appointments/a1");
    expect(patch?.body).toEqual({ staff_id: "st2" });
  });

  it("puts a refused technician back and names the clash", async () => {
    const user = userEvent.setup();
    renderSheet(base, (s) =>
      s.method === "PATCH"
        ? Response.json(
            {
              code: "appointment.staff_busy",
              existing_id: "a2",
              message: "busy",
            },
            { status: 409 },
          )
        : undefined,
    );
    const pick = await screen.findByLabelText("KTV làm");
    await user.selectOptions(pick, "st1");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /đã có lịch 17:15–18:00 \(bạn của Hung\)\. Chọn KTV khác\./,
    );
    expect(pick).toHaveValue("");
  });

  it("asks for a technician before 'Chỉ báo đã đến', without sending", async () => {
    const user = userEvent.setup();
    const { sent } = renderSheet(base);
    await user.click(
      await screen.findByRole("button", { name: "Chỉ báo đã đến" }),
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Chọn KTV làm rồi bấm lại “Chỉ báo đã đến”.",
    );
    expect(screen.getByLabelText("KTV làm")).toHaveFocus();
    expect(sent.filter((s) => s.method === "POST")).toHaveLength(0);
  });

  it("records a person on their own: the primary action opens their cart", async () => {
    const user = userEvent.setup();
    const { onComplete } = renderSheet(base);
    await user.click(
      await screen.findByRole("button", { name: "Khách đến · ghi lượt" }),
    );
    expect(onComplete).toHaveBeenCalledOnce();
  });

  it("a companion without a profile: one tap creates it from the booked name, no banner", async () => {
    const user = userEvent.setup();
    const { sent, onComplete } = renderSheet(FRIEND, (s) => {
      if (s.method === "POST" && s.url.endsWith("/customers"))
        return Response.json(
          {
            id: "c2",
            full_name: "bạn của Hung",
            named: true,
            phone: null,
            version: 1,
          },
          { status: 201 },
        );
      if (s.method === "PATCH")
        return Response.json({
          ...FRIEND,
          customer_id: "c2",
          is_guest: false,
          version: 2,
        });
      return undefined;
    });
    expect(await screen.findByText("Chưa có SĐT")).toBeInTheDocument();
    const profile = screen.getByRole("group", { name: "Hồ sơ khách" });
    // The primary action waits and points at the choice instead of failing silently.
    await user.click(
      screen.getByRole("button", { name: "Khách đến · ghi lượt" }),
    );
    expect(onComplete).not.toHaveBeenCalled();
    expect(within(profile).getByRole("alert")).toHaveTextContent(
      "Chọn “Tạo hồ sơ” hoặc “Ghi là khách vãng lai” rồi bấm lại.",
    );
    expect(
      within(profile).getByRole("button", { name: "Tạo hồ sơ" }),
    ).toHaveFocus();
    await user.click(
      within(profile).getByRole("button", { name: "Tạo hồ sơ" }),
    );
    await vi.waitFor(() => {
      expect(sent.some((s) => s.method === "PATCH")).toBe(true);
    });
    expect(sent.find((s) => s.url.endsWith("/customers"))?.body).toEqual({
      full_name: "bạn của Hung",
      phone: null,
    });
    expect(sent.find((s) => s.method === "PATCH")?.body).toEqual({
      customer_id: "c2",
    });
  });

  it("or records them as an anonymous walk-in (ADR-0011) in one tap", async () => {
    const user = userEvent.setup();
    const { sent } = renderSheet(FRIEND, (s) => {
      if (s.method === "POST" && s.url.endsWith("/customers"))
        return Response.json(
          {
            id: "w1",
            full_name: "Khách vãng lai",
            named: false,
            phone: null,
            version: 1,
          },
          { status: 201 },
        );
      if (s.method === "PATCH")
        return Response.json({
          ...FRIEND,
          customer_id: "w1",
          is_guest: false,
          version: 2,
        });
      return undefined;
    });
    await user.click(
      await screen.findByRole("button", { name: "Ghi là khách vãng lai" }),
    );
    await vi.waitFor(() => {
      expect(sent.some((s) => s.method === "PATCH")).toBe(true);
    });
    expect(sent.find((s) => s.url.endsWith("/customers"))?.body).toEqual({
      full_name: null,
      phone: null,
    });
  });

  it("a family sharing one phone: the companion's profile goes without it", async () => {
    const user = userEvent.setup();
    const sharing = { ...FRIEND, customer_phone: "+84905111222" };
    const { sent } = renderSheet(sharing, (s) => {
      if (s.method === "POST" && s.url.endsWith("/customers")) {
        const body = s.body as { phone: string | null };
        return body.phone
          ? Response.json(
              {
                code: "customer.phone_taken",
                existing_id: "c1",
                message: "taken",
              },
              { status: 409 },
            )
          : Response.json(
              {
                id: "c3",
                full_name: "bạn của Hung",
                named: true,
                phone: null,
                version: 1,
              },
              { status: 201 },
            );
      }
      if (s.method === "PATCH")
        return Response.json({
          ...sharing,
          customer_id: "c3",
          is_guest: false,
          version: 2,
        });
      return undefined;
    });
    await user.click(await screen.findByRole("button", { name: "Tạo hồ sơ" }));
    await vi.waitFor(() => {
      expect(sent.some((s) => s.method === "PATCH")).toBe(true);
    });
    expect(sent.find((s) => s.method === "PATCH")?.body).toEqual({
      customer_id: "c3",
    });
  });
  it("undoes a mistaken arrival after one confirm (EM 2026-11-07 item 5a)", async () => {
    const user = userEvent.setup();
    const arrived: Appointment = {
      ...base,
      status: "arrived",
      staff_id: "st1",
      staff_name: "Nguyễn Thị Lan",
      arrived_at: "2026-11-07T08:00:00Z",
      version: 3,
    };
    const { sent } = renderSheet(arrived, (s) =>
      s.method === "POST" && s.url.endsWith("/undo-arrival")
        ? Response.json({ ...arrived, status: "booked", arrived_at: null, version: 4 })
        : undefined,
    );
    expect(screen.queryByRole("button", { name: "Chỉ báo đã đến" })).not.toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "Hoàn tác đã đến" }));
    // Nothing is sent before the confirm.
    expect(sent.filter((x) => x.method === "POST")).toHaveLength(0);
    const confirm = screen.getByRole("group", { name: /Đưa lịch về Chưa đến/ });
    await user.click(within(confirm).getByRole("button", { name: "Hoàn tác đã đến" }));
    await vi.waitFor(() => {
      expect(sent.some((x) => x.method === "POST" && x.url.endsWith("/appointments/a1/undo-arrival"))).toBe(true);
    });
  });

  it("offers no undo for a booking that has not arrived", async () => {
    renderSheet(base);
    expect(await screen.findByRole("button", { name: "Chỉ báo đã đến" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Hoàn tác đã đến" })).not.toBeInTheDocument();
  });

  it("shows “Không đến” once the time passes, with the sheet still open (ADR-0019 D20)", async () => {
    // The booking starts 200 ms after "now" (15:00 in the spa).
    renderSheet({ ...base, starts_at: "2026-11-07T08:00:00.200Z" });
    expect(await screen.findByRole("button", { name: /Hủy lịch/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Không đến" })).not.toBeInTheDocument();
    vi.setSystemTime(new Date("2026-11-07T08:00:01Z"));
    expect(await screen.findByRole("button", { name: "Không đến" })).toBeInTheDocument();
  });
});
