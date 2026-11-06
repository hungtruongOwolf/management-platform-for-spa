import type { Schemas } from "@vpyd/api-client";

export type AttendanceStatus = Schemas["AttendanceStatus"];
export type Level = Schemas["Level"];
export type PayFlag = Schemas["PayFlag"];

/** The sheet's codes: ✓ đi làm, P, ĐX, K. */
export const STATUS_CODE: Record<AttendanceStatus, string> = {
  present: "✓",
  leave_p: "P",
  leave_dx: "ĐX",
  absent_k: "K",
};

export const STATUS_LABEL: Record<AttendanceStatus, string> = {
  present: "Đi làm",
  leave_p: "Nghỉ có phép (P)",
  leave_dx: "Nghỉ đột xuất (ĐX)",
  absent_k: "Nghỉ không lý do (K)",
};

export const STATUS_HINT: Record<AttendanceStatus, string> = {
  present: "Ghi giờ đến để tính phút đi muộn",
  leave_p: "Có lý do chính đáng, báo trước ít nhất 3 ngày",
  leave_dx: "Bất khả kháng, bổ sung giấy tờ khi đi làm lại",
  absent_k: "Không lý do, không báo hoặc không bổ sung được giấy tờ",
};

export const STATUSES: AttendanceStatus[] = ["present", "leave_p", "leave_dx", "absent_k"];

export const LEVEL_LABEL: Record<Level, string> = {
  m1: "Mức 1 (Xuất sắc)",
  m2: "Mức 2 (Tốt)",
  m3: "Mức 3 (Đạt)",
  fail: "Chưa đạt",
};

export const LEVEL_SHORT: Record<Level, string> = { m1: "Mức 1", m2: "Mức 2", m3: "Mức 3", fail: "Chưa đạt" };

export const LEVELS: Level[] = ["m1", "m2", "m3", "fail"];

export const FLAG_LABEL: Record<PayFlag, string> = {
  no_pay_terms: "Chưa có lương cứng",
  unrecorded_days: "Còn ngày chưa chấm công",
  absent_k5_plus: "Nghỉ K từ 5 ngày (cần chủ spa xem)",
  evaluation_missing: "Chưa đánh giá tháng",
};

/** "2026-11" → "Tháng 11/2026". */
export function monthLabel(ym: string): string {
  const [y, m] = ym.split("-");
  return `Tháng ${String(Number(m))}/${y ?? ""}`;
}

/** "2026-11" ± n months. */
export function shiftMonth(ym: string, n: number): string {
  const [y, m] = ym.split("-").map(Number);
  const d = new Date(Date.UTC(y ?? 2026, (m ?? 1) - 1 + n, 1));
  return `${String(d.getUTCFullYear())}-${String(d.getUTCMonth() + 1).padStart(2, "0")}`;
}

/** Every day `YYYY-MM-DD` of a month. */
export function daysOf(ym: string): string[] {
  const [y, m] = ym.split("-").map(Number);
  const count = new Date(Date.UTC(y ?? 2026, m ?? 1, 0)).getUTCDate();
  return Array.from({ length: count }, (_, i) => `${ym}-${String(i + 1).padStart(2, "0")}`);
}

const WEEKDAY = ["CN", "T2", "T3", "T4", "T5", "T6", "T7"];

/** The sheet's "Thứ" column: CN, T2 … T7. */
export function weekdayShort(iso: string): string {
  return WEEKDAY[new Date(`${iso}T00:00:00Z`).getUTCDay()] ?? "";
}

/** 500 → "5%", 750 → "7,5%". */
export function percentOfBp(bp: number): string {
  return `${(bp / 100).toLocaleString("vi-VN", { maximumFractionDigits: 2 })}%`;
}
