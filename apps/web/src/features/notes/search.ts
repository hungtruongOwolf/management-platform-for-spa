/**
 * URL state of /nhat-ky-ghi-chu (lib/searchState.ts). Imported by router.tsx, so it stays tiny and
 * free of the screen's code: the screen itself is a lazy chunk.
 */
import type { NoteKind } from "@vpyd/api-client";
import { param, searchSchema, type Parser } from "../../lib/searchState";

export const NOTE_KINDS = [
  "visit_note",
  "customer_note",
  "health_note",
  "attendance_note",
  "review_note",
  "adjustment",
  "follow_up",
  "void",
  "employment_end",
  "appointment_note",
  "evaluation_note",
  "staff_note",
  "package_extension",
  "discount",
  "appointment_reason",
  "package_note",
] as const satisfies readonly NoteKind[];

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** A record id; "" when absent or malformed (the URL is user input). */
const id: Parser<string> = (raw) => (typeof raw === "string" && UUID.test(raw) ? raw.toLowerCase() : "");

/**
 * /nhat-ky-ghi-chu?from=…&to=…&kind=…&staff=…&customer=…&q=… — "" dates = the last 30 days
 * (the API's default), "" kind = every kind the caller may read.
 */
export const notesSearch = searchSchema({
  from: param.day(),
  to: param.day(),
  kind: param.oneOf<NoteKind | "">(["", ...NOTE_KINDS], ""),
  staff: id,
  customer: id,
  q: param.text(80),
});

export type NotesSearch = ReturnType<typeof notesSearch.parse>;
