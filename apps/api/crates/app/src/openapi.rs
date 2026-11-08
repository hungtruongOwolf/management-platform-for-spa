//! The API contract. `app openapi` writes it to apps/api/openapi.json; the TS client is generated
//! from that file, so a backend change that breaks the frontend fails the frontend typecheck (principle #8).

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(title = "Viên Phúc Y Đường API", version = "1"),
    paths(
        identity::http::login,
        identity::http::logout,
        identity::http::logout_all,
        identity::http::reauth,
        identity::http::switch_tenant,
        identity::http::me,
        identity::http::mfa_verify,
        identity::http::mfa_enroll_start,
        identity::http::mfa_enroll_confirm,
        catalog::http::list_categories,
        catalog::http::list_services,
        catalog::http::list_package_plans,
        catalog::http::create_category,
        catalog::http::update_category,
        catalog::http::create_service,
        catalog::http::update_service,
        catalog::http::create_plan,
        catalog::http::update_plan,
        crm::http::search,
        crm::http::create,
        crm::http::read,
        crm::http::update,
        crm::http::set_health_note,
        crm::http::phone_check,
        crm::http::record_follow_up,
        crm::http::list_follow_ups,
        org::http::list_staff,
        org::http::create_staff,
        org::http::list_profiles,
        org::http::list_accounts,
        org::http::get_staff,
        org::http::update_staff,
        org::http::set_staff_pay,
        org::http::set_staff_account,
        org::http::end_employment,
        org::http::rehire_staff,
        org::http::staff_history,
        hr::http::get_attendance,
        hr::http::put_attendance,
        hr::http::get_daily_reviews,
        hr::http::put_daily_reviews,
        hr::http::get_evaluations,
        hr::http::get_evaluation_history,
        hr::http::put_evaluation,
        hr::http::get_adjustments,
        hr::http::post_adjustment,
        hr::http::put_adjustment,
        hr::http::delete_adjustment,
        hr::http::get_payroll,
        hr::http::get_payslip,
        hr::http::close_payroll,
        hr::http::get_rules,
        hr::http::put_rules,
        hr::http::get_my_staff,
        hr::http::get_my_day,
        hr::http::get_my_evaluation_history,
        packages::http::read_package,
        packages::http::list_customer_packages,
        sales::http::import_opening_balance,
        packages::http::extend_package,
        packages::http::cancel_package,
        sales::http::list_sales,
        sales::http::create_sale,
        sales::http::create_checkout,
        sales::http::read_sale,
        sales::http::void_sale,
        operations::http::create_visit,
        operations::http::list_visits,
        operations::http::read_visit,
        operations::http::update_visit,
        operations::http::void_visit,
        operations::http::correct_visit,
        operations::http::add_to_visit,
        operations::http::convert_visit,
        operations::appointments::http::calendar,
        operations::appointments::http::my_calendar,
        operations::appointments::http::book,
        operations::appointments::http::book_group,
        operations::appointments::http::update_settings,
        operations::appointments::http::read_appointment,
        operations::appointments::http::update_appointment,
        operations::appointments::http::arrive,
        operations::appointments::http::undo_arrival,
        operations::appointments::http::move_group,
        operations::appointments::http::complete,
        operations::appointments::http::cancel,
        operations::appointments::http::no_show,
        reporting::http::daily_report,
        reporting::http::range_report,
        reporting::http::ledger,
        reporting::http::inactive_customers,
        reporting::http::customer_history,
        reporting::http::customer_summary,
        reporting::http::package_usage,
        reporting::http::list_packages,
        reporting::http::list_notes,
    ),
    // Enums used only as query parameters are not reached from any body, so register them.
    components(schemas(
        reporting::dto::PackageStatusFilter,
        reporting::dto::InactiveScope,
        reporting::dto::Granularity,
        reporting::dto::RangePreset,
        reporting::dto::HistoryKind,
        reporting::dto::CustomerKindFilter,
        reporting::dto::FollowUpFilter,
        reporting::dto::InactiveSort,
        reporting::notes_dto::NoteKind,
    )),
    tags(
        (name = "auth", description = "Đăng nhập, phiên, đổi spa"),
        (name = "catalog", description = "Bảng giá dịch vụ và gói liệu trình"),
        (name = "customers", description = "Hồ sơ khách hàng"),
        (name = "staff", description = "Nhân viên: hồ sơ, chấm công, đánh giá, thưởng/phạt, bảng lương"),
        (name = "packages", description = "Liệu trình của khách"),
        (name = "sales", description = "Bán liệu trình, thanh toán"),
        (name = "visits", description = "Ghi lượt khách tại quầy"),
        (name = "appointments", description = "Lịch hẹn: đặt lịch, khách đến, hoàn tất thành lượt khách"),
        (name = "reports", description = "Xem lại và theo dõi: hôm nay, theo khoảng ngày, sổ doanh thu, lịch sử khách, liệu trình"),
    )
)]
pub struct ApiDoc;

/// Pretty JSON with a trailing newline (stable diffs in git and CI).
pub fn json() -> Result<String, serde_json::Error> {
    ApiDoc::openapi().to_pretty_json().map(|mut s| {
        s.push('\n');
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_every_mounted_endpoint() {
        let doc = ApiDoc::openapi();
        let paths: Vec<&str> = doc.paths.paths.keys().map(String::as_str).collect();
        for expected in [
            "/api/v1/auth/login",
            "/api/v1/auth/logout",
            "/api/v1/auth/logout-all",
            "/api/v1/auth/reauth",
            "/api/v1/auth/switch-tenant",
            "/api/v1/me",
            "/api/v1/auth/mfa/verify",
            "/api/v1/auth/mfa/enroll/start",
            "/api/v1/auth/mfa/enroll/confirm",
            "/api/v1/service-categories",
            "/api/v1/services",
            "/api/v1/package-plans",
            "/api/v1/service-categories/{id}",
            "/api/v1/services/{id}",
            "/api/v1/package-plans/{id}",
            "/api/v1/customers",
            "/api/v1/customers/{id}",
            "/api/v1/customers/{id}/health-note",
            "/api/v1/customers/phone-check",
            "/api/v1/customers/{id}/follow-ups",
            "/api/v1/staff",
            "/api/v1/staff/{id}",
            "/api/v1/staff/{id}/pay",
            "/api/v1/staff/profiles",
            "/api/v1/staff/accounts",
            "/api/v1/attendance",
            "/api/v1/daily-reviews",
            "/api/v1/evaluations",
            "/api/v1/adjustments",
            "/api/v1/payroll",
            "/api/v1/payroll/payslips/{id}",
            "/api/v1/payroll/close",
            "/api/v1/payroll/rules",
            "/api/v1/me/staff",
            "/api/v1/me/staff/day",
            "/api/v1/me/appointments",
            "/api/v1/sales",
            "/api/v1/sales/{id}",
            "/api/v1/customer-packages/{id}",
            "/api/v1/customers/{id}/packages",
            "/api/v1/opening-balances",
            "/api/v1/visits",
            "/api/v1/visits/{id}",
            "/api/v1/visits/{id}/void",
            "/api/v1/visits/{id}/correct",
            "/api/v1/sales/{id}/void",
            "/api/v1/customer-packages/{id}/extend",
            "/api/v1/customer-packages/{id}/cancel",
            "/api/v1/reports/daily",
            "/api/v1/reports/range",
            "/api/v1/reports/ledger",
            "/api/v1/customers/inactive",
            "/api/v1/customers/{id}/history",
            "/api/v1/customers/{id}/summary",
            "/api/v1/customer-packages/{id}/usage",
            "/api/v1/packages",
        ] {
            assert!(paths.contains(&expected), "{expected} missing from OpenAPI");
        }
        let schemas = doc.components.map(|c| c.schemas).unwrap_or_default();
        for schema in [
            "Me",
            "LoginResponse",
            "MfaEnrollment",
            "MfaEnrolled",
            "ServiceDto",
            "ServicePrices",
            "PackagePlanDto",
            "ServiceWrite",
            "PlanWrite",
            "SaleCreate",
            "SaleDto",
            "DaySaleDto",
            "CustomerPackageDto",
            "StaffDto",
            "OpeningBalanceCreate",
            "OpeningBalanceDto",
            "VisitCreate",
            "VisitDto",
            "VisitUpdate",
            "VisitCorrection",
            "VisitCorrected",
            "VisitAdditions",
            "VisitConversion",
            "VisitPackageSaleCreate",
            "VisitPackageSaleDto",
            "VoidCreate",
            "PackageExtend",
            "PackageCancel",
            "RefundDto",
            "DailyReport",
            "RangeReport",
            "LedgerPage",
            "HistoryPage",
            "CustomerStats",
            "PackageUsage",
            "PackageOverviewPage",
            "InactivePage",
            "PhoneCheckDto",
            "FollowUpCreate",
            "FollowUpDto",
            "FollowUpList",
            "RangePreset",
            "FollowUpFilter",
            "NotesPage",
            "NoteKind",
        ] {
            assert!(schemas.contains_key(schema), "{schema} missing");
        }
    }

    /// Only `{…}` segments are path parameters; a query struct derived without
    /// `#[into_params(parameter_in = Query)]` is silently documented as a path parameter and the
    /// generated client then builds the wrong URL (`/customers/phone-check?phone=` was one).
    #[test]
    fn non_path_parameters_are_documented_as_query() -> Result<(), serde_json::Error> {
        let doc = serde_json::to_value(ApiDoc::openapi())?;
        let paths = doc["paths"].as_object().cloned().unwrap_or_default();
        for (path, item) in paths {
            for (method, op) in item.as_object().cloned().unwrap_or_default() {
                for p in op["parameters"].as_array().cloned().unwrap_or_default() {
                    let name = p["name"].as_str().unwrap_or_default();
                    let in_path = path.contains(&format!("{{{name}}}"));
                    let location = p["in"].as_str().unwrap_or_default();
                    if location == "header" {
                        continue;
                    }
                    assert_eq!(
                        location,
                        if in_path { "path" } else { "query" },
                        "{method} {path} parameter {name}"
                    );
                }
            }
        }
        Ok(())
    }
}
