import { describe, expect, it } from "vitest";
import { discountBody, discountPreview, serviceInstant, visitBody } from "./api";
import { deviceZoneDiffers, hms, msUntilMidnight } from "./countdown";

describe("countdown to the spa's midnight (AC5.3)", () => {
  it("counts to 00:00 in Asia/Ho_Chi_Minh whatever the device zone", () => {
    // 23:00:00 in Vietnam = 16:00:00 UTC: one hour left.
    expect(msUntilMidnight(new Date("2026-11-05T16:00:00Z"))).toBe(3_600_000);
    // 00:00:30 in Vietnam: almost a whole day left.
    expect(hms(msUntilMidnight(new Date("2026-11-05T17:00:30Z")))).toBe("23:59:30");
    // A device in New York sees the same count: only the instant matters.
    expect(msUntilMidnight(new Date("2026-11-05T16:00:00Z"), "Asia/Ho_Chi_Minh")).toBe(
      msUntilMidnight(new Date("2026-11-05T16:00:00Z")),
    );
  });

  it("formats hh:mm:ss, rounding seconds up", () => {
    expect(hms(3_723_000)).toBe("01:02:03");
    expect(hms(1)).toBe("00:00:01");
    expect(hms(0)).toBe("00:00:00");
  });

  it("notices a device in another zone", () => {
    const now = new Date("2026-11-05T05:00:00Z");
    const own = new Intl.DateTimeFormat().resolvedOptions().timeZone;
    expect(deviceZoneDiffers(own, now)).toBe(false);
    const elsewhere = own === "Pacific/Kiritimati" ? "Pacific/Pago_Pago" : "Pacific/Kiritimati";
    expect(deviceZoneDiffers(elsewhere, now)).toBe(true);
  });
});

describe("visit discount preview (AC8.1, the server decides)", () => {
  it("rounds a percent down like the server and caps an amount at the subtotal", () => {
    expect(discountPreview(448_000, { kind: "percent", value: "10", reason: "x" })).toBe(44_800);
    expect(discountPreview(99_999, { kind: "percent", value: "10", reason: "x" })).toBe(9_999);
    expect(discountPreview(100_000, { kind: "amount", value: "20.000", reason: "x" })).toBe(20_000);
    expect(discountPreview(100_000, { kind: "amount", value: "200000", reason: "x" })).toBe(0);
    expect(discountPreview(100_000, { kind: "percent", value: "abc", reason: "x" })).toBe(0);
    expect(discountPreview(100_000, null)).toBe(0);
  });

  it("sends exactly one of percent / amount, with the reason", () => {
    expect(discountBody({ kind: "percent", value: "10", reason: " Khách quen " })).toEqual({
      percent: 10,
      amount: null,
      reason: "Khách quen",
    });
    const body = visitBody("c1", [], [], "", {
      discount: { kind: "amount", value: "50000", reason: "Tặng" },
      checkInAt: "2026-11-04T11:30:00.000Z",
    });
    expect(body.discount).toEqual({ percent: null, amount: 50_000, reason: "Tặng" });
    expect(body.check_in_at).toBe("2026-11-04T11:30:00.000Z");
    expect("discount" in visitBody("c1", [], [], "")).toBe(false);
  });
});

describe("service time (giờ làm, AC5.1)", () => {
  it("is the spa's wall clock on the chosen day, or now when left empty", () => {
    expect(serviceInstant("2026-11-04", "18:30")).toBe("2026-11-04T11:30:00.000Z");
    expect(serviceInstant("2026-11-04", "")).toBeNull();
  });
});
