import type { Schemas } from "@vpyd/api-client";
import type { IconName } from "../../app/Icon";

export type AttendanceStatus = Schemas["AttendanceStatus"];
export type Level = Schemas["Level"];
export type PayFlag = Schemas["PayFlag"];

/**
 * A status in words, everywhere it is shown (F20, EM 2026-11-08: the paper sheet's P / ĐX / K
 * codes are gone from every screen; a status is its words, or its mark with the words beside it
 * or in its accessible name).
 */
export const STATUS_LABEL: Record<AttendanceStatus, string> = {
  present: "Đi làm",
  leave_p: "Nghỉ có phép",
  leave_dx: "Nghỉ đột xuất",
  absent_k: "Nghỉ không phép",
};

/**
 * Each status's mark, the daily book tiles' icon, always on its status colour (`att-<status>`):
 * colour is never the only signal, and the icon is never the only name.
 */
export const STATUS_ICON: Record<AttendanceStatus, IconName> = {
  present: "check",
  leave_p: "calendar",
  leave_dx: "alert",
  absent_k: "x",
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

/** Chip of a month's result: good levels calm, Mức 3 amber, Chưa đạt red. */
export const LEVEL_BADGE: Record<Level, string> = { m1: "badge-ok", m2: "badge-ok", m3: "badge-warn", fail: "badge-danger" };

/** The scorecard's criteria by key (hr::evaluation::CRITERIA), for views without the sheet's list. */
export const CRITERION_LABEL: Record<string, string> = {
  technique: "Kỹ thuật liệu trình",
  attitude: "Thái độ và giao tiếp",
  customer_feedback: "Phản hồi khách hàng",
  diligence: "Chuyên cần",
  hygiene: "Vệ sinh và tác phong",
  learning: "Tinh thần học hỏi",
};

export const FLAG_LABEL: Record<PayFlag, string> = {
  no_pay_terms: "Chưa có lương cứng",
  unrecorded_days: "Còn ngày chưa chấm công",
  absent_k5_plus: "Nghỉ không phép từ 5 ngày (cần chủ spa xem)",
  evaluation_missing: "Chưa đánh giá tháng",
  net_pay_shortfall: "Khoản trừ vượt lương: thực lĩnh 0đ",
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
