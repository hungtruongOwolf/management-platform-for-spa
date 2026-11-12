import { ApiError, NETWORK_MESSAGE } from "./api";

/**
 * Vietnamese messages for stable API error codes. Unknown codes fall back to a generic message.
 * Codes only a lazy screen can meet are added by that screen (`addMessages`, e.g. staff/api.ts), so
 * their text stays out of their chunks. This module is not in the startup chunk either: the
 * sign-in page loads it after its first paint (LoginPage), every other screen is a lazy chunk.
 */
const MESSAGES: Record<string, string> = {
  "auth.invalid_credentials": "Số điện thoại hoặc mật khẩu không đúng.",
  "auth.unauthenticated": "Phiên đăng nhập đã hết. Vui lòng đăng nhập lại.",
  "auth.reauth_required": "Vui lòng nhập lại mật khẩu để tiếp tục.",
  "auth.forbidden": "Bạn không có quyền thực hiện thao tác này.",
  "auth.spa_suspended": "Spa đang tạm ngưng. Liên hệ bộ phận hỗ trợ.",
  "auth.bad_origin": "Yêu cầu không hợp lệ. Vui lòng tải lại trang.",
  "feature.disabled": "Tính năng này đang tắt cho spa. Liên hệ bộ phận hỗ trợ.",
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
    "Nhân viên được chọn không còn làm việc ở spa. Vui lòng chọn người khác.",
  "package.no_sessions_left":
    "Liệu trình vừa hết buổi (có thể vừa được dùng ở máy khác). Đã cập nhật số buổi.",
  "package.expired":
    "Liệu trình đã hết hạn. Cần quản lý cho dùng tiếp (ghi lý do).",
  "package.cancelled":
    "Liệu trình đã bị hủy: không trừ, cộng lại buổi hay hủy hóa đơn được nữa.",
  "package.item_not_found":
    "Không tìm thấy liệu trình còn dùng được cho dịch vụ này. Gợi ý bán gói mới.",
  "price.gender_required": "Chọn Nữ hoặc Nam để tính giá dịch vụ lẻ.",
  "price.variant_not_available":
    "Dịch vụ này không áp dụng cho giới tính của khách.",
  "visit.check_in_not_today":
    "Giờ làm phải thuộc ngày hôm nay. Chỉ chủ spa nhập bù được ngày đã qua.",
  "appointment.closed": "Lịch hẹn này đã kết thúc (hoàn tất, hủy hoặc không đến).",
  "appointment.not_today": "Chỉ làm được việc này vào đúng ngày của lịch hẹn.",
  "appointment.customer_required":
    "Khách mới: tạo hồ sơ khách trước rồi mới hoàn tất được.",
  "customer.phone_taken": "Số điện thoại này đã có hồ sơ khách.",
  "customer.name_required":
    "Gói liệu trình cần tên khách. Mở “Sửa hồ sơ”, thêm tên rồi bán gói.",
  "visit.voided": "Lượt này đã bị hủy, không sửa được.",
  "visit.corrected":
    "Lượt này vừa được sửa (có thể ở máy khác). Mở lượt mới nhất để sửa tiếp.",
  "day.locked": "Ngày đã chốt, liên hệ chủ spa.",
  "idempotency.key_reuse_mismatch":
    "Thao tác này có thể đã được lưu ở lần gửi trước. Đóng hộp thoại, tải lại rồi kiểm tra trước khi làm lại.",
};

/** Messages for field-level validation codes every form shares. */
const FIELD_MESSAGES: Record<string, string> = {
  required: "Bắt buộc nhập.",
  too_long: "Quá dài.",
  out_of_range: "Giá trị không hợp lệ.",
  negative: "Không được là số âm.",
  inactive: "Dịch vụ này đang ngừng bán.",
  not_found: "Không còn tồn tại.",
  exceeds_subtotal: "Lớn hơn số tiền phải trả.",
  must_be_positive: "Phải lớn hơn 0.",
  exceeds_total: "Số buổi còn lại lớn hơn tổng số buổi.",
  in_future: "Không được là ngày trong tương lai.",
  before_start: "Trước ngày bắt đầu.",
  invalid_format: "Sai định dạng.",
  invalid: "Giá trị không hợp lệ.",
  in_past: "Không được là ngày đã qua.",
  not_allowed: "Không dùng được ở đây.",
  invalid_time: "Giờ không hợp lệ (dạng 09:00).",
  unknown: "Không tìm thấy nhân viên.",
  percent_or_amount: "Chỉ nhập % hoặc số tiền, không cả hai.",
  count_out_of_range: "Số lượng không hợp lệ.",
  not_today: "Chỉ đổi sang ngày khác khi khách chưa đến.",
  before_from: "Ngày kết thúc trước ngày bắt đầu.",
  not_visit_day: "Giờ làm phải thuộc đúng ngày của lượt.",
  too_old: "Ngày quá xa trong quá khứ.",
  conflicts_with_birth_date: "Năm sinh khác năm của ngày sinh.",
};

/**
 * Field texts of one feature's forms, passed to `fieldMessage` by those forms only. Never
 * registered globally: two lazy screens that give one code different meanings (a field `taken`,
 * `not_closed`…) cannot overwrite each other's text depending on which chunk loaded last.
 */
export type FieldTexts = Readonly<Record<string, string>>;

/**
 * Registers the API error codes of a lazy feature (called once at the top of its module). The
 * codes are namespaced (`staff.account_taken`), so one table serves every screen.
 */
export function addMessages(codes: Record<string, string>): void {
  Object.assign(MESSAGES, codes);
}

/** Retrying cannot help a 403: the role lacks the permission ("Thử lại" is hidden). */
export function retryable(error: unknown): boolean {
  return !(error instanceof ApiError && error.status === 403);
}

export function messageFor(error: unknown): string {
  if (error instanceof ApiError) {
    return MESSAGES[error.code] ?? "Có lỗi xảy ra. Vui lòng thử lại.";
  }
  return NETWORK_MESSAGE;
}

/** The text of the server's refusal of `field`: the form's own `texts` first, then the shared ones. */
export function fieldMessage(
  error: unknown,
  field: string,
  texts?: FieldTexts,
): string | undefined {
  if (!(error instanceof ApiError)) return undefined;
  const code = error.fieldCode(field);
  return code === undefined
    ? undefined
    : (texts?.[code] ?? FIELD_MESSAGES[code] ?? "Giá trị không hợp lệ.");
}
