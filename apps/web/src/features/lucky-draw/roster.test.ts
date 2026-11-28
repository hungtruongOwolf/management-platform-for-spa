import { describe, expect, it } from "vitest";
import { BATCH_MAX, batches, chars, cleanName, parseRoster } from "./roster";

describe("parseRoster (AC2.2)", () => {
  it("reads comma, semicolon and Excel's tab, gender in any case, with or without accents", () => {
    const roster = parseRoster("Nguyễn Thị Lan, Nữ\nTrần Văn Minh\tNam\nLê Thu Hà; nu\nPhạm Quốc Bảo,NAM\nĐỗ An , NỮ");
    expect(roster.errors).toEqual([]);
    expect(roster.people).toEqual([
      { name: "Nguyễn Thị Lan", gender: "female" },
      { name: "Trần Văn Minh", gender: "male" },
      { name: "Lê Thu Hà", gender: "female" },
      { name: "Phạm Quốc Bảo", gender: "male" },
      { name: "Đỗ An", gender: "female" },
    ]);
    expect(roster.lines).toEqual([1, 2, 3, 4, 5]);
  });

  it("skips blank lines and a first-line header, and counts lines as the owner sees them", () => {
    const roster = parseRoster("Họ tên, Giới tính\n\nLan, Nữ\n   \nMinh\tNam\r\n");
    expect(roster.people.map((p) => p.name)).toEqual(["Lan", "Minh"]);
    expect(roster.lines).toEqual([3, 5]);
    // A header anywhere else is a bad line.
    expect(parseRoster("Lan, Nữ\nHọ tên, Giới tính").errors.map((e) => e.line)).toEqual([2]);
    expect(parseRoster("Họ và tên\tGiới tính\nLan\tNữ").people).toHaveLength(1);
  });

  it("names every bad line by its number, with what to write", () => {
    const roster = parseRoster("Lan, Nữ\nMinh\n, Nam\nHà, khác\nBảo,\nAn\t");
    expect(roster.people).toEqual([{ name: "Lan", gender: "female" }]);
    expect(roster.errors).toEqual([
      { line: 2, text: "Dòng 2: thiếu giới tính — ví dụ: Trần Văn Minh, Nam" },
      { line: 3, text: "Dòng 3: thiếu họ tên — ví dụ: Nguyễn Thị Lan, Nữ" },
      { line: 4, text: "Dòng 4: giới tính “khác” không rõ — ghi Nữ hoặc Nam" },
      { line: 5, text: "Dòng 5: thiếu giới tính — ví dụ: Trần Văn Minh, Nam" },
      { line: 6, text: "Dòng 6: thiếu giới tính — ví dụ: Trần Văn Minh, Nam" },
    ]);
  });

  it("never guesses a gender from the name: 'Trần Văn Nam' alone has none", () => {
    expect(parseRoster("Trần Văn Nam").errors).toHaveLength(1);
  });

  it("refuses a name over 120 characters, counting Vietnamese letters as one", () => {
    const long = "Ễ".repeat(121);
    expect(parseRoster(`${long}, Nữ`).errors[0]?.text).toBe("Dòng 1: họ tên dài hơn 120 ký tự");
    expect(parseRoster(`${"Ễ".repeat(120)}, Nữ`).errors).toEqual([]);
  });

  it("warns on a name already in the program or earlier in the paste, and still adds it", () => {
    const roster = parseRoster("le thi anh, Nữ\nMinh, Nam\nMINH ,Nam", ["Lê Thị Ánh"]);
    expect(roster.people).toHaveLength(3);
    expect(roster.warnings).toEqual([
      { line: 1, text: "Dòng 1: “le thi anh” trùng tên với người đã có" },
      { line: 3, text: "Dòng 3: “MINH” trùng tên với dòng 2" },
    ]);
  });

  it("tidies the spaces pasted cells carry", () => {
    expect(cleanName("  Nguyễn  Thị   Lan ")).toBe("Nguyễn Thị Lan");
    expect(parseRoster("Nguyễn  Lan\t\tNữ").people).toEqual([{ name: "Nguyễn Lan", gender: "female" }]);
  });

  it("keeps a comma inside the name: the last separator is the gender's", () => {
    expect(parseRoster("Lan, chị, Nữ").people).toEqual([{ name: "Lan, chị", gender: "female" }]);
  });
});

describe("batches", () => {
  it("splits a long list into requests of at most 500, in order", () => {
    const list = Array.from({ length: 1201 }, (_, i) => i);
    const parts = batches(list);
    expect(parts.map((p) => p.length)).toEqual([BATCH_MAX, BATCH_MAX, 201]);
    expect(parts.flat()).toEqual(list);
    expect(batches([])).toEqual([]);
  });
});

describe("chars", () => {
  it("counts code points", () => {
    expect(chars("Nguyễn")).toBe(6);
  });
});
