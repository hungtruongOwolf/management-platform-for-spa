import { ApiError } from "./api";

/** Vietnamese messages for stable API error codes. Unknown codes fall back to a generic message. */
const MESSAGES: Record<string, string> = {
  "auth.invalid_credentials": "Số điện thoại hoặc mật khẩu không đúng.",
  "auth.unauthenticated": "Phiên đăng nhập đã hết. Vui lòng đăng nhập lại.",
  "auth.reauth_required": "Vui lòng nhập lại mật khẩu để tiếp tục.",
  "auth.forbidden": "Bạn không có quyền thực hiện thao tác này.",
  "auth.bad_origin": "Yêu cầu không hợp lệ. Vui lòng tải lại trang.",
  rate_limited: "Bạn thao tác quá nhanh. Vui lòng đợi một lát rồi thử lại.",
  validation_failed: "Thông tin chưa hợp lệ. Vui lòng kiểm tra lại.",
};

export function messageFor(error: unknown): string {
  if (error instanceof ApiError) {
    return MESSAGES[error.code] ?? "Có lỗi xảy ra. Vui lòng thử lại.";
  }
  return "Không kết nối được máy chủ. Vui lòng kiểm tra mạng.";
}
