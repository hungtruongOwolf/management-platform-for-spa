import { describe, expect, it } from "vitest";
import { blockOf, columnLanes, density } from "./layout";

const at = (h: number, m = 0) => h * 60 + m;

describe("day grid event layer (round 6: events float over the grid)", () => {
  it("an event's height is its real time span, in slots", () => {
    // 30-minute "Gội đầu" at 17:30 on a 15-minute grid from 09:00: 2 slots, not 10.
    expect(blockOf(at(17, 30), 30, at(9), at(21), 15)).toEqual({
      top: 34,
      height: 2,
    });
    expect(blockOf(at(9), 90, at(9), at(21), 15)).toEqual({
      top: 0,
      height: 6,
    });
  });

  it("a walk-in off the slot grid sits at its real minute", () => {
    expect(blockOf(at(17, 37), 30, at(9), at(21), 15)).toEqual({
      top: (8 * 60 + 37) / 15,
      height: 2,
    });
  });

  it("clips to opening hours and never grows the grid", () => {
    expect(blockOf(at(8, 30), 60, at(9), at(21), 15)).toEqual({
      top: 0,
      height: 2,
    });
    expect(blockOf(at(20, 30), 60, at(9), at(21), 15)).toEqual({
      top: 46,
      height: 2,
    });
    expect(blockOf(at(21, 15), 30, at(9), at(21), 15)).toBeNull();
    expect(blockOf(at(10), 0, at(9), at(21), 15)).toBeNull();
  });

  it("only overlapping events share the column width", () => {
    const lanes = columnLanes([
      { id: "a", start: at(10), minutes: 60 },
      { id: "b", start: at(10, 30), minutes: 60 },
      { id: "c", start: at(11, 15), minutes: 30 },
      { id: "alone", start: at(14), minutes: 30 },
    ]);
    expect(lanes.get("a")).toEqual({ lane: 0, lanes: 2 });
    expect(lanes.get("b")).toEqual({ lane: 1, lanes: 2 });
    // c overlaps b only; a has ended, so c reuses lane 0 in the same cluster.
    expect(lanes.get("c")).toEqual({ lane: 0, lanes: 2 });
    expect(lanes.get("alone")).toEqual({ lane: 0, lanes: 1 });
  });

  it("back-to-back events do not overlap", () => {
    const lanes = columnLanes([
      { id: "a", start: at(10), minutes: 30 },
      { id: "b", start: at(10, 30), minutes: 30 },
    ]);
    expect(lanes.get("a")).toEqual({ lane: 0, lanes: 1 });
    expect(lanes.get("b")).toEqual({ lane: 0, lanes: 1 });
  });

  it("short events get a compact layout", () => {
    expect(density(1)).toBe("compact");
    expect(density(2)).toBe("short");
    expect(density(4)).toBe("full");
  });
});
