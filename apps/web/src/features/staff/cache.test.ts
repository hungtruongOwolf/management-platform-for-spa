import { describe, expect, it } from "vitest";
import type { AttendanceDay, AttendanceSheet, ReviewSheet } from "./api";
import { withAttendance, withReviews } from "./cache";

const day = (staff_id: string, date: string, status: AttendanceDay["status"] = "present"): AttendanceDay => ({
  staff_id,
  date,
  status,
  late_minutes: 0,
});

const totals = { present: 0, late_count: 0, late_minutes: 0, leave_p: 0, leave_dx: 0, absent_k: 0, unrecorded: 0 };

function attendanceSheet(days: AttendanceDay[]): AttendanceSheet {
  const person = (staff_id: string) => ({
    staff_id,
    full_name: staff_id,
    position: "Kỹ thuật viên",
    shift_start: "08:00",
    days: days.filter((d) => d.staff_id === staff_id),
    totals,
    can_record: true,
    periods: [],
    visit_days: [],
  });
  return { month: "2026-11", today: "2026-11-07", days_in_month: 30, default_shift_start: "08:00", closed: false, staff: [person("a"), person("b")] };
}

describe("withAttendance", () => {
  it("replaces the written person-day with the server's answer and keeps everyone else", () => {
    const sheet = attendanceSheet([day("a", "2026-11-06"), day("a", "2026-11-07", "absent_k"), day("b", "2026-11-07")]);
    const next = withAttendance(sheet, [{ staff_id: "a", date: "2026-11-07", status: "leave_p" }], [day("a", "2026-11-07", "leave_p")]);
    expect(next.staff[0]?.days.map((d) => [d.date, d.status])).toEqual([
      ["2026-11-06", "present"],
      ["2026-11-07", "leave_p"],
    ]);
    expect(next.staff[1]).toBe(sheet.staff[1]);
  });

  it("removes a cleared day (the server answers nothing for it)", () => {
    const sheet = attendanceSheet([day("a", "2026-11-07")]);
    const next = withAttendance(sheet, [{ staff_id: "a", date: "2026-11-07", status: null }], []);
    expect(next.staff[0]?.days).toEqual([]);
  });

  it("leaves another month's sheet as it was", () => {
    const sheet = { ...attendanceSheet([]), month: "2026-10" };
    const next = withAttendance(sheet, [{ staff_id: "a", date: "2026-11-07", status: "present" }], [day("a", "2026-11-07")]);
    expect(next.staff[0]?.days).toEqual([]);
  });
});

describe("withReviews", () => {
  const sheet: ReviewSheet = {
    month: "2026-11",
    today: "2026-11-07",
    closed: false,
    staff: [
      {
        staff_id: "a",
        full_name: "A",
        position: "Kỹ thuật viên",
        can_record: true,
        compliments: 3,
        complaints: 1,
        days: [
          { staff_id: "a", date: "2026-11-06", compliments: 2, complaints: 1, note: null },
          { staff_id: "a", date: "2026-11-07", compliments: 1, complaints: 0, note: null },
        ],
      },
    ],
  };

  it("sets the day's counts and the month totals", () => {
    const next = withReviews(sheet, [{ staff_id: "a", date: "2026-11-07", compliments: 4, complaints: 2, note: null }]);
    expect(next.staff[0]?.days.find((d) => d.date === "2026-11-07")).toMatchObject({ compliments: 4, complaints: 2 });
    expect(next.staff[0]).toMatchObject({ compliments: 6, complaints: 3 });
  });

  it("all zero and no note clears the day, like the server", () => {
    const next = withReviews(sheet, [{ staff_id: "a", date: "2026-11-07", compliments: 0, complaints: 0, note: null }]);
    expect(next.staff[0]?.days.map((d) => d.date)).toEqual(["2026-11-06"]);
    expect(next.staff[0]).toMatchObject({ compliments: 2, complaints: 1 });
  });
});
