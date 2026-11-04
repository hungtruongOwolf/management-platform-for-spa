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
        org::http::list_staff,
        packages::http::read_package,
        packages::http::list_customer_packages,
        packages::http::import_opening_balance,
        sales::http::create_sale,
        sales::http::read_sale,
    ),
    tags(
        (name = "auth", description = "Đăng nhập, phiên, đổi spa"),
        (name = "catalog", description = "Bảng giá dịch vụ và gói liệu trình"),
        (name = "customers", description = "Hồ sơ khách hàng"),
        (name = "staff", description = "Nhân viên (chọn người chốt, KTV)"),
        (name = "packages", description = "Liệu trình của khách, nhập từ sổ cũ"),
        (name = "sales", description = "Bán liệu trình, thanh toán"),
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
            "/api/v1/staff",
            "/api/v1/sales",
            "/api/v1/sales/{id}",
            "/api/v1/customer-packages/{id}",
            "/api/v1/customers/{id}/packages",
            "/api/v1/opening-balances",
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
        ] {
            assert!(schemas.contains_key(schema), "{schema} missing");
        }
    }
}
