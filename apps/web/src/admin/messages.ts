import { ApiError } from "../lib/api";
import { addMessages, messageFor } from "../lib/errors";

/** Error messages only the platform console meets: they live in its chunk, not the spa's startup bundle. */
addMessages(
  {
    "platform.step_up_required": "Cần xác nhận lại mật khẩu và mã để làm thao tác này.",
    "tenant.slug_taken": "Mã spa này đã có. Chọn mã khác.",
    "tenant.status_unchanged": "Spa đã ở trạng thái này.",
    "account.phone_taken": "Số điện thoại này đã có tài khoản (mỗi người chỉ thuộc một spa).",
  },
  {},
);

/** The console signs in with a username, so the spa's "phone or password" wording does not fit. */
export function adminMessage(error: unknown): string {
  if (error instanceof ApiError && error.code === "auth.invalid_credentials") {
    return "Tên đăng nhập hoặc mật khẩu không đúng.";
  }
  return messageFor(error);
}

/** Vietnamese for the field codes of a 422 (`errors[].field`). */
const FIELD_LABELS: Record<string, string> = {
  name: "Nhập tên spa.",
  slug: "Mã spa chỉ gồm chữ thường không dấu, số và dấu gạch ngang (tối đa 40).",
  branch_name: "Tên chi nhánh quá dài.",
  "owner.phone": "Số điện thoại di động không hợp lệ.",
  "owner.full_name": "Nhập tên chủ spa.",
  reason: "Nhập lý do (ít nhất 3 ký tự).",
};

export function adminFieldError(error: unknown, field: string): string | undefined {
  if (!(error instanceof ApiError) || error.fieldCode(field) === undefined) return undefined;
  return FIELD_LABELS[field] ?? "Giá trị không hợp lệ.";
}
