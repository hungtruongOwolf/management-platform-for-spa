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
        packages::http::read_package,
        packages::http::list_customer_packages,
        sales::http::import_opening_balance,
        packages::http::extend_package,
        packages::http::cancel_package,
        sales::http::create_sale,
        sales::http::read_sale,
        sales::http::void_sale,
        operations::http::create_visit,
        operations::http::list_visits,
        operations::http::read_visit,
        operations::http::update_visit,
        operations::http::void_visit,
        reporting::http::daily_report,
        reporting::http::range_report,
        reporting::http::ledger,
        reporting::http::inactive_customers,
        reporting::http::customer_history,
        reporting::http::customer_summary,
        reporting::http::package_usage,
        reporting::http::list_packages,
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
    )),
    tags(
        (name = "auth", description = "Đăng nhập, phiên, đổi spa"),
        (name = "catalog", description = "Bảng giá dịch vụ và gói liệu trình"),
        (name = "customers", description = "Hồ sơ khách hàng"),
        (name = "staff", description = "Nhân viên (chọn người chốt, KTV)"),
        (name = "packages", description = "Liệu trình của khách"),
        (name = "sales", description = "Bán liệu trình, thanh toán"),
        (name = "visits", description = "Ghi lượt khách tại quầy"),
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
            "/api/v1/sales",
            "/api/v1/sales/{id}",
            "/api/v1/customer-packages/{id}",
            "/api/v1/customers/{id}/packages",
            "/api/v1/opening-balances",
            "/api/v1/visits",
            "/api/v1/visits/{id}",
            "/api/v1/visits/{id}/void",
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
            "CustomerPackageDto",
            "StaffDto",
            "OpeningBalanceCreate",
            "OpeningBalanceDto",
            "VisitCreate",
            "VisitDto",
            "VisitUpdate",
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
