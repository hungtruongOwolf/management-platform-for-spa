import type { Appointment, Calendar, CalendarVisit } from "@vpyd/api-client";
import { describe, expect, it } from "vitest";
import {
  addedMinutes,
  autoMinutes,
  daysFrom,
  entriesOf,
  forStaff,
  hhmm,
  instantAt,
  mondayOf,
  paymentLabel,
  peakPeople,
  peopleAt,
  spaMinutes,
} from "./api";

function appt(over: Partial<Appointment>): Appointment {
  return {
    id: "a1",
    status: "booked",
    phase: "waiting",
    group_id: null,
    business_date: "2026-11-06",
    starts_at: "2026-11-06T03:00:00Z",
    ends_at: "2026-11-06T04:00:00Z",
    duration_min: 60,
    customer_id: "c1",
    customer_name: "Nguyễn Thị Hà",
    customer_phone: "+84905111222",
    customer_gender: "female",
    is_guest: false,
    staff_id: "st1",
    staff_name: "KTV Lan",
    services: [],
    note: null,
    status_reason: null,
    arrived_at: null,
    closed_at: null,
    visit_id: null,
    payment_state: "unpaid",
    paid_amount: 0,
    version: 1,
    created_at: "2026-11-05T03:00:00Z",
    ...over,
  };
}

function walkIn(over: Partial<CalendarVisit>): CalendarVisit {
  return {
    id: "v1",
    phase: "in_service",
    business_date: "2026-11-06",
    starts_at: "2026-11-06T04:00:00Z",
    ends_at: "2026-11-06T05:30:00Z",
    duration_min: 90,
    customer_id: "c2",
    customer_name: "Trần Văn Đức",
    customer_gender: "male",
    staff_id: "st2",
    staff_name: "KTV Thu",
    services: [],
    payment_state: "paid",
    paid_amount: 399_000,
    ...over,
  };
}

const cal = (
  appointments: Appointment[],
  visits: CalendarVisit[] = [],
  max: number | null = null,
): Calendar => ({
  from: "2026-11-06",
  to: "2026-11-06",
  today: "2026-11-05",
  opens_at: "09:00",
  closes_at: "21:00",
  slot_minutes: 15,
  time_zone: "Asia/Ho_Chi_Minh",
  technicians: [
    { id: "st1", full_name: "KTV Lan" },
    { id: "st2", full_name: "KTV Thu" },
  ],
  max_per_slot: max,
  appointments,
  visits,
});

describe("calendar helpers", () => {
  it("shows one technician, the unassigned ones, or everyone", () => {
    const all = cal(
      [
        appt({ id: "lan" }),
        appt({ id: "free", staff_id: null, staff_name: null }),
        appt({ id: "gone", staff_id: "left-the-spa" }),
      ],
      [walkIn({})],
    );
    const lan = forStaff(all, "st1");
    expect(lan.technicians.map((t) => t.id)).toEqual(["st1"]);
    expect(lan.appointments.map((a) => a.id)).toEqual(["lan"]);
    expect(lan.visits).toEqual([]);
    expect(forStaff(all, "st2").visits.map((v) => v.id)).toEqual(["v1"]);
    const none = forStaff(all, "none");
    expect(none.technicians).toEqual([]);
    expect(none.appointments.map((a) => a.id)).toEqual(["free", "gone"]);
    // Everyone, and an id that is no technician (stale link): the whole calendar.
    expect(forStaff(all, "")).toBe(all);
    expect(forStaff(all, "left-the-spa")).toBe(all);
  });

  it("converts instants to spa minutes and back", () => {
    expect(spaMinutes("2026-11-06T03:00:00Z")).toBe(600);
    expect(hhmm(615)).toBe("10:15");
    expect(instantAt("2026-11-06", 600)).toBe("2026-11-06T03:00:00.000Z");
  });

  it("counts people per slot from bookings and walk-ins; cancelled and voided free it", () => {
    const c = cal(
      [
        appt({}),
        appt({
          id: "a2",
          staff_id: null,
          starts_at: "2026-11-06T03:30:00Z",
          duration_min: 60,
        }),
        appt({
          id: "a3",
          status: "cancelled",
          phase: "cancelled",
          starts_at: "2026-11-06T03:00:00Z",
        }),
      ],
      [walkIn({}), walkIn({ id: "v2", phase: "voided" })],
    );
    expect(peopleAt(c, "2026-11-06", 600)).toBe(1); // 10:00: a1
    expect(peopleAt(c, "2026-11-06", 630)).toBe(2); // 10:30: a1 + a2
    expect(peopleAt(c, "2026-11-06", 660)).toBe(2); // 11:00: a2 + walk-in
    expect(peopleAt(c, "2026-11-06", 735)).toBe(1); // 12:15: walk-in
    expect(peopleAt(c, "2026-11-06", 750)).toBe(0); // 12:30: ends are exclusive
    expect(peakPeople(c, "2026-11-06", 600, 90)).toBe(2);
    // One timeline, earliest first: bookings and walk-ins together.
    expect(entriesOf(c, "2026-11-06").map((e) => e.id)).toEqual([
      "a1",
      "a3",
      "a2",
      "v1",
      "v2",
    ]);
  });

  it("length follows the services, rounded up to the slot", () => {
    expect(autoMinutes([], 15)).toBe(60);
    expect(autoMinutes([30], 15)).toBe(30);
    expect(autoMinutes([30, 90], 15)).toBe(120);
    // Round 6: an untimed add-on next to a timed service adds nothing (was 60 each: 30 → 150).
    expect(autoMinutes([40, null], 15)).toBe(45);
    expect(autoMinutes([30, null, null], 15)).toBe(30);
    expect(autoMinutes([null], 15)).toBe(60);
    expect(autoMinutes([null, null], 15)).toBe(120);
    expect(autoMinutes([5], 15)).toBe(15);
  });

  it("an untimed service shows what it adds", () => {
    expect(addedMinutes(30, [30, null])).toBe(30);
    expect(addedMinutes(null, [30, null])).toBe(0);
    expect(addedMinutes(null, [null])).toBe(60);
  });

  it("weeks start on Monday", () => {
    expect(mondayOf("2026-11-06")).toBe("2026-11-02");
    expect(mondayOf("2026-11-02")).toBe("2026-11-02");
    expect(daysFrom("2026-11-02", 7).at(-1)).toBe("2026-11-08");
  });

  it("says whether the visit was paid", () => {
    const money = (n: number) => `${String(n)}đ`;
    expect(paymentLabel(appt({}), money)).toBe("Chưa thanh toán");
    expect(
      paymentLabel(
        appt({ status: "done", payment_state: "paid", paid_amount: 349_000 }),
        money,
      ),
    ).toBe("Đã thanh toán 349000đ");
    expect(
      paymentLabel(appt({ status: "done", payment_state: "no_charge" }), money),
    ).toMatch(/không phát sinh/);
  });
});
