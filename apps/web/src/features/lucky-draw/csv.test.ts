import { describe, expect, it } from "vitest";
import { csvCell, csvFileName, winnersCsv } from "./csv";
import { detail, person, prize, result } from "./fixtures";

describe("winnersCsv (AC6.3)", () => {
  const draw = detail({
    participants: [person("p1", '=HYPERLINK("x")', "female", 0, "r1"), person("p2", 'Minh "Bé"', "male", 1, "r2")],
    prizes: [prize("g1", "Gội đầu, dưỡng sinh")],
    results: [result("r1", 1, "g1", "p1"), result("r2", 2, "g1", "p2", true)],
  });

  it("is UTF-8 with a BOM, CRLF rows, the six columns, every result in spin order", () => {
    const csv = winnersCsv(draw, (iso) => iso.slice(11, 16));
    expect(csv.startsWith("﻿")).toBe(true);
    expect(csv.slice(1).split("\r\n")).toEqual([
      '"Lượt","Họ tên","Giới tính","Giải thưởng","Lúc quay","Trạng thái"',
      `"1","'=HYPERLINK(""x"")","Nữ","Gội đầu, dưỡng sinh","01:00","Đã trúng"`,
      '"2","Minh ""Bé""","Nam","Gội đầu, dưỡng sinh","02:00","Vắng mặt"',
      "",
    ]);
  });

  it("writes the time on the spa's clock by default", () => {
    expect(winnersCsv(draw)).toContain('"08:00 20/11/2026"');
  });
});

describe("csvCell: CSV injection guard", () => {
  it("prefixes a cell a spreadsheet would run", () => {
    for (const bad of ["=1+1", "+1", "-1", "@SUM(A1)", "\tx", "\rx", "  =1"]) expect(csvCell(bad).startsWith(`"'`)).toBe(true);
    expect(csvCell("Lan")).toBe('"Lan"');
    expect(csvCell(12)).toBe('"12"');
    expect(csvCell("Lê-Na")).toBe('"Lê-Na"');
  });
});

describe("csvFileName", () => {
  it("names the file after the program", () => {
    expect(csvFileName("Tri ân 20/11")).toBe("vong-quay-tri-an-20-11.csv");
    expect(csvFileName("!!!")).toBe("vong-quay.csv");
  });
});
