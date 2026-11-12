/**
 * Vietnamese names for a spa's own audit trail as the console shows it (ADR-0019 D20). The codes
 * are the `action` / `entity_type` the API crates write (`AuditEntry`, 0040's
 * `platform_apply_role_template`); a code added later and not named here shows as "Thao tác khác"
 * with the code beside it, never as a bare code.
 */
const ACTIONS: Record<string, string> = {
  "account.locked_by_platform": "Nền tảng khóa tài khoản",
  "account.unlocked_by_platform": "Nền tảng mở khóa tài khoản",
  "account.mfa_reset_by_platform": "Nền tảng đặt lại xác thực hai bước",
  "account.sessions_revoked_by_platform": "Nền tảng đăng xuất mọi thiết bị",
  "account.password_reset_by_platform": "Nền tảng đặt lại mật khẩu",
  "account.password_change": "Đổi mật khẩu",
  "account.password_reset_requested": "Gửi yêu cầu quên mật khẩu",
  "account.password_reset_dismissed": "Bỏ qua yêu cầu quên mật khẩu",
  "appointment.create": "Đặt lịch hẹn",
  "appointment.update": "Sửa lịch hẹn",
  "appointment.arrive": "Khách đến",
  "appointment.undo_arrival": "Hoàn tác khách đến",
  "appointment.complete": "Xong lịch hẹn",
  "appointment.no_show": "Khách không đến",
  "appointment.cancel": "Hủy lịch hẹn",
  "appointment.settings": "Sửa cài đặt lịch hẹn",
  "attendance.record": "Chấm công",
  "branch.create": "Tạo chi nhánh",
  "catalog.category_create": "Thêm nhóm dịch vụ",
  "catalog.category_update": "Sửa nhóm dịch vụ",
  "catalog.category_delete": "Xóa nhóm dịch vụ",
  "catalog.service_create": "Thêm dịch vụ",
  "catalog.service_update": "Sửa dịch vụ",
  "catalog.plan_create": "Thêm gói liệu trình",
  "catalog.plan_update": "Sửa gói liệu trình",
  "customer.create": "Thêm khách",
  "customer.update": "Sửa hồ sơ khách",
  "customer.health_note_set": "Sửa ghi chú sức khỏe",
  "customer.follow_up": "Ghi liên hệ chăm sóc",
  "customer_package.create": "Bán gói",
  "customer_package.extend": "Gia hạn gói",
  "customer_package.cancel": "Hủy gói",
  "customer_package.use_expired_override": "Dùng gói đã hết hạn",
  "evaluation.record": "Đánh giá tháng",
  "evaluation.daily_review": "Đánh giá ngày",
  "feature.changed": "Bật / tắt tính năng",
  "opening_balance.import": "Nhập số dư sổ cũ",
  "payroll.close": "Chốt lương",
  "payroll.rules_update": "Sửa quy tắc lương",
  "role.permissions_changed": "Sửa quyền vai trò",
  "role.permissions_reset": "Đưa vai trò về mẫu",
  "sale.create": "Thu tiền",
  "sale.void": "Hủy hóa đơn",
  "sale.note_update": "Sửa ghi chú gói",
  "sale.paper_book": "Nhập từ sổ giấy",
  "staff.create": "Thêm nhân viên",
  "staff.update": "Sửa hồ sơ nhân viên",
  "staff.end": "Cho nghỉ việc",
  "staff.rehire": "Nhận lại nhân viên",
  "staff.pay_update": "Sửa lương nhân viên",
  "staff.adjustment": "Thưởng / phạt",
  "staff.adjustment_void": "Hủy thưởng / phạt",
  "staff.account_link": "Gắn tài khoản",
  "staff.login_create": "Tạo tài khoản đăng nhập",
  "staff.login_password_reset": "Đặt lại mật khẩu nhân viên",
  "staff.login_role": "Đổi vai trò tài khoản",
  "tenant.created": "Tạo spa",
  "tenant.suspended": "Tạm ngưng spa",
  "tenant.unsuspended": "Mở lại spa",
  "tenant.owner_mfa_changed": "Nền tảng bật / tắt xác thực 2 bước cho chủ spa",
  "visit.create": "Ghi lượt",
  "visit.addition": "Thêm vào lượt",
  "visit.update": "Sửa lượt",
  "visit.correct": "Sửa lượt",
  "visit.void": "Hủy lượt",
  "visit.convert_to_package": "Chuyển lượt sang gói",
};

const ENTITIES: Record<string, string> = {
  appointment: "Lịch hẹn",
  branch: "Chi nhánh",
  customer: "Khách",
  customer_package: "Gói của khách",
  package_plan: "Gói liệu trình",
  role: "Vai trò",
  sale: "Hóa đơn",
  service: "Dịch vụ",
  service_category: "Nhóm dịch vụ",
  staff: "Nhân viên",
  tenant: "Spa",
  user: "Tài khoản",
  visit: "Lượt",
};

/** The Vietnamese name of a spa audit action; `known` is false for a code not named here. */
export function spaActionLabel(action: string): { text: string; known: boolean } {
  const text = ACTIONS[action];
  return text ? { text, known: true } : { text: "Thao tác khác", known: false };
}

export function spaEntityLabel(entityType: string): string {
  return ENTITIES[entityType] ?? entityType;
}
