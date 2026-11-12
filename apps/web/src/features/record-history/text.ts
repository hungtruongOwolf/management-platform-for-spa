// Words of a record's history (design record-history §1.5): the meta line "Ghi bởi Hà · 10:07 ·
// đã sửa 1 lần" and one line per change. Part of the lazy chunk (it uses sentence.ts).
import type { ActivityEntry, RecordHistory } from "@vpyd/api-client";
import { spaTime, spaToday } from "../../lib/format";
import { actorName, changeText, discountOf, historyVerb, moneyOf } from "../activity/sentence";

/** "03/02/2025". */
export function fullDay(iso: string): string {
  return iso.split("-").reverse().join("/");
}

/** "12/11" in the same year as `ref`, "12/11/2025" otherwise (ISO days). */
function dayText(iso: string, ref: string): string {
  const full = fullDay(iso);
  return iso.slice(0, 4) === ref.slice(0, 4) ? full.slice(0, 5) : full;
}

/**
 * When a line happened: "10:30" on the record's own day, "10:30 12/11" on another, the year added
 * when it is another year too.
 */
export function whenText(at: string, day: string, recordDay: string): string {
  return day === recordDay ? spaTime(at) : `${spaTime(at)} ${dayText(day, recordDay)}`;
}

type Kind = RecordHistory["record"]["type"];

/** Records said with the time they were written (a day of their own), not only the day. */
const TIMED: ReadonlySet<Kind> = new Set(["visit", "appointment", "attendance", "daily_review", "adjustment"]);

/** The price list's records and the spa's settings: said by their last edit (design §2.10). */
const EDITED: ReadonlySet<Kind> = new Set(["service", "package_plan", "service_category", "payroll_rules", "appointment_settings"]);

/** The spa's settings: nobody "created" them, every save is a "sửa". */
const SETTINGS: ReadonlySet<Kind> = new Set(["payroll_rules", "appointment_settings"]);

/**
 * The price list and the settings (design §1.5, §2.10, K37): "Sửa lần cuối bởi Lan · 12/11/2026[ ·
 * đã sửa 2 lần]" by the newest save; a service, plan or group only added: "Ghi bởi Hà ·
 * 03/02/2025"; seeded with the spa, no trail: "Chưa rõ ai ghi"; settings never saved (still the
 * defaults): "Chưa có ai sửa".
 */
function editedHead(h: RecordHistory): string {
  const { created, last } = h.summary;
  const latest = last ?? (created?.actor ? { actor: created.actor, day: created.day } : undefined);
  if (!latest) return SETTINGS.has(h.record.type) ? "Chưa có ai sửa" : "Chưa rõ ai ghi";
  const verb = last || SETTINGS.has(h.record.type) ? "Sửa lần cuối bởi" : "Ghi bởi";
  return `${verb} ${actorName({ actor: latest.actor })} · ${fullDay(latest.day)}`;
}

/**
 * The meta line, by what the history is about (after re-anchoring):
 * - a visit, a booking, a staff-book cell, a bonus / penalty: "Ghi bởi Hà · 15:10[ 12/11][ · đã
 *   sửa 2 lần][ · đã hủy]" (the date when it was written on another day than the record's own);
 * - a customer: "Tạo bởi Hà · 03/02/2025[ · đã sửa 1 lần]";
 * - a package: "Ghi bởi Hà · 12/11/2026[ · đã hủy]" (its lifecycle is no edit, D9);
 * - a paper-book entry: "Nhập từ sổ giấy · 03/02/2025"; nobody known: "Chưa rõ ai ghi"; a
 *   staff-book cell nobody ever wrote: "Chưa có ai ghi" (K23);
 * - the price list, the settings: by their last edit (`editedHead`).
 * `recordDay`: the record's own day (a visit's or a booking's business day, a cell's day, the day
 * a bonus happened); defaults to the day it was recorded.
 */
export function metaText(h: RecordHistory, recordDay?: string): string {
  const { created, edits, state } = h.summary;
  const kind = h.record.type;
  let head: string;
  if (EDITED.has(kind)) head = editedHead(h);
  else if (created?.source === "paper_book") head = `Nhập từ sổ giấy · ${fullDay(created.day)}`;
  else if (!created?.actor) head = (kind === "attendance" || kind === "daily_review") && h.items.length === 0 && h.omitted === 0 ? "Chưa có ai ghi" : "Chưa rõ ai ghi";
  else {
    const who = actorName({ actor: created.actor });
    const verb = kind === "customer" ? "Tạo bởi" : "Ghi bởi";
    const own = recordDay ?? created.day;
    const when =
      TIMED.has(kind) && created.at
        ? created.day === own
          ? spaTime(created.at)
          : `${spaTime(created.at)} ${dayText(created.day, own)}`
        : fullDay(created.day);
    head = `${verb} ${who} · ${when}`;
  }
  const edited = kind !== "customer_package" && edits > 0 ? ` · đã sửa ${String(edits)} lần` : "";
  const gone = state === "voided" || state === "cancelled" ? " · đã hủy" : "";
  return `${head}${edited}${gone}`;
}

/**
 * One line: "Hà ghi lượt · Thu 300.000 ₫", "Lan hủy gói · Hoàn 1.500.000 ₫ · lý do: khách đổi ý".
 * The record is on screen already, so its subject is never repeated.
 */
export function lineText(e: ActivityEntry): string {
  return [`${actorName(e)} ${historyVerb(e)}`, moneyOf(e), discountOf(e), e.reason ? `lý do: ${e.reason}` : ""]
    .filter(Boolean)
    .join(" · ");
}

/** The changed values under a line: "KTV: Thu → Hoa", "SĐT (đã đổi)". */
export function changeLines(e: ActivityEntry): string[] {
  return e.changes.map(changeText);
}

/**
 * The record's own day when the screen does not say: the day it was recorded, else today. The
 * price list and the settings have no day of their own: a line says its date unless it is today.
 */
export function recordDayOf(h: RecordHistory, day?: string): string {
  return day ?? (EDITED.has(h.record.type) ? undefined : h.summary.created?.day) ?? spaToday();
}
