import { ApiError } from "../../lib/api";
import { addMessages, messageFor, type FieldTexts } from "../../lib/errors";

// Refusals only these screens meet (architecture/lucky-draw.md §3), kept in their lazy chunk.
addMessages({
  "lucky_draw.participant_drawn": "Người này đã có kết quả quay, nên không sửa hay xóa được nữa.",
  "lucky_draw.prize_drawn":
    "Giải này đã có kết quả quay: không xóa được và không đổi được “Dành cho”. Tên, mô tả, số suất vẫn sửa được.",
  "lucky_draw.quantity_below_awarded": "Số suất không được ít hơn số suất đã trao.",
  "lucky_draw.prize_exhausted": "Giải này vừa hết suất. Chọn giải khác rồi bấm “Quay”.",
  "lucky_draw.no_eligible": "Không còn ai phù hợp với giải này. Chọn giải khác rồi bấm “Quay”.",
  "lucky_draw.result_forfeited": "Kết quả này đã được trả giải trước đó.",
  "idempotency.in_progress": "Lượt quay trước còn đang được lưu. Chờ vài giây rồi bấm “Quay” lại.",
  "precondition.if_match_required": "Dữ liệu trên màn đã cũ. Vui lòng tải lại trang rồi thử lại.",
});

/** Field refusals of these forms (`fieldMessage(…, DRAW_FIELDS)`). */
export const DRAW_FIELDS: FieldTexts = {
  required: "Bắt buộc nhập.",
  too_long: "Quá dài.",
  out_of_range: "Ngoài khoảng cho phép.",
  too_many: "Quá số lượng cho phép.",
};

/**
 * A spin that never got a clear answer (no network, a server error) may still have been saved:
 * the next press resends it with the same key (AC4.6).
 */
export function spinUnanswered(error: unknown): boolean {
  return !(error instanceof ApiError) || error.status >= 500 || error.code === "idempotency.in_progress";
}

/** What the result card says when a spin failed. */
export function spinError(error: unknown): string {
  if (error instanceof ApiError && !spinUnanswered(error)) return messageFor(error);
  if (error instanceof ApiError && error.code === "idempotency.in_progress") return messageFor(error);
  return "Chưa quay được, kiểm tra mạng rồi bấm “Quay” lại.";
}
