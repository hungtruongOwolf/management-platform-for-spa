import { spaDateTime } from "../../lib/format";
import { GENDER_LABEL, fold, wins } from "./draw";
import type { LuckyDrawDetail } from "./types";

/**
 * A cell Excel or Sheets would run as a formula (`=`, `+`, `-`, `@`, or a leading tab / CR, also
 * after spaces) gets a leading apostrophe, so a pasted name like `=HYPERLINK(…)` stays text
 * (OWASP "CSV injection"). Every cell is quoted.
 */
export function csvCell(value: string | number): string {
  let text = String(value);
  if (/^\s*[=+\-@]|^[\t\r]/.test(text)) text = `'${text}`;
  return `"${text.replace(/"/g, '""')}"`;
}

/**
 * "Tải danh sách (CSV)" (AC6.3): every result in spin order, forfeited ones included and marked.
 * UTF-8 with a BOM so Excel reads the Vietnamese right; CRLF rows. Built from the data on screen.
 */
export function winnersCsv(draw: LuckyDrawDetail, time: (iso: string) => string = spaDateTime): string {
  const rows: (string | number)[][] = [["Lượt", "Họ tên", "Giới tính", "Giải thưởng", "Lúc quay", "Trạng thái"]];
  for (const { result, person, prize } of wins(draw)) {
    rows.push([
      result.seq,
      person?.name ?? "",
      person ? GENDER_LABEL[person.gender] : "",
      prize?.name ?? "",
      time(result.drawn_at),
      result.forfeited_at === null ? "Đã trúng" : "Vắng mặt",
    ]);
  }
  return `\uFEFF${rows.map((row) => row.map(csvCell).join(",")).join("\r\n")}\r\n`;
}

/** "Tri ân 20/11" → "vong-quay-tri-an-20-11.csv". */
export function csvFileName(name: string): string {
  const slug = fold(name).replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  return `vong-quay${slug ? `-${slug}` : ""}.csv`;
}

/** Hands the file to the browser's download. */
export function downloadCsv(draw: LuckyDrawDetail): void {
  const url = URL.createObjectURL(new Blob([winnersCsv(draw)], { type: "text/csv;charset=utf-8" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = csvFileName(draw.name);
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => {
    URL.revokeObjectURL(url);
  }, 1000);
}
