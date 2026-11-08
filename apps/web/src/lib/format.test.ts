import { describe, expect, it } from "vitest";
import { spaDateTime, spaTime } from "./format";

describe("spa clock (F27: never the browser's time zone)", () => {
  it("shows an instant on the spa's clock and calendar, dd/mm/yyyy", () => {
    // 12:27 UTC = 19:27 in Asia/Ho_Chi_Minh, whatever zone the browser (or this runner) is in.
    expect(spaTime("2026-11-08T12:27:27Z")).toBe("19:27");
    expect(spaDateTime("2026-11-08T12:27:27.123456Z")).toBe("19:27 08/11/2026");
  });

  it("moves to the spa's next day before 07:00 UTC+7, with a 24-hour clock", () => {
    expect(spaDateTime("2026-11-07T23:30:00Z")).toBe("06:30 08/11/2026");
    expect(spaTime("2026-11-07T17:05:00Z")).toBe("00:05");
  });
});
