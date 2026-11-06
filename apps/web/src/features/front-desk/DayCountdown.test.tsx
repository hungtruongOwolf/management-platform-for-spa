import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DayCountdown } from "./DayCountdown";

afterEach(() => {
  vi.useRealTimers();
});

describe("DayCountdown", () => {
  // EM 2026-11-07: the time sat in an 8.5ch box, leaving a gap after "Còn". Each digit now has its
  // own slot (the font has no tabular figures), so the line is exactly as wide as the time.
  it("reads the time once and draws each digit in its own fixed slot", () => {
    vi.useFakeTimers();
    // 20:47:37 in Asia/Ho_Chi_Minh → 03:12:23 to the spa's midnight.
    vi.setSystemTime(new Date("2026-11-07T13:47:37Z"));
    const { container } = render(<DayCountdown owner={false} />);
    const time = container.querySelector(".day-clock-time");
    expect(time).not.toBeNull();
    expect(screen.getByText("03:12:23")).toHaveClass("sr-only");
    const drawn = time?.querySelector('[aria-hidden="true"]');
    expect(drawn?.textContent).toBe("03:12:23");
    const digits = Array.from(time?.querySelectorAll(".day-clock-digit") ?? []).map((d) => d.textContent);
    expect(digits).toEqual(["0", "3", "1", "2", "2", "3"]);
  });
});
