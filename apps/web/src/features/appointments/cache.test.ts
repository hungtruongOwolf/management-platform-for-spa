import { QueryClient } from "@tanstack/react-query";
import type { Appointment, Calendar } from "@vpyd/api-client";
import { describe, expect, it } from "vitest";
import { putAppointment } from "./cache";

const booking = (id: string, status: Appointment["status"]) => ({ id, status }) as Appointment;
const calendar = (...appointments: Appointment[]) => ({ appointments }) as Calendar;

describe("putAppointment", () => {
  it("puts the server's answer in every cached calendar holding that booking, and only there", () => {
    const qc = new QueryClient();
    const day = calendar(booking("a", "booked"), booking("b", "booked"));
    const other = calendar(booking("c", "booked"));
    qc.setQueryData(["appointments", "2026-11-07", "2026-11-07"], day);
    qc.setQueryData(["appointments", "2026-11-03", "2026-11-09"], calendar(booking("a", "booked")));
    qc.setQueryData(["appointments", "2026-11-10", "2026-11-10"], other);

    putAppointment(qc, booking("a", "arrived"));

    const statuses = (key: string[]) => qc.getQueryData<Calendar>(key)?.appointments.map((x) => `${x.id}:${x.status}`);
    expect(statuses(["appointments", "2026-11-07", "2026-11-07"])).toEqual(["a:arrived", "b:booked"]);
    expect(statuses(["appointments", "2026-11-03", "2026-11-09"])).toEqual(["a:arrived"]);
    expect(qc.getQueryData(["appointments", "2026-11-10", "2026-11-10"])).toBe(other);
  });
});
