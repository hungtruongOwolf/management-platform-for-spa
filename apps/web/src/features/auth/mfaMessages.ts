import { addMessages } from "../../lib/errors";

// Second-factor messages: only /xac-thuc (a lazy route, owners only) and the platform console's
// sign-in meet them, so they stay out of the startup chunk (lib/errors.ts, perf budget).
addMessages(
  {
    "auth.invalid_mfa_code": "Mã không đúng, đã hết hạn hoặc đã dùng. Đợi mã mới rồi thử lại.",
    "auth.mfa_required": "Vui lòng nhập mã xác thực hai bước.",
    "auth.mfa_enrollment_required": "Vui lòng thiết lập xác thực hai bước.",
    "auth.mfa_already_enrolled": "Tài khoản đã bật xác thực hai bước.",
    "auth.mfa_enrollment_changed": "Mã QR đã được tạo lại ở nơi khác. Vui lòng đăng nhập lại.",
    "auth.mfa_enrollment_not_started": "Vui lòng đăng nhập lại để thiết lập.",
  },
  {},
);
