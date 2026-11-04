import { describe, expect, it } from "vitest";
import { describePrices, describeDuration } from "./prices";

describe("describePrices", () => {
  it("shows one price when it applies to everyone", () => {
    expect(describePrices({ all: 49_000 })).toEqual([{ label: null, amount: "49.000đ" }]);
  });

  it("shows female and male prices side by side", () => {
    expect(describePrices({ female: 79_000, male: 99_000 })).toEqual([
      { label: "Nữ", amount: "79.000đ" },
      { label: "Nam", amount: "99.000đ" },
    ]);
  });

  it("marks a gender that is not offered", () => {
    expect(describePrices({ female: 399_000 })).toEqual([
      { label: "Nữ", amount: "399.000đ" },
      { label: "Nam", amount: "—" },
    ]);
  });

  it("says when no price is set yet", () => {
    expect(describePrices({})).toEqual([{ label: null, amount: "Chưa có giá" }]);
  });
});

describe("describeDuration", () => {
  it("formats minutes and the consult case", () => {
    expect(describeDuration(90)).toBe("90 phút");
    expect(describeDuration(null)).toBe("Theo tư vấn");
    expect(describeDuration(undefined)).toBe("Theo tư vấn");
  });
});
