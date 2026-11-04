import { describe, expect, it } from "vitest";
import { formatVnd } from "./money";

describe("formatVnd", () => {
  it("formats with dot thousands separators and a trailing đ", () => {
    expect(formatVnd(1_500_000)).toBe("1.500.000đ");
    expect(formatVnd(49_000)).toBe("49.000đ");
  });

  it("formats zero", () => {
    expect(formatVnd(0)).toBe("0đ");
  });

  it("rejects non-integer or unsafe amounts instead of silently rounding", () => {
    expect(() => formatVnd(1.5)).toThrow(RangeError);
    expect(() => formatVnd(Number.MAX_SAFE_INTEGER + 1)).toThrow(RangeError);
  });
});
