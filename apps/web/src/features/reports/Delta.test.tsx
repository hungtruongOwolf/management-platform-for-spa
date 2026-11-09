import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Delta } from "./ui";

const shown = (now: number, before: number) => {
  const { unmount } = render(<Delta now={now} before={before} label="so với hôm qua" />);
  const text = screen.getByText("so với hôm qua").parentElement?.textContent ?? "";
  unmount();
  return text.replace("so với hôm qua", "").trim();
};

describe("Delta", () => {
  it("compares against the previous value", () => {
    expect(shown(1_200_000, 1_000_000)).toBe("+20%");
    expect(shown(800_000, 1_000_000)).toBe("-20%");
    expect(shown(0, 0)).toBe("=");
  });

  it("keeps the direction right when net revenue is negative (refunds above sales)", () => {
    // From −500k to −100k is better: up, not down.
    expect(shown(-100_000, -500_000)).toBe("+80%");
    expect(shown(-500_000, 1_000_000)).toBe("-150%");
    // From zero: ±100% in the direction of the new value (it used to read +100% for a loss).
    expect(shown(-100_000, 0)).toBe("-100%");
    expect(shown(100_000, 0)).toBe("+100%");
  });
});
