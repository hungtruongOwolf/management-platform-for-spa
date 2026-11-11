// The one state chip of a visit in a list (spec record-history D9, design §1.4): three conditions
// that exclude each other, checked in this order. Outside the lazy chunk: lists draw it on their
// own, from fields they already have.

export type VisitChip = { label: "Đã thay"; badge: "badge-muted" } | { label: "Đã hủy"; badge: "badge-danger" } | { label: "Đã sửa"; badge: "badge-muted" };

export interface VisitMarks {
  /** "Sửa lượt" replaced this visit with another one. */
  corrected_by_visit_id?: string | null | undefined;
  voided: boolean;
  /** Sent only to `record.history.read` holders (absent otherwise): what was recorded changed. */
  edited?: boolean | null | undefined;
}

/**
 * "Đã thay" = replaced by "Sửa lượt" (for everyone); "Đã hủy" = voided and not replaced (for
 * everyone); "Đã sửa" = `edited` and still standing (only those the server sends `edited` to).
 */
export function visitChip(v: VisitMarks): VisitChip | null {
  if (v.corrected_by_visit_id) return { label: "Đã thay", badge: "badge-muted" };
  if (v.voided) return { label: "Đã hủy", badge: "badge-danger" };
  if (v.edited === true) return { label: "Đã sửa", badge: "badge-muted" };
  return null;
}
