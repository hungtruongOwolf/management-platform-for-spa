import { ApiError } from "./api";

/** Vietnamese messages for stable API error codes. Unknown codes fall back to a generic message. */
const MESSAGES: Record<string, string> = {
  "auth.invalid_credentials": "Số điện thoại hoặc mật khẩu không đúng.",
  "auth.unauthenticated": "Phiên đăng nhập đã hết. Vui lòng đăng nhập lại.",
  "auth.reauth_required": "Vui lòng nhập lại mật khẩu để tiếp tục.",
  "auth.forbidden": "Bạn không có quyền thực hiện thao tác này.",
  "auth.bad_origin": "Yêu cầu không hợp lệ. Vui lòng tải lại trang.",
  "conflict.stale_version":
    "Dữ liệu vừa được người khác sửa. Vui lòng tải lại rồi thử lại.",
  not_found: "Không tìm thấy dữ liệu (có thể đã bị xóa hoặc đổi).",
  rate_limited: "Bạn thao tác quá nhanh. Vui lòng đợi một lát rồi thử lại.",
  validation_failed:
    "Thông tin chưa hợp lệ. Vui lòng kiểm tra các ô được đánh dấu.",
  "payment.amount_mismatch": "Số tiền thu chưa khớp với số phải trả.",
  "discount.reason_required": "Giảm giá phải ghi lý do.",
  "discount.limit_exceeded": "Mức giảm vượt quyền của bạn — cần chủ spa duyệt.",
  "staff.inactive":
    "Người tư vấn chốt không còn làm việc ở spa. Vui lòng chọn người khác.",
  "idempotency.key_reuse_mismatch":
    "Hóa đơn này có thể đã được lưu. Vui lòng đóng hộp thoại và kiểm tra lại liệu trình.",
};

/** Messages for field-level validation codes. */
const FIELD_MESSAGES: Record<string, string> = {
  required: "Bắt buộc nhập.",
  taken: "Tên này đã được dùng.",
  too_long: "Quá dài.",
  out_of_range: "Giá trị không hợp lệ.",
  negative: "Không được là số âm.",
  mixed_variants:
    "Chọn một giá chung hoặc giá theo giới tính, không chọn cả hai.",
  duplicate: "Dịch vụ bị trùng trong gói.",
  inactive: "Dịch vụ này đang ngừng bán.",
  not_found: "Không còn tồn tại.",
  exceeds_subtotal: "Lớn hơn số tiền phải trả.",
  must_be_positive: "Phải lớn hơn 0.",
};

export function messageFor(error: unknown): string {
  if (error instanceof ApiError) {
    return MESSAGES[error.code] ?? "Có lỗi xảy ra. Vui lòng thử lại.";
  }
  return "Không kết nối được máy chủ. Vui lòng kiểm tra mạng.";
}

export function fieldMessage(
  error: unknown,
  field: string,
): string | undefined {
  if (!(error instanceof ApiError)) return undefined;
  const code = error.fieldCode(field);
  return code === undefined
    ? undefined
    : (FIELD_MESSAGES[code] ?? "Giá trị không hợp lệ.");
}
