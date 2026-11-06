import { describe, expect, it } from "vitest";
import { daysBetween, isClosedPackage, packageState } from "./packageState";

const today = "2026-11-06";
const p = (status: "active" | "expired" | "exhausted" | "cancelled", left: number, expires: string | null = "2027-05-01") => ({
  status,
  sessions_remaining: left,
  expires_on: expires,
});

describe("packageState: one state per package", () => {
  it("keeps the server's closed and expired states", () => {
    expect(packageState(p("cancelled", 4), today)).toBe("cancelled");
    expect(packageState(p("exhausted", 0), today)).toBe("exhausted");
    expect(packageState(p("expired", 3, "2026-10-01"), today)).toBe("expired");
  });

  it("marks an active package near its end as ending (few sessions or expiry within 30 days)", () => {
    expect(packageState(p("active", 8), today)).toBe("active");
    expect(packageState(p("active", 2), today)).toBe("ending");
    expect(packageState(p("active", 8, "2026-12-06"), today)).toBe("ending");
    expect(packageState(p("active", 8, "2026-12-07"), today)).toBe("active");
  });

  it("never calls a no-expiry package ending because of its date", () => {
    expect(packageState(p("active", 8, null), today)).toBe("active");
  });

  it("closes used-up and cancelled packages only", () => {
    expect(isClosedPackage(p("cancelled", 4))).toBe(true);
    expect(isClosedPackage(p("exhausted", 0))).toBe(true);
    expect(isClosedPackage(p("expired", 3))).toBe(false);
    expect(isClosedPackage(p("active", 1))).toBe(false);
  });

  it("counts spa calendar days", () => {
    expect(daysBetween("2026-11-06", "2026-12-06")).toBe(30);
    expect(daysBetween("2026-11-06", "2026-11-01")).toBe(-5);
  });
});
