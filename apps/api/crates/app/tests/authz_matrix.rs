//! S10: every route declares who may call it, and the server enforces it.
//!
//! `ROUTES` below is the single table of truth for this test and mirrors
//! memory/wiki/security/authz-matrix.md. Adding an endpoint without a row here fails
//! `every_mounted_route_has_a_matrix_row` (the CI gate "every route declares a permission").
//! How to add a row: memory/wiki/conventions/testing.md.

mod common;

use axum::Router;
use axum::http::StatusCode;
use common::{
    Harness, OWNER_PHONE, PASSWORD, Reply, TestResult, add_member, call_with_headers, code_now,
    current_code, forget_used_step, harness,
};
use identity::SystemRole::{self, Manager, Owner, Technician};
use identity::provisioning::create_tenant;
use kernel::Permission as P;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU32, Ordering};
use utoipa::OpenApi;

// ------------------------------------------------------------------------------------ the table

/// Who may call a route.
#[derive(Debug, Clone, Copy)]
enum Access {
    /// No session needed (login, health).
    Public,
    /// Any signed-in user; no tenant permission involved (`CurrentSession`).
    Session,
    /// Only a session that passed the password step and still owes its second factor
    /// (`PendingMfaSession`, S2b). A full session gets 409 `auth.mfa_not_pending`.
    PendingMfa,
    /// Signed in inside a spa and holding this permission (`AuthUser` + `actor.require`).
    /// `allow` lists the default roles that hold it (authz-matrix.md columns).
    Perm(P, &'static [SystemRole]),
    /// A full platform operator session (ADR-0018, cookie `__Host-op-session`); a spa session of
    /// any role is `401`. Writes also need a recent step-up (`403 platform.step_up_required`).
    Operator,
    /// An operator session that passed the password and still owes its second factor.
    OperatorPending,
}

/// What the `{id}` in the path refers to; the test substitutes a real record of that kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    None,
    Category,
    Service,
    Plan,
    Customer,
    Sale,
    CustomerPackage,
    Visit,
    Staff,
    Appointment,
    Adjustment,
    /// An open forgotten-password request (ADR-0016).
    ResetRequest,
    /// The spa itself (platform console routes).
    Tenant,
}

struct Route {
    method: &'static str,
    /// Exactly as in the OpenAPI document.
    path: &'static str,
    access: Access,
    id: Target,
    /// A valid request body for the caller's own tenant (`None` = no body). Must deserialize, or
    /// extraction fails with 4xx before the permission check runs.
    body: fn(&Fixture) -> Option<Value>,
    headers: &'static [(&'static str, &'static str)],
    /// The call revokes the caller's session (logout, switch-tenant): use a throwaway session.
    consumes_session: bool,
    /// The handler requires `Idempotency-Key`; every call sends a fresh one.
    idempotent: bool,
    /// Open to a former staff member (ADR-0014: `staff.read_own_past`, derived, never a role).
    former_staff: bool,
}

const ALL: &[SystemRole] = &[Owner, Manager, Technician];
const OWNER_MANAGER: &[SystemRole] = &[Owner, Manager];
const OWNER_ONLY: &[SystemRole] = &[Owner];
const IF_MATCH: &[(&str, &str)] = &[("If-Match", "\"1\"")];

const fn route(method: &'static str, path: &'static str, access: Access) -> Route {
    Route {
        method,
        path,
        access,
        id: Target::None,
        body: no_body,
        headers: &[],
        consumes_session: false,
        idempotent: false,
        former_staff: false,
    }
}

impl Route {
    const fn id(mut self, id: Target) -> Self {
        self.id = id;
        self
    }
    const fn body(mut self, body: fn(&Fixture) -> Option<Value>) -> Self {
        self.body = body;
        self
    }
    const fn if_match(mut self) -> Self {
        self.headers = IF_MATCH;
        self
    }
    const fn consumes_session(mut self) -> Self {
        self.consumes_session = true;
        self
    }
    const fn idempotent(mut self) -> Self {
        self.idempotent = true;
        self
    }
    const fn former_staff(mut self) -> Self {
        self.former_staff = true;
        self
    }
}

/// One row per (method, path) in the OpenAPI document. Keep in the same order as `openapi.rs`.
const ROUTES: &[Route] = &[
    // auth — identity
    route("POST", "/api/v1/auth/login", Access::Public),
    route("POST", "/api/v1/auth/logout", Access::Session).consumes_session(),
    route("POST", "/api/v1/auth/logout-all", Access::Session).consumes_session(),
    route("POST", "/api/v1/auth/reauth", Access::Session).body(reauth_body),
    route("POST", "/api/v1/auth/switch-tenant", Access::Session)
        .body(switch_body)
        .consumes_session(),
    route("GET", "/api/v1/me", Access::Session),
    route("POST", "/api/v1/auth/mfa/verify", Access::PendingMfa).body(mfa_code_body),
    route("POST", "/api/v1/auth/mfa/enroll/start", Access::PendingMfa),
    route(
        "POST",
        "/api/v1/auth/mfa/enroll/confirm",
        Access::PendingMfa,
    )
    .body(mfa_code_body),
    // ADR-0016: every role changes its own password (a too-short new one stops at 422 after the
    // session and the current password were accepted); the forgotten-password request is public.
    route("POST", "/api/v1/auth/password", Access::Session).body(password_change_body),
    route(
        "POST",
        "/api/v1/auth/password-reset-requests",
        Access::Public,
    )
    .body(reset_request_body),
    // catalog
    route(
        "GET",
        "/api/v1/service-categories",
        Access::Perm(P::CatalogRead, ALL),
    ),
    route("GET", "/api/v1/services", Access::Perm(P::CatalogRead, ALL)),
    route(
        "GET",
        "/api/v1/package-plans",
        Access::Perm(P::CatalogRead, ALL),
    ),
    route(
        "POST",
        "/api/v1/service-categories",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .body(category_body),
    route(
        "PUT",
        "/api/v1/service-categories/{id}",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .id(Target::Category)
    .body(category_update_body),
    route(
        "POST",
        "/api/v1/services",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .body(service_body),
    route(
        "PUT",
        "/api/v1/services/{id}",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .id(Target::Service)
    .body(service_body)
    .if_match(),
    route(
        "POST",
        "/api/v1/package-plans",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .body(plan_body),
    route(
        "PUT",
        "/api/v1/package-plans/{id}",
        Access::Perm(P::CatalogManage, OWNER_ONLY),
    )
    .id(Target::Plan)
    .body(plan_body)
    .if_match(),
    // customers — crm
    route(
        "GET",
        "/api/v1/customers",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    ),
    route(
        "POST",
        "/api/v1/customers",
        Access::Perm(P::CustomerWrite, OWNER_MANAGER),
    )
    .body(customer_body),
    route(
        "GET",
        "/api/v1/customers/{id}",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    )
    .id(Target::Customer),
    route(
        "PUT",
        "/api/v1/customers/{id}",
        Access::Perm(P::CustomerWrite, OWNER_MANAGER),
    )
    .id(Target::Customer)
    .body(customer_body)
    .if_match(),
    route(
        "PUT",
        "/api/v1/customers/{id}/health-note",
        Access::Perm(P::CustomerHealthNotesWrite, OWNER_MANAGER),
    )
    .id(Target::Customer)
    .body(health_note_body)
    .if_match(),
    route(
        "GET",
        "/api/v1/customers/phone-check",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    ),
    route(
        "POST",
        "/api/v1/customers/{id}/follow-ups",
        Access::Perm(P::CustomerFollowUp, OWNER_MANAGER),
    )
    .id(Target::Customer)
    .body(follow_up_body)
    .idempotent(),
    route(
        "GET",
        "/api/v1/customers/{id}/follow-ups",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    )
    .id(Target::Customer),
    // staff — org
    route("GET", "/api/v1/staff", Access::Perm(P::StaffReadBasic, ALL)),
    route(
        "POST",
        "/api/v1/staff",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .body(staff_body),
    // Roster read (0025): owner + manager; details only with staff.manage (staff.rs).
    route(
        "GET",
        "/api/v1/staff/profiles",
        Access::Perm(P::StaffRead, OWNER_MANAGER),
    ),
    route(
        "GET",
        "/api/v1/staff/accounts",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    ),
    route(
        "GET",
        "/api/v1/staff/{id}",
        Access::Perm(P::StaffRead, OWNER_MANAGER),
    )
    .id(Target::Staff),
    route(
        "GET",
        "/api/v1/staff/{id}/history",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .id(Target::Staff),
    // ADR-0016, owner only (+ re-auth, fresh in this test). The fixture technician has no login
    // and keeps none (create stops at 412 or 422), so reset / role stop at 409 staff.no_login.
    route(
        "POST",
        "/api/v1/staff/{id}/login",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .id(Target::Staff)
    .body(login_body)
    .if_match(),
    route(
        "POST",
        "/api/v1/staff/{id}/login/reset-password",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .id(Target::Staff),
    route(
        "PUT",
        "/api/v1/staff/{id}/login/role",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .id(Target::Staff)
    .body(login_role_body),
    route(
        "GET",
        "/api/v1/staff/password-reset-requests",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    ),
    route(
        "POST",
        "/api/v1/staff/password-reset-requests/{id}/dismiss",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .id(Target::ResetRequest),
    route(
        "PUT",
        "/api/v1/staff/{id}/account",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .id(Target::Staff)
    .body(account_body)
    .if_match(),
    // Empty reason: the owner's call stops at 422 after the permission and tenant checks, so
    // the fixture technician keeps working for the other rows.
    route(
        "POST",
        "/api/v1/staff/{id}/end",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .id(Target::Staff)
    .body(end_body)
    .if_match(),
    // The fixture technician is working: the owner's call is 409 staff.already_active.
    route(
        "POST",
        "/api/v1/staff/{id}/rehire",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .id(Target::Staff)
    .body(rehire_body)
    .if_match(),
    route(
        "PUT",
        "/api/v1/staff/{id}",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .id(Target::Staff)
    .body(staff_body)
    .if_match(),
    // Also needs payroll.read (same default holder).
    route(
        "PUT",
        "/api/v1/staff/{id}/pay",
        Access::Perm(P::StaffManage, OWNER_ONLY),
    )
    .id(Target::Staff)
    .body(pay_body),
    // staff management — hr
    route(
        "GET",
        "/api/v1/attendance",
        Access::Perm(P::AttendanceRecord, OWNER_MANAGER),
    ),
    route(
        "PUT",
        "/api/v1/attendance",
        Access::Perm(P::AttendanceRecord, OWNER_MANAGER),
    )
    .body(attendance_body),
    route(
        "GET",
        "/api/v1/daily-reviews",
        Access::Perm(P::EvaluationRecord, OWNER_MANAGER),
    ),
    route(
        "PUT",
        "/api/v1/daily-reviews",
        Access::Perm(P::EvaluationRecord, OWNER_MANAGER),
    )
    .body(daily_review_body),
    route(
        "GET",
        "/api/v1/evaluations",
        Access::Perm(P::EvaluationRecord, OWNER_MANAGER),
    ),
    // One person's months (EM 2026-11-08): the rule of GET /evaluations. Without `staff_id` an
    // entitled caller gets 422; `tests/staff.rs` covers ids, other spas and every role.
    route(
        "GET",
        "/api/v1/evaluations/history",
        Access::Perm(P::EvaluationRecord, OWNER_MANAGER),
    ),
    route(
        "PUT",
        "/api/v1/evaluations",
        Access::Perm(P::EvaluationRecord, OWNER_MANAGER),
    )
    .body(evaluation_body),
    route(
        "GET",
        "/api/v1/adjustments",
        Access::Perm(P::EvaluationRecord, OWNER_MANAGER),
    ),
    route(
        "POST",
        "/api/v1/adjustments",
        Access::Perm(P::EvaluationRecord, OWNER_MANAGER),
    )
    .body(adjustment_body)
    .idempotent(),
    route(
        "PUT",
        "/api/v1/adjustments/{id}",
        Access::Perm(P::EvaluationRecord, OWNER_MANAGER),
    )
    .id(Target::Adjustment)
    .body(adjustment_body)
    .idempotent(),
    route(
        "DELETE",
        "/api/v1/adjustments/{id}",
        Access::Perm(P::EvaluationRecord, OWNER_MANAGER),
    )
    .id(Target::Adjustment),
    route(
        "GET",
        "/api/v1/payroll",
        Access::Perm(P::PayrollRead, OWNER_ONLY),
    ),
    route(
        "GET",
        "/api/v1/payroll/payslips/{id}",
        Access::Perm(P::PayrollRead, OWNER_ONLY),
    )
    .id(Target::Staff),
    // Last month still has unrecorded days in the fixture: the owner gets 422, never a close.
    route(
        "POST",
        "/api/v1/payroll/close",
        Access::Perm(P::PayrollClose, OWNER_ONLY),
    )
    .body(payroll_close_body)
    .idempotent(),
    route(
        "GET",
        "/api/v1/payroll/rules",
        Access::Perm(P::PayrollRead, OWNER_ONLY),
    ),
    route(
        "PUT",
        "/api/v1/payroll/rules",
        Access::Perm(P::PayrollClose, OWNER_ONLY),
    )
    .body(payroll_rules_body),
    route(
        "GET",
        "/api/v1/me/staff",
        Access::Perm(P::StaffReadOwn, ALL),
    )
    .former_staff(),
    route(
        "GET",
        "/api/v1/me/staff/day",
        Access::Perm(P::StaffReadOwn, ALL),
    )
    .former_staff(),
    // My own evaluation history (EM 2026-11-08): the session's staff link, former staff included
    // (only the months they worked; tests/evaluation_history.rs).
    route(
        "GET",
        "/api/v1/me/evaluations/history",
        Access::Perm(P::StaffReadOwn, ALL),
    )
    .former_staff(),
    route(
        "GET",
        "/api/v1/me/appointments",
        Access::Perm(P::StaffReadOwn, ALL),
    ),
    // packages
    route(
        "GET",
        "/api/v1/customer-packages/{id}",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    )
    .id(Target::CustomerPackage),
    route(
        "GET",
        "/api/v1/customers/{id}/packages",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    )
    .id(Target::Customer),
    route(
        "POST",
        "/api/v1/opening-balances",
        // Operator migration since 0013; the transitional route is owner-only.
        Access::Perm(P::OpeningBalanceImport, OWNER_ONLY),
    )
    .body(opening_balance_body)
    .idempotent(),
    route(
        "POST",
        "/api/v1/customer-packages/{id}/extend",
        Access::Perm(P::PackageExtendExpiry, OWNER_ONLY),
    )
    .id(Target::CustomerPackage)
    .body(extend_body)
    .if_match()
    .idempotent(),
    // 🔒: the owner's fresh login is within the re-auth window; staleness is tested in voids.rs.
    route(
        "POST",
        "/api/v1/customer-packages/{id}/cancel",
        Access::Perm(P::PackageCancelRefund, OWNER_ONLY),
    )
    .id(Target::CustomerPackage)
    .body(cancel_body)
    .idempotent(),
    // sales
    // F6: the day list's package sales also need customer.read (same holders by default).
    route(
        "GET",
        "/api/v1/sales",
        Access::Perm(P::SaleRead, OWNER_MANAGER),
    ),
    route(
        "POST",
        "/api/v1/sales",
        Access::Perm(P::PackageSell, OWNER_MANAGER),
    )
    .body(sell_body)
    .idempotent(),
    route(
        "POST",
        "/api/v1/sales/checkout",
        Access::Perm(P::PackageSell, OWNER_MANAGER),
    )
    .body(checkout_body)
    .idempotent(),
    route(
        "GET",
        "/api/v1/sales/{id}",
        Access::Perm(P::SaleRead, OWNER_MANAGER),
    )
    .id(Target::Sale),
    // F4 "Sửa ghi chú gói": package.sell + customer.health_notes.write (same holders by default).
    route(
        "PATCH",
        "/api/v1/sales/{id}",
        Access::Perm(P::CustomerHealthNotesWrite, OWNER_MANAGER),
    )
    .id(Target::Sale)
    .body(sale_note_body),
    route(
        "POST",
        "/api/v1/sales/{id}/void",
        Access::Perm(P::RecordVoidSameDay, OWNER_MANAGER),
    )
    .id(Target::Sale)
    .body(void_body)
    .idempotent(),
    // visits — operations
    route(
        "POST",
        "/api/v1/visits",
        Access::Perm(P::VisitRecord, OWNER_MANAGER),
    )
    .body(visit_body)
    .idempotent(),
    route(
        "GET",
        "/api/v1/visits",
        Access::Perm(P::VisitRead, OWNER_MANAGER),
    ),
    route(
        "GET",
        "/api/v1/visits/{id}",
        Access::Perm(P::VisitRead, OWNER_MANAGER),
    )
    .id(Target::Visit),
    route(
        "PATCH",
        "/api/v1/visits/{id}",
        Access::Perm(P::VisitRecord, OWNER_MANAGER),
    )
    .id(Target::Visit)
    .body(visit_update_body)
    .if_match(),
    route(
        "POST",
        "/api/v1/visits/{id}/void",
        Access::Perm(P::RecordVoidSameDay, OWNER_MANAGER),
    )
    .id(Target::Visit)
    .body(void_body)
    .idempotent(),
    // "Sửa lượt" (US6b): also needs `visit.record`, checked first (technicians lack both).
    route(
        "POST",
        "/api/v1/visits/{id}/correct",
        Access::Perm(P::RecordVoidSameDay, OWNER_MANAGER),
    )
    .id(Target::Visit)
    .body(correction_body)
    .if_match()
    .idempotent(),
    // Round 6 (spec §10): "Thêm dịch vụ vào lượt" (US12) and "Tính buổi hôm nay vào gói" (US11).
    route(
        "POST",
        "/api/v1/visits/{id}/additions",
        Access::Perm(P::VisitRecord, OWNER_MANAGER),
    )
    .id(Target::Visit)
    .body(addition_body)
    .idempotent(),
    // Also needs `visit.record` and `package.sell`, checked first (technicians lack all three).
    route(
        "POST",
        "/api/v1/visits/{id}/convert-to-package",
        Access::Perm(P::RecordVoidSameDay, OWNER_MANAGER),
    )
    .id(Target::Visit)
    .body(conversion_body)
    .if_match()
    .idempotent(),
    // appointments — operations (spec appointments.md)
    route(
        "GET",
        "/api/v1/appointments",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    ),
    route(
        "POST",
        "/api/v1/appointments",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    )
    .body(appointment_body)
    .idempotent(),
    route(
        "POST",
        "/api/v1/appointments/group",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    )
    .body(appointment_group_body)
    .idempotent(),
    // The advisory "max customers per slot" (round 5): owner only.
    route(
        "PUT",
        "/api/v1/appointment-settings",
        Access::Perm(P::AppointmentSettings, OWNER_ONLY),
    )
    .body(appointment_settings_body),
    route(
        "GET",
        "/api/v1/appointments/{id}",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    )
    .id(Target::Appointment),
    route(
        "PATCH",
        "/api/v1/appointments/{id}",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    )
    .id(Target::Appointment)
    .body(visit_update_body)
    .if_match(),
    route(
        "POST",
        "/api/v1/appointments/{id}/arrive",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    )
    .id(Target::Appointment)
    .body(arrive_body),
    route(
        "POST",
        "/api/v1/appointments/{id}/undo-arrival",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    )
    .id(Target::Appointment),
    route(
        "POST",
        "/api/v1/appointments/{id}/move-group",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    )
    .id(Target::Appointment)
    .body(move_group_body)
    .if_match(),
    // Also needs visit.record (same default holders).
    route(
        "POST",
        "/api/v1/appointments/{id}/complete",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    )
    .id(Target::Appointment)
    .body(complete_body)
    .idempotent(),
    route(
        "POST",
        "/api/v1/appointments/{id}/cancel",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    )
    .id(Target::Appointment)
    .body(void_body),
    route(
        "POST",
        "/api/v1/appointments/{id}/no-show",
        Access::Perm(P::AppointmentManage, OWNER_MANAGER),
    )
    .id(Target::Appointment)
    .body(void_body),
    // management views — reporting (read-only)
    route(
        "GET",
        "/api/v1/reports/daily",
        Access::Perm(P::ReportRead, OWNER_MANAGER),
    ),
    route(
        "GET",
        "/api/v1/reports/range",
        Access::Perm(P::ReportRead, OWNER_MANAGER),
    ),
    route(
        "GET",
        "/api/v1/reports/ledger",
        Access::Perm(P::ReportRevenueRead, OWNER_MANAGER),
    ),
    // "Nhật ký ghi chú": report.read gates the screen; each note kind keeps its own rule
    // (tests/notes.rs).
    route(
        "GET",
        "/api/v1/notes",
        Access::Perm(P::ReportRead, OWNER_MANAGER),
    ),
    route(
        "GET",
        "/api/v1/customers/inactive",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    ),
    // Also needs visit.read + sale.read (same default holders).
    route(
        "GET",
        "/api/v1/customers/{id}/history",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    )
    .id(Target::Customer),
    route(
        "GET",
        "/api/v1/customers/{id}/summary",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    )
    .id(Target::Customer),
    route(
        "GET",
        "/api/v1/customer-packages/{id}/usage",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    )
    .id(Target::CustomerPackage),
    route(
        "GET",
        "/api/v1/packages",
        Access::Perm(P::CustomerRead, OWNER_MANAGER),
    ),
    // platform admin console (ADR-0018): operators only, never a spa session
    route("POST", "/api/v1/admin/auth/login", Access::Public).body(operator_login_body),
    route("POST", "/api/v1/admin/auth/logout", Access::Operator).consumes_session(),
    route("POST", "/api/v1/admin/auth/reauth", Access::Operator).body(operator_reauth_body),
    route(
        "POST",
        "/api/v1/admin/auth/mfa/verify",
        Access::OperatorPending,
    )
    .body(mfa_code_body),
    route(
        "POST",
        "/api/v1/admin/auth/mfa/enroll/start",
        Access::OperatorPending,
    ),
    route(
        "POST",
        "/api/v1/admin/auth/mfa/enroll/confirm",
        Access::OperatorPending,
    )
    .body(mfa_code_body),
    route("GET", "/api/v1/admin/me", Access::Operator),
    route("GET", "/api/v1/admin/tenants", Access::Operator),
    route("POST", "/api/v1/admin/tenants", Access::Operator).body(new_tenant_body),
    route("GET", "/api/v1/admin/tenants/{id}", Access::Operator).id(Target::Tenant),
    route(
        "POST",
        "/api/v1/admin/tenants/{id}/suspend",
        Access::Operator,
    )
    .id(Target::Tenant)
    .body(reason_body),
    route(
        "POST",
        "/api/v1/admin/tenants/{id}/unsuspend",
        Access::Operator,
    )
    .id(Target::Tenant)
    .body(reason_body),
    route("GET", "/api/v1/admin/audit", Access::Operator),
];

/// Mounted but deliberately left out of the OpenAPI contract. Must answer without a session.
const PUBLIC_UNDOCUMENTED: &[(&str, &str)] = &[("GET", "/api/v1/health")];

// ----------------------------------------------------------------------------------- bodies

fn no_body(_: &Fixture) -> Option<Value> {
    None
}

fn operator_login_body(_: &Fixture) -> Option<Value> {
    Some(json!({"username": common::OPERATOR, "password": "not-the-password"}))
}

fn operator_reauth_body(_: &Fixture) -> Option<Value> {
    Some(json!({"password": "not-the-password", "code": "000000"}))
}

/// A fresh slug and owner phone per call, so the operator's call succeeds.
fn new_tenant_body(_: &Fixture) -> Option<Value> {
    static N: AtomicU32 = AtomicU32::new(1);
    let n = N.fetch_add(1, Ordering::Relaxed);
    Some(json!({
        "name": "Spa mới",
        "slug": format!("spa-moi-{n}"),
        "owner": {"phone": format!("0908{n:06}"), "full_name": "Chủ spa mới"},
    }))
}

fn reason_body(_: &Fixture) -> Option<Value> {
    Some(json!({"reason": "Kiểm thử ma trận quyền"}))
}
fn reauth_body(_: &Fixture) -> Option<Value> {
    Some(json!({"password": PASSWORD}))
}
fn mfa_code_body(_: &Fixture) -> Option<Value> {
    Some(json!({"code": "000000"}))
}

fn switch_body(f: &Fixture) -> Option<Value> {
    Some(json!({"tenant_id": f.tenant_id}))
}
fn category_body(_: &Fixture) -> Option<Value> {
    Some(json!({"name": "Nhóm kiểm thử"}))
}
fn category_update_body(_: &Fixture) -> Option<Value> {
    Some(json!({"name": "Nhóm kiểm thử (sửa)", "sort_order": 1}))
}
fn service_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "category_id": f.category, "name": "Gội đầu dưỡng sinh", "description": null,
        "duration_min": 60, "recommended": false, "active": true, "prices": {"all": 150_000}
    }))
}
fn plan_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "name": "Gói 10 buổi", "price": 1_200_000, "validity_months": 6, "gift_note": null,
        "active": true, "items": [{"service_id": f.service, "sessions": 10, "bonus_sessions": 0}]
    }))
}
fn customer_body(f: &Fixture) -> Option<Value> {
    Some(
        json!({"full_name": "Trần Thị Lan", "phone": f.customer_phone, "gender": "female",
                "birth_year": null, "address": null, "source": null, "note": null}),
    )
}
fn health_note_body(_: &Fixture) -> Option<Value> {
    Some(json!({"health_note": "Dị ứng tinh dầu sả"}))
}
fn staff_body(_: &Fixture) -> Option<Value> {
    Some(json!({"full_name": "KTV Hoa", "position": "Kỹ thuật viên"}))
}
fn password_change_body(_: &Fixture) -> Option<Value> {
    Some(json!({"current_password": PASSWORD, "new_password": "ngan"}))
}
fn reset_request_body(_: &Fixture) -> Option<Value> {
    Some(json!({"phone": "0905999888"}))
}
fn login_body(_: &Fixture) -> Option<Value> {
    // An invalid phone: the owner's call stops at 422 after the permission and tenant checks, so the
    // fixture technician never gets a login (later rows would then see a manager).
    Some(json!({"phone": "12345", "role": "technician"}))
}
fn login_role_body(_: &Fixture) -> Option<Value> {
    Some(json!({"role": "manager"}))
}
fn account_body(_: &Fixture) -> Option<Value> {
    Some(json!({"user_id": null}))
}
fn end_body(_: &Fixture) -> Option<Value> {
    Some(json!({"ended_on": yesterday(), "reason": " "}))
}
fn rehire_body(_: &Fixture) -> Option<Value> {
    Some(json!({"started_on": yesterday()}))
}
fn pay_body(_: &Fixture) -> Option<Value> {
    Some(json!({"base_salary": 5_000_000, "commission_rate_bp": 500}))
}
/// Yesterday in the spa's time zone (UTC+7): recorded days are never in the future.
fn yesterday() -> String {
    (chrono::Utc::now() + chrono::Duration::hours(7) - chrono::Duration::days(1))
        .date_naive()
        .to_string()
}
fn attendance_body(f: &Fixture) -> Option<Value> {
    Some(
        json!({"entries": [{"staff_id": f.staff, "date": yesterday(), "status": "present",
                              "check_in": "09:05"}]}),
    )
}
fn daily_review_body(f: &Fixture) -> Option<Value> {
    Some(json!({"entries": [{"staff_id": f.staff, "date": yesterday(), "compliments": 1}]}))
}
fn evaluation_body(f: &Fixture) -> Option<Value> {
    let criteria: Vec<Value> = [
        "technique",
        "attitude",
        "customer_feedback",
        "diligence",
        "hygiene",
        "learning",
    ]
    .iter()
    .map(|k| json!({"key": k, "points": 10}))
    .collect();
    Some(
        json!({"staff_id": f.staff, "month": &yesterday()[..7], "criteria": criteria,
                "coaching_plan": "Kèm cặp kỹ thuật"}),
    )
}
fn adjustment_body(f: &Fixture) -> Option<Value> {
    Some(json!({"staff_id": f.staff, "kind": "bonus", "amount": 100_000, "reason": "Khách khen"}))
}
fn last_month() -> String {
    let today = (chrono::Utc::now() + chrono::Duration::hours(7)).date_naive();
    let first = today - chrono::Duration::days(i64::from(chrono::Datelike::day(&today)));
    first.format("%Y-%m").to_string()
}
fn payroll_close_body(_: &Fixture) -> Option<Value> {
    Some(json!({"month": last_month()}))
}
fn payroll_rules_body(_: &Fixture) -> Option<Value> {
    Some(json!({"late_grace_minutes": 5}))
}
fn follow_up_body(_: &Fixture) -> Option<Value> {
    Some(json!({"channel": "phone", "outcome": "no_answer"}))
}
/// A one-off line (flat 150.000đ) paid in cash: repeatable without using up package sessions.
fn visit_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "customer_id": f.customer,
        "lines": [{"service_id": f.service, "source": "one_off", "staff_id": f.staff}],
        "payments": [{"method": "cash", "amount": 150_000}]
    }))
}
/// Tomorrow 10:00 in the spa (always inside the default 09:00–21:00 hours, whenever the test runs).
fn tomorrow_at_ten() -> String {
    let vn = chrono::FixedOffset::east_opt(7 * 3600).unwrap_or_else(|| panic!("offset"));
    let day = chrono::Utc::now().with_timezone(&vn).date_naive() + chrono::Duration::days(1);
    format!("{day}T10:00:00+07:00")
}
fn appointment_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "customer_id": f.customer, "service_ids": [f.service], "staff_id": f.staff,
        "starts_at": tomorrow_at_ten(),
    }))
}
fn appointment_group_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "starts_at": tomorrow_at_ten(),
        "people": [
            {"customer_id": f.customer, "service_ids": [f.service]},
            {"guest": {"full_name": "Bé Na", "phone": "0905777888"}, "service_ids": [f.service]},
        ],
    }))
}
fn appointment_settings_body(_: &Fixture) -> Option<Value> {
    Some(json!({"max_per_slot": 4}))
}
fn move_group_body(_: &Fixture) -> Option<Value> {
    Some(json!({"starts_at": tomorrow_at_ten()}))
}
fn arrive_body(f: &Fixture) -> Option<Value> {
    Some(json!({"staff_id": f.staff}))
}
fn complete_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "lines": [{"service_id": f.service, "source": "one_off", "staff_id": f.staff}],
        "payments": [{"method": "cash", "amount": 150_000}]
    }))
}
fn visit_update_body(_: &Fixture) -> Option<Value> {
    Some(json!({}))
}
fn correction_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "reason": "Chọn nhầm dịch vụ",
        "customer_id": f.customer,
        "lines": [{"service_id": f.service, "source": "one_off", "staff_id": f.staff}],
        "payments": [{"method": "cash", "amount": 150_000}]
    }))
}
fn addition_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "lines": [{"service_id": f.service, "source": "one_off", "staff_id": f.staff}],
        "payments": [{"method": "cash", "amount": 150_000}]
    }))
}
fn conversion_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "package_sale": {"package_plan_id": f.plan, "closer_staff_id": f.staff},
        "payments": [{"method": "cash", "amount": 1_000_000}]
    }))
}
fn void_body(_: &Fixture) -> Option<Value> {
    Some(json!({"reason": "Ghi nhầm"}))
}
fn extend_body(_: &Fixture) -> Option<Value> {
    Some(json!({"expires_on": "2099-12-31", "reason": "Khách ốm"}))
}
fn cancel_body(_: &Fixture) -> Option<Value> {
    Some(json!({"reason": "Khách chuyển nhà", "refund_amount": 100_000, "refund_method": "cash"}))
}
fn sell_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "customer_id": f.customer, "closer_staff_id": f.staff,
        "lines": [{"package_plan_id": f.plan}],
        "payments": [{"method": "cash", "amount": 1_200_000}]
    }))
}

fn sale_note_body(_: &Fixture) -> Option<Value> {
    Some(json!({"note": "Khách đau vai gáy"}))
}

fn checkout_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "customer_id": f.customer,
        "package_sales": [{"package_plan_id": f.plan, "closer_staff_id": f.staff}],
        "payments": [{"method": "cash", "amount": 1_200_000}]
    }))
}

fn opening_balance_body(f: &Fixture) -> Option<Value> {
    Some(json!({
        "customer_id": f.customer,
        "packages": [{
            "name": "Gói 10 buổi (sổ cũ)", "sold_price": 1_200_000, "starts_on": "2026-06-01",
            "items": [{"service_id": f.service, "sessions_purchased": 10, "sessions_remaining": 4}]
        }]
    }))
}

// --------------------------------------------------------------------------------- fixtures

/// One spa with one record of every kind a `{id}` can point at.
struct Fixture {
    tenant_id: String,
    category: String,
    service: String,
    plan: String,
    customer: String,
    customer_phone: &'static str,
    staff: String,
    sale: String,
    customer_package: String,
    visit: String,
    appointment: String,
    adjustment: String,
    reset_request: String,
}

impl Fixture {
    fn id_of(&self, target: Target) -> &str {
        match target {
            Target::None => "",
            Target::Category => &self.category,
            Target::Service => &self.service,
            Target::Plan => &self.plan,
            Target::Customer => &self.customer,
            Target::Sale => &self.sale,
            Target::CustomerPackage => &self.customer_package,
            Target::Visit => &self.visit,
            Target::Staff => &self.staff,
            Target::Appointment => &self.appointment,
            Target::Adjustment => &self.adjustment,
            Target::ResetRequest => &self.reset_request,
            Target::Tenant => &self.tenant_id,
        }
    }
}

const MANAGER_PHONE: &str = "0905000002";
const TECHNICIAN_PHONE: &str = "0905000003";
const OTHER_OWNER_PHONE: &str = "0906000001";

fn phone_of(role: SystemRole) -> &'static str {
    match role {
        Owner => OWNER_PHONE,
        Manager => MANAGER_PHONE,
        Technician => TECHNICIAN_PHONE,
    }
}

/// Every request comes from its own client IP so per-IP rate limits never mask a result.
fn next_ip() -> String {
    static N: AtomicU32 = AtomicU32::new(1);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("10.{}.{}.{}", (n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff)
}

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    session: Option<&str>,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> Reply {
    let ip = next_ip();
    let mut all: Vec<(&str, &str)> = vec![("X-Forwarded-For", &ip)];
    all.extend_from_slice(headers);
    let path = path.strip_prefix("/api/v1").unwrap_or(path);
    call_with_headers(app, method, path, session, body, &all).await
}

/// Sends `route` as written in the table: its headers, plus a fresh Idempotency-Key if it needs one.
async fn send_route(
    app: &Router,
    route: &Route,
    path: &str,
    session: Option<&str>,
    body: Option<Value>,
) -> Reply {
    static KEY: AtomicU32 = AtomicU32::new(1);
    let key = format!(
        "00000000-0000-4000-8000-{:012x}",
        KEY.fetch_add(1, Ordering::Relaxed)
    );
    let mut headers = route.headers.to_vec();
    if route.idempotent {
        headers.push(("Idempotency-Key", &key));
    }
    send(app, route.method, path, session, body, &headers).await
}

/// Password step only. For owners this is a pending-MFA session.
async fn password_session(app: &Router, phone: &str) -> Result<(String, Reply), String> {
    let ip = next_ip();
    let reply = call_with_headers(
        app,
        "POST",
        "/auth/login",
        None,
        Some(json!({"phone": phone, "password": PASSWORD})),
        &[("X-Forwarded-For", &ip)],
    )
    .await;
    let token = reply
        .session()
        .ok_or_else(|| format!("login {phone} → {} {}", reply.status, reply.body))?;
    Ok((token, reply))
}

/// A full session: owners enroll on their first login and enter a current code afterwards.
async fn session(h: &Harness, phone: &str) -> Result<String, String> {
    let (pending, reply) = password_session(&h.app, phone).await?;
    let done = match reply.body["next"].as_str() {
        Some("mfa_enroll") => {
            let start = send(
                &h.app,
                "POST",
                "/auth/mfa/enroll/start",
                Some(&pending),
                None,
                &[],
            )
            .await;
            let key = start.body["manual_key"].as_str().unwrap_or_default();
            let body = json!({"code": code_now(key)});
            send(
                &h.app,
                "POST",
                "/auth/mfa/enroll/confirm",
                Some(&pending),
                Some(body),
                &[],
            )
            .await
        }
        Some("mfa_verify") => {
            // Many owner logins per test fall inside one 30 s step; replay is tested in mfa.rs.
            forget_used_step(h, phone)
                .await
                .map_err(|e| e.to_string())?;
            let code = current_code(h, phone).await.map_err(|e| e.to_string())?;
            let body = json!({"code": code});
            send(
                &h.app,
                "POST",
                "/auth/mfa/verify",
                Some(&pending),
                Some(body),
                &[],
            )
            .await
        }
        _ => return Ok(pending),
    };
    done.session()
        .ok_or_else(|| format!("MFA for {phone} → {} {}", done.status, done.body))
}

async fn created_id(app: &Router, owner: &str, path: &str, body: Value) -> Result<String, String> {
    let reply = send(app, "POST", path, Some(owner), Some(body), &[]).await;
    reply.body["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("fixture POST {path} → {} {}", reply.status, reply.body))
}

async fn fixture(
    h: &Harness,
    slug: &str,
    owner_phone: &str,
    customer_phone: &'static str,
) -> Result<Fixture, Box<dyn std::error::Error>> {
    let (app, db) = (&h.app, &h.owner);
    let owner = session(h, owner_phone).await?;
    let tenant_id: String = sqlx::query_scalar("SELECT id::text FROM app.tenant WHERE slug = $1")
        .bind(slug)
        .fetch_one(db)
        .await?;
    let category = created_id(
        app,
        &owner,
        "/service-categories",
        json!({"name": "Dưỡng sinh"}),
    )
    .await?;
    let mut f = Fixture {
        tenant_id,
        category,
        service: String::new(),
        plan: String::new(),
        customer: String::new(),
        customer_phone,
        staff: org::provisioning::add_staff(db, slug, "KTV Lan", "Kỹ thuật viên")
            .await?
            .to_string(),
        sale: String::new(),
        customer_package: String::new(),
        visit: String::new(),
        appointment: String::new(),
        adjustment: String::new(),
        reset_request: String::new(),
    };
    f.service = created_id(
        app,
        &owner,
        "/services",
        service_body(&f).unwrap_or_default(),
    )
    .await?;
    f.plan = created_id(
        app,
        &owner,
        "/package-plans",
        plan_body(&f).unwrap_or_default(),
    )
    .await?;
    f.customer = created_id(
        app,
        &owner,
        "/customers",
        customer_body(&f).unwrap_or_default(),
    )
    .await?;
    let sold = send(
        app,
        "POST",
        "/sales",
        Some(&owner),
        sell_body(&f),
        &[("Idempotency-Key", &uuid::Uuid::now_v7().to_string())],
    )
    .await;
    let (Some(sale), Some(package)) = (
        sold.body["id"].as_str(),
        sold.body["customer_packages"][0]["id"].as_str(),
    ) else {
        return Err(format!("fixture POST /sales → {} {}", sold.status, sold.body).into());
    };
    f.sale = sale.to_owned();
    f.customer_package = package.to_owned();
    let visit = send(
        app,
        "POST",
        "/visits",
        Some(&owner),
        visit_body(&f),
        &[("Idempotency-Key", &uuid::Uuid::now_v7().to_string())],
    )
    .await;
    f.visit = visit.body["id"]
        .as_str()
        .ok_or_else(|| format!("fixture POST /visits → {} {}", visit.status, visit.body))?
        .to_owned();
    let booked = send(
        app,
        "POST",
        "/appointments",
        Some(&owner),
        appointment_body(&f),
        &[("Idempotency-Key", &uuid::Uuid::now_v7().to_string())],
    )
    .await;
    f.appointment = booked.body["id"]
        .as_str()
        .ok_or_else(|| {
            format!(
                "fixture POST /appointments → {} {}",
                booked.status, booked.body
            )
        })?
        .to_owned();
    let adjusted = send(
        app,
        "POST",
        "/adjustments",
        Some(&owner),
        adjustment_body(&f),
        &[("Idempotency-Key", &uuid::Uuid::now_v7().to_string())],
    )
    .await;
    f.adjustment = adjusted.body["id"]
        .as_str()
        .ok_or_else(|| {
            format!(
                "fixture POST /adjustments → {} {}",
                adjusted.status, adjusted.body
            )
        })?
        .to_owned();
    // A forgotten-password request of the spa (written as the public route would).
    f.reset_request = sqlx::query_scalar(
        "INSERT INTO app.password_reset_request (id, tenant_id, user_id, expires_at)
         SELECT $1, t.id, u.id, now() + interval '24 hours'
         FROM app.tenant t, app.user_account u WHERE t.slug = $2 AND u.phone_e164 = $3
         RETURNING id::text",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(slug)
    .bind(format!("+84{}", &owner_phone[1..]))
    .fetch_one(db)
    .await?;
    Ok(f)
}

struct World {
    app: Router,
    h: Harness,
    home: Fixture,
    other: Fixture,
}

/// Spa `vpyd` (owner, manager, technician) and spa `other` (its own owner), each with fixtures.
async fn world(pool: PgPool) -> Result<World, Box<dyn std::error::Error>> {
    let h = harness(pool).await?;
    add_member(&h.owner, "vpyd", MANAGER_PHONE, "Quản lý", Manager).await?;
    add_member(&h.owner, "vpyd", TECHNICIAN_PHONE, "KTV", Technician).await?;
    create_tenant(&h.owner, "other", "Spa khác", "Huế").await?;
    add_member(&h.owner, "other", OTHER_OWNER_PHONE, "Chủ spa khác", Owner).await?;
    let home = fixture(&h, "vpyd", OWNER_PHONE, "0905 777 001").await?;
    let other = fixture(&h, "other", OTHER_OWNER_PHONE, "0905 777 002").await?;
    Ok(World {
        app: h.app.clone(),
        h,
        home,
        other,
    })
}

/// Links the manager's login of spa `vpyd` to a staff profile of its own.
async fn link_manager(w: &World) -> Result<(), Box<dyn std::error::Error>> {
    let staff = org::provisioning::add_staff(&w.h.owner, "vpyd", "Quản lý Hoa", "Quản lý").await?;
    sqlx::query(
        "UPDATE app.staff SET user_id = (SELECT id FROM app.user_account WHERE phone_e164 = $2)
         WHERE id = $1",
    )
    .bind(staff)
    .bind(format!("+84{}", &MANAGER_PHONE[1..]))
    .execute(&w.h.owner)
    .await?;
    Ok(())
}

fn concrete(route: &Route, f: &Fixture) -> String {
    route.path.replace("{id}", f.id_of(route.id))
}

fn label(route: &Route) -> String {
    format!("{} {}", route.method, route.path)
}

// -------------------------------------------------------------------------------- the tests

/// Routes mounted in the router source (`.route("/x", get(a).post(b))`), read from every context
/// crate. Catches a route that was mounted but never added to the OpenAPI document.
fn mounted_routes() -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = BTreeSet::new();
    let mut stack = vec![crates];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if name != "tests" && name != "target" {
                    stack.push(path);
                }
            } else if name.ends_with(".rs") {
                let text = std::fs::read_to_string(&path)?;
                // Unit-test modules mount throwaway routes (e.g. /echo); production code ends before them.
                let production = text.split("#[cfg(test)]").next().unwrap_or("");
                found.extend(routes_in(production));
            }
        }
    }
    Ok(found)
}

/// Parses `.route("<path>", <method>(..).<method>(..))` calls into "METHOD /api/v1<path>".
fn routes_in(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find(".route(") {
        rest = &rest[start + ".route(".len()..];
        // The argument list, up to the matching close paren.
        let mut depth = 1;
        let end = rest
            .char_indices()
            .find(|&(_, c)| {
                depth += match c {
                    '(' => 1,
                    ')' => -1,
                    _ => 0,
                };
                depth == 0
            })
            .map_or(rest.len(), |(i, _)| i);
        let args = &rest[..end];
        let Some(path) = args.split('"').nth(1) else {
            continue;
        };
        let handlers = args.split_once(',').map_or("", |(_, h)| h);
        for method in ["get", "post", "put", "patch", "delete"] {
            let calls = handlers
                .match_indices(&format!("{method}("))
                .filter(|(i, _)| {
                    *i == 0 || !handlers[..*i].ends_with(|c: char| c.is_alphanumeric() || c == '_')
                })
                .count();
            if calls > 0 {
                out.push(format!("{} /api/v1{path}", method.to_uppercase()));
            }
        }
    }
    out
}

#[test]
fn every_mounted_route_has_a_matrix_row() -> TestResult {
    let doc = app::openapi::ApiDoc::openapi();
    let mut documented = BTreeSet::new();
    for (path, item) in &doc.paths.paths {
        for (method, op) in [
            ("GET", &item.get),
            ("POST", &item.post),
            ("PUT", &item.put),
            ("PATCH", &item.patch),
            ("DELETE", &item.delete),
        ] {
            if op.is_some() {
                documented.insert(format!("{method} {path}"));
            }
        }
    }
    let rows: BTreeSet<String> = ROUTES.iter().map(label).collect();
    assert_eq!(rows.len(), ROUTES.len(), "duplicate row in ROUTES");
    let undocumented: BTreeSet<String> = PUBLIC_UNDOCUMENTED
        .iter()
        .map(|(m, p)| format!("{m} {p}"))
        .collect();

    let missing: Vec<&String> = documented.difference(&rows).collect();
    assert!(
        missing.is_empty(),
        "route(s) without an authorization row: {missing:?}\n\
         → add a row to ROUTES in apps/api/crates/app/tests/authz_matrix.rs that mirrors \
         memory/wiki/security/authz-matrix.md (see memory/wiki/conventions/testing.md)"
    );
    let stale: Vec<&String> = rows.difference(&documented).collect();
    assert!(
        stale.is_empty(),
        "ROUTES has rows for routes that are not in the OpenAPI document: {stale:?}"
    );

    let mounted = mounted_routes()?;
    let known: BTreeSet<String> = documented.union(&undocumented).cloned().collect();
    let hidden: Vec<&String> = mounted.difference(&known).collect();
    assert!(
        hidden.is_empty(),
        "route(s) mounted in a router but missing from the OpenAPI document (openapi.rs): {hidden:?}"
    );
    let ghost: Vec<&String> = known.difference(&mounted).collect();
    assert!(
        ghost.is_empty(),
        "documented route(s) not found in any router source: {ghost:?} (route parser out of date?)"
    );
    Ok(())
}

/// The table must agree with the seeded roles: a row's `allow` list is exactly the roles whose
/// default permissions contain that permission. Catches drift between roles.rs and the matrix.
#[test]
fn matrix_rows_agree_with_seeded_roles() {
    for route in ROUTES {
        if let Access::Perm(permission, allow) = route.access {
            let holders: Vec<SystemRole> = SystemRole::ALL
                .into_iter()
                .filter(|r| r.default_permissions().contains(&permission))
                .collect();
            assert_eq!(
                holders,
                allow.to_vec(),
                "{}: authz-matrix says {allow:?} hold {}, roles.rs seeds {holders:?}",
                label(route),
                permission.as_str()
            );
        }
    }
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn every_non_public_route_rejects_anonymous_and_forged_sessions(pool: PgPool) -> TestResult {
    let w = world(pool).await?;
    let mut failures = Vec::new();
    for route in ROUTES {
        let path = concrete(route, &w.home);
        let body = (route.body)(&w.home);
        let anonymous = send_route(&w.app, route, &path, None, body.clone()).await;
        if matches!(route.access, Access::Public) {
            if anonymous.status == StatusCode::UNAUTHORIZED
                && anonymous.body["code"] == "auth.unauthenticated"
            {
                failures.push(format!("{}: public route demanded a session", label(route)));
            }
            continue;
        }
        let forged = send_route(&w.app, route, &path, Some("forged-session-token"), body).await;
        for (who, reply) in [("anonymous", &anonymous), ("forged cookie", &forged)] {
            if reply.status != StatusCode::UNAUTHORIZED {
                failures.push(format!(
                    "{} as {who}: expected 401, got {} {}",
                    label(route),
                    reply.status,
                    reply.body
                ));
            }
        }
    }
    for (method, path) in PUBLIC_UNDOCUMENTED {
        let reply = send(&w.app, method, path, None, None, &[]).await;
        if reply.status != StatusCode::OK {
            failures.push(format!(
                "{method} {path}: expected 200, got {}",
                reply.status
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn each_role_gets_exactly_what_the_matrix_grants(pool: PgPool) -> TestResult {
    let w = world(pool).await?;
    // A manager records only once their login is linked to their own profile (EM 2026-11-08,
    // `staff.link_required`, covered in `staff.rs`); here the manager is a linked one.
    link_manager(&w).await?;
    let mut failures = Vec::new();
    let mut cells = 0;
    for role in SystemRole::ALL {
        let mut shared = session(&w.h, phone_of(role)).await?;
        for route in ROUTES {
            let expected_allowed = match route.access {
                // Pending-MFA routes: `pending_mfa_sessions_reach_only_the_mfa_routes`.
                // Operator rows: `every_admin_route_needs_a_full_operator_session`.
                Access::Public
                | Access::PendingMfa
                | Access::Operator
                | Access::OperatorPending => continue,
                Access::Session => true,
                Access::Perm(_, allow) => allow.contains(&role),
            };
            let token = if route.consumes_session {
                session(&w.h, phone_of(role)).await?
            } else {
                shared.clone()
            };
            let reply = send_route(
                &w.app,
                route,
                &concrete(route, &w.home),
                Some(&token),
                (route.body)(&w.home),
            )
            .await;
            cells += 1;
            if route.consumes_session {
                // logout-all revokes every session of the user, the shared one included.
                shared = session(&w.h, phone_of(role)).await?;
            }
            let ok = if expected_allowed {
                !matches!(
                    reply.status,
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                ) && !reply.status.is_server_error()
            } else {
                reply.status == StatusCode::FORBIDDEN && reply.body["code"] == "auth.forbidden"
            };
            if !ok {
                failures.push(format!(
                    "{} as {}: expected {}, got {} {}",
                    label(route),
                    role.key(),
                    if expected_allowed {
                        "allowed"
                    } else {
                        "403 auth.forbidden"
                    },
                    reply.status,
                    reply.body
                ));
            }
        }
    }
    assert!(cells > 0);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// ADR-0014 (EM 2026-11-07 Q-S6): a former staff member — a manager or technician whose linked
/// profile has left — holds only `staff.read_own_past`, whatever their role grants. Session
/// routes and the rows marked `.former_staff()` (their own month and day) are allowed; every
/// other route is `403 auth.forbidden`. The owner is never downgraded (`staff.rs`).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_former_staff_member_reaches_only_their_past_pay(pool: PgPool) -> TestResult {
    let w = world(pool).await?;
    let owner = session(&w.h, OWNER_PHONE).await?;
    for role in [Manager, Technician] {
        let user: String =
            sqlx::query_scalar("SELECT id::text FROM app.user_account WHERE phone_e164 = $1")
                .bind(format!("+84{}", &phone_of(role)[1..]))
                .fetch_one(&w.h.owner)
                .await?;
        let hired = send(
            &w.app,
            "POST",
            "/staff",
            Some(&owner),
            Some(json!({"full_name": role.key(), "position": "Kỹ thuật viên", "user_id": user})),
            &[],
        )
        .await;
        let (Some(id), Some(version)) = (hired.body["id"].as_str(), hired.body["version"].as_i64())
        else {
            return Err(format!("hire {} → {} {}", role.key(), hired.status, hired.body).into());
        };
        let tag = format!("\"{version}\"");
        let ended = send(
            &w.app,
            "POST",
            &format!("/staff/{id}/end"),
            Some(&owner),
            Some(json!({"ended_on": "2025-12-31", "reason": "Nghỉ việc"})),
            &[("If-Match", &tag)],
        )
        .await;
        assert_eq!(ended.status, StatusCode::OK, "{}", ended.body);
    }
    let mut failures = Vec::new();
    for role in [Manager, Technician] {
        let mut shared = session(&w.h, phone_of(role)).await?;
        for route in ROUTES {
            let expected_allowed = match route.access {
                Access::Public
                | Access::PendingMfa
                | Access::Operator
                | Access::OperatorPending => continue,
                Access::Session => true,
                Access::Perm(..) => route.former_staff,
            };
            let token = if route.consumes_session {
                session(&w.h, phone_of(role)).await?
            } else {
                shared.clone()
            };
            let reply = send_route(
                &w.app,
                route,
                &concrete(route, &w.home),
                Some(&token),
                (route.body)(&w.home),
            )
            .await;
            if route.consumes_session {
                shared = session(&w.h, phone_of(role)).await?;
            }
            let ok = if expected_allowed {
                !matches!(
                    reply.status,
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                ) && !reply.status.is_server_error()
            } else {
                reply.status == StatusCode::FORBIDDEN && reply.body["code"] == "auth.forbidden"
            };
            if !ok {
                failures.push(format!(
                    "{} as former {}: expected {}, got {} {}",
                    label(route),
                    role.key(),
                    if expected_allowed {
                        "allowed"
                    } else {
                        "403 auth.forbidden"
                    },
                    reply.status,
                    reply.body
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// authz-matrix test #7 (S2b): a session that only passed the password step reaches nothing but
/// the MFA routes and logout; a full session cannot use the MFA-step routes.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn pending_mfa_sessions_reach_only_the_mfa_routes(pool: PgPool) -> TestResult {
    let w = world(pool).await?;
    let (pending, login) = password_session(&w.app, OWNER_PHONE).await?;
    assert_eq!(
        login.body["next"], "mfa_verify",
        "owner enrolled by the fixture"
    );
    let full = session(&w.h, OWNER_PHONE).await?;
    let mut failures = Vec::new();
    for route in ROUTES {
        let path = concrete(route, &w.home);
        let body = (route.body)(&w.home);
        match route.access {
            Access::Public | Access::Operator | Access::OperatorPending => {}
            Access::PendingMfa => {
                let reply = send_route(&w.app, route, &path, Some(&full), body).await;
                if (reply.status, reply.body["code"].as_str())
                    != (StatusCode::CONFLICT, Some("auth.mfa_not_pending"))
                {
                    failures.push(format!(
                        "{} with a full session: expected 409 auth.mfa_not_pending, got {} {}",
                        label(route),
                        reply.status,
                        reply.body
                    ));
                }
            }
            // Logout accepts a pending session by design; checked last (it ends the session).
            Access::Session if route.path == "/api/v1/auth/logout" => {}
            Access::Session | Access::Perm(..) => {
                let reply = send_route(&w.app, route, &path, Some(&pending), body).await;
                let code = reply.body["code"].as_str().unwrap_or_default();
                if reply.status != StatusCode::UNAUTHORIZED || !code.starts_with("auth.mfa_") {
                    failures.push(format!(
                        "{} with a pending-MFA session: expected 401 auth.mfa_*, got {} {}",
                        label(route),
                        reply.status,
                        reply.body
                    ));
                }
            }
        }
    }
    let out = send(&w.app, "POST", "/auth/logout", Some(&pending), None, &[]).await;
    if out.status != StatusCode::NO_CONTENT {
        failures.push(format!("logout of a pending session: got {}", out.status));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// authz-matrix test #2: an id from another spa is indistinguishable from a missing one.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn ids_from_another_spa_are_not_found(pool: PgPool) -> TestResult {
    let w = world(pool).await?;
    let intruder = session(&w.h, OTHER_OWNER_PHONE).await?;
    let mut failures = Vec::new();
    let mut swept = 0;
    let spa_scoped =
        |r: &&Route| !matches!(r.access, Access::Operator | Access::OperatorPending);
    for route in ROUTES
        .iter()
        .filter(|r| r.id != Target::None)
        .filter(spa_scoped)
    {
        // The intruder's body references only their own records; the path id is the only foreign thing.
        let reply = send_route(
            &w.app,
            route,
            &concrete(route, &w.home),
            Some(&intruder),
            (route.body)(&w.other),
        )
        .await;
        swept += 1;
        if reply.status != StatusCode::NOT_FOUND {
            failures.push(format!(
                "{} with another spa's id: expected 404, got {} {}",
                label(route),
                reply.status,
                reply.body
            ));
        }
        // Control: the same call on the intruder's own record is not a 404, so the 404 above is
        // tenant isolation and not a broken request.
        let own = send_route(
            &w.app,
            route,
            &concrete(route, &w.other),
            Some(&intruder),
            (route.body)(&w.other),
        )
        .await;
        if own.status == StatusCode::NOT_FOUND {
            failures.push(format!(
                "{}: control call on own record was 404",
                label(route)
            ));
        }
    }
    assert!(swept > 0, "no {{id}} routes swept");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// ADR-0018: every console route needs a full operator session. Spa sessions of every role (the
/// owner's included) are `401`, a pending operator session reaches only the MFA steps, a full one
/// is allowed, and a write without a recent step-up is `403 platform.step_up_required`.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn every_admin_route_needs_a_full_operator_session(pool: PgPool) -> TestResult {
    let w = world(pool).await?;
    common::add_operator(&w.h).await?;
    let mut failures = Vec::new();
    let admin_rows = || {
        ROUTES
            .iter()
            .filter(|r| matches!(r.access, Access::Operator | Access::OperatorPending))
    };
    for role in SystemRole::ALL {
        let spa = session(&w.h, phone_of(role)).await?;
        for route in admin_rows() {
            let reply = send_route(
                &w.app,
                route,
                &concrete(route, &w.home),
                Some(&spa),
                (route.body)(&w.home),
            )
            .await;
            if reply.status != StatusCode::UNAUTHORIZED {
                failures.push(format!(
                    "{} as spa {}: expected 401, got {} {}",
                    label(route),
                    role.key(),
                    reply.status,
                    reply.body
                ));
            }
        }
    }
    // Enroll once, so the pending sessions below are `mfa_verify` ones.
    let mut full = common::operator_session(&w.h).await?;
    for route in admin_rows().filter(|r| matches!(r.access, Access::Operator)) {
        let pending = operator_pending(&w.app).await;
        let reply = operator_send(&w.app, route, &w.home, &pending).await;
        if reply.status != StatusCode::UNAUTHORIZED {
            failures.push(format!(
                "{} with a pending operator session: expected 401, got {} {}",
                label(route),
                reply.status,
                reply.body
            ));
        }
    }
    for route in admin_rows() {
        let token = match route.access {
            Access::OperatorPending => operator_pending(&w.app).await,
            _ if route.consumes_session => common::operator_session(&w.h).await?,
            _ => full.clone(),
        };
        let reply = operator_send(&w.app, route, &w.home, &token).await;
        if route.path == "/api/v1/admin/auth/reauth" {
            // The session is accepted; the wrong password in the body is refused on its own.
            if reply.body["code"] != "auth.invalid_credentials" {
                failures.push(format!(
                    "{}: expected auth.invalid_credentials, got {} {}",
                    label(route),
                    reply.status,
                    reply.body
                ));
            }
            full = common::operator_session(&w.h).await?;
            continue;
        }
        let refused = reply.status == StatusCode::FORBIDDEN
            || reply.status.is_server_error()
            || (reply.status == StatusCode::UNAUTHORIZED
                && reply.body["code"] == "auth.unauthenticated");
        if refused {
            failures.push(format!(
                "{} as operator: expected allowed, got {} {}",
                label(route),
                reply.status,
                reply.body
            ));
        }
    }
    common::expire_step_up(&w.h).await?;
    for route in admin_rows().filter(|r| {
        matches!(r.access, Access::Operator)
            && r.method != "GET"
            && !r.path.starts_with("/api/v1/admin/auth/")
    }) {
        let reply = operator_send(&w.app, route, &w.home, &full).await;
        if (reply.status, reply.body["code"].as_str())
            != (StatusCode::FORBIDDEN, Some("platform.step_up_required"))
        {
            failures.push(format!(
                "{} without a step-up: expected 403 platform.step_up_required, got {} {}",
                label(route),
                reply.status,
                reply.body
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// Sends `route` with the operator cookie, from a fresh client IP.
async fn operator_send(app: &Router, route: &Route, f: &Fixture, op: &str) -> Reply {
    let ip = next_ip();
    let cookie = format!("__Host-op-session={op}");
    let path = concrete(route, f);
    let path = path.strip_prefix("/api/v1").unwrap_or(&path);
    call_with_headers(
        app,
        route.method,
        path,
        None,
        (route.body)(f),
        &[("X-Forwarded-For", &ip), ("Cookie", &cookie)],
    )
    .await
}

/// The password step of the operator: a session still owing its second factor.
async fn operator_pending(app: &Router) -> String {
    let ip = next_ip();
    call_with_headers(
        app,
        "POST",
        "/admin/auth/login",
        None,
        Some(json!({"username": common::OPERATOR, "password": common::OPERATOR_PASSWORD})),
        &[("X-Forwarded-For", &ip)],
    )
    .await
    .op_session()
    .unwrap_or_default()
}

#[test]
fn route_parser_reads_chained_and_multiline_routes() {
    let source = r#"
        Router::new()
            .route(
                "/a",
                get(list_a).post(create_a),
            )
            .route("/a/{id}", put(update_a))
            .route("/b", get(forget(x)))
    "#;
    assert_eq!(
        routes_in(source),
        [
            "GET /api/v1/a",
            "POST /api/v1/a",
            "PUT /api/v1/a/{id}",
            "GET /api/v1/b"
        ]
    );
}
