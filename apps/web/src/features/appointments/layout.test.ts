import { describe, expect, it } from "vitest";
import { blockOf, cardLines, columnLanes, gutterBands } from "./layout";

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

  it("a card holds whole lines only: name, then phase, then services", () => {
    // 15 minutes: one line, the phase dot sits by the name.
    expect(cardLines(1, 1)).toEqual({ name: 1, meta: 0, services: 0 });
    // 30 minutes: name + phase.
    expect(cardLines(2, 2)).toEqual({ name: 1, meta: 1, services: 0 });
    // 45 minutes alone in its column: two name lines, the phase, one services line.
    expect(cardLines(3, 1)).toEqual({ name: 2, meta: 1, services: 1 });
    // 45 minutes sharing the column: the phase and tag may wrap, no room for services.
    expect(cardLines(3, 2)).toEqual({ name: 2, meta: 2, services: 0 });
    // An hour sharing the column: one services line under them.
    expect(cardLines(4, 2)).toEqual({ name: 2, meta: 2, services: 1 });
    expect(cardLines(8, 1).services).toBe(9);
    // Three lanes or more: a sliver, one line each.
    expect(cardLines(4, 3)).toEqual({ name: 1, meta: 1, services: 0 });
  });

  it("the time gutter has one band per hour, the hour on its first slot", () => {
    expect(gutterBands(at(9), at(11), 15)).toEqual([
      { row: 0, span: 4, m: at(9) },
      { row: 4, span: 4, m: at(10) },
    ]);
    // Opens at 08:30 and closes at 10:15: a half-hour band first, a short band last.
    expect(gutterBands(at(8, 30), at(10, 15), 15)).toEqual([
      { row: 0, span: 2, m: at(8, 30) },
      { row: 2, span: 4, m: at(9) },
      { row: 6, span: 1, m: at(10) },
    ]);
  });
});
