import { fold } from "./draw";
import type { Gender, ParticipantWrite } from "./types";

/** Limits of the API (spec lucky-draw §3, AC2.1). */
export const NAME_MAX = 120;
export const PEOPLE_MAX = 2000;
export const BATCH_MAX = 500;

export interface RosterNote {
  /** Line number in the pasted text, from 1 (blank lines count: they are lines the owner sees). */
  line: number;
  text: string;
}

export interface Roster {
  people: ParticipantWrite[];
  /** The line each person came from (same order as `people`). */
  lines: number[];
  /** Lines that stop the list from being sent. */
  errors: RosterNote[];
  /** Same name as someone already in, or as an earlier line: added anyway (AC2.2). */
  warnings: RosterNote[];
}

const GENDERS: Record<string, Gender> = { nu: "female", nam: "male" };
const HEADER = /^ho (va )?ten$/;

/** Length in characters (code points), as the API counts it: "Nguyễn" is 6. */
export function chars(text: string): number {
  return Array.from(text).length;
}

/** One space between words, no edge spaces (pasted cells often carry tabs or non-breaking spaces). */
export function cleanName(name: string): string {
  return name.replace(/\s+/g, " ").trim();
}

/** "Họ tên, Nữ" / "Họ tên<Tab>Nam" / "Họ tên; Nam": the name, and the gender cell (`null`: none). */
function split(line: string): [string, string | null] {
  if (line.includes("\t")) {
    const cells = line.split("\t").map((c) => c.trim()).filter(Boolean);
    return cells.length < 2 ? [cells[0] ?? "", null] : [cells[0] ?? "", cells[cells.length - 1] ?? ""];
  }
  const at = Math.max(line.lastIndexOf(","), line.lastIndexOf(";"));
  return at < 0 ? [line, null] : [line.slice(0, at), line.slice(at + 1)];
}

/**
 * Reads a pasted list, one person per line (AC2.2). Blank lines are skipped, and so is a first line
 * that is the header ("Họ tên, Giới tính"). Gender ignores case and accents (`nu`, `Nữ`, `NAM`).
 * Every bad line is reported by number; `existing` are the names already in the program.
 */
export function parseRoster(text: string, existing: readonly string[] = []): Roster {
  const people: ParticipantWrite[] = [];
  const lines: number[] = [];
  const errors: RosterNote[] = [];
  const warnings: RosterNote[] = [];
  const known = new Set(existing.map(fold));
  const seen = new Map<string, number>();
  let first = true;
  text.split(/\r\n|\n|\r/).forEach((raw, index) => {
    const line = index + 1;
    if (raw.trim() === "") return;
    const [rawName, rawGender] = split(raw);
    const name = cleanName(rawName);
    const isHeader = first && HEADER.test(fold(name)) && fold(rawGender ?? "") === "gioi tinh";
    first = false;
    if (isHeader) return;
    const gender = rawGender === null ? undefined : GENDERS[fold(rawGender).replace(/ /g, "")];
    if (name === "") {
      errors.push({ line, text: `Dòng ${String(line)}: thiếu họ tên — ví dụ: Nguyễn Thị Lan, Nữ` });
      return;
    }
    if (chars(name) > NAME_MAX) {
      errors.push({ line, text: `Dòng ${String(line)}: họ tên dài hơn ${String(NAME_MAX)} ký tự` });
      return;
    }
    if (rawGender === null || rawGender.trim() === "") {
      errors.push({ line, text: `Dòng ${String(line)}: thiếu giới tính — ví dụ: Trần Văn Minh, Nam` });
      return;
    }
    if (!gender) {
      errors.push({ line, text: `Dòng ${String(line)}: giới tính “${rawGender.trim()}” không rõ — ghi Nữ hoặc Nam` });
      return;
    }
    const key = fold(name);
    const earlier = seen.get(key);
    if (known.has(key)) warnings.push({ line, text: `Dòng ${String(line)}: “${name}” trùng tên với người đã có` });
    else if (earlier !== undefined)
      warnings.push({ line, text: `Dòng ${String(line)}: “${name}” trùng tên với dòng ${String(earlier)}` });
    if (earlier === undefined) seen.set(key, line);
    people.push({ name, gender });
    lines.push(line);
  });
  return { people, lines, errors, warnings };
}

/** The list in batches the API takes in one request (≤ 500), sent one after another. */
export function batches<T>(items: readonly T[], size = BATCH_MAX): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size));
  return out;
}
