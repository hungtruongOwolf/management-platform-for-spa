//! Built-in roles seeded for every new tenant. Owners can edit roles later (spec #4); this is only
//! the starting point. Must match memory/wiki/security/authz-matrix.md.

use kernel::Permission;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemRole {
    Owner,
    Manager,
    Technician,
}

impl SystemRole {
    pub const ALL: [SystemRole; 3] = [Self::Owner, Self::Manager, Self::Technician];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Manager => "manager",
            Self::Technician => "technician",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Owner => "Chủ spa",
            Self::Manager => "Quản lý",
            Self::Technician => "Kỹ thuật viên",
        }
    }

    /// Seed value of `role.requires_mfa` (ADR-0004: owners must use TOTP). The database enforces
    /// it for the system owner role; other roles may opt in later as data.
    pub const fn requires_mfa(self) -> bool {
        matches!(self, Self::Owner)
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.key() == key)
    }

    pub fn default_permissions(self) -> Vec<Permission> {
        use Permission as P;
        match self {
            // Everything inside the tenant; platform administration is never a tenant permission.
            Self::Owner => P::ALL
                .iter()
                .copied()
                .filter(|p| *p != P::PlatformTenantManage)
                .collect(),
            Self::Manager => vec![
                P::CatalogRead,
                P::StaffReadBasic,
                P::CustomerRead,
                P::CustomerWrite,
                P::CustomerHealthNotesRead,
                P::CustomerHealthNotesWrite,
                // Re-engagement calls are the manager's job (EM 2026-11-05, round 3).
                P::CustomerFollowUp,
                P::PackageSell,
                P::PackageUseExpiredOverride,
                P::VisitRecord,
                P::VisitRead,
                // Bookings are the front desk's job (EM review round 4, spec appointments.md).
                P::AppointmentManage,
                P::PaymentCollect,
                P::SaleRead,
                P::SaleDiscount,
                P::RecordVoidSameDay,
                P::ReportRead,
                // Default on: the manager is also the cashier (spec management-views, open question Q1).
                P::ReportRevenueRead,
                // Staff management (spec staff-management): the manager keeps the daily books of
                // attendance and evaluation but sees no salary (payroll.*, open question Q-S1).
                P::AttendanceRecord,
                P::EvaluationRecord,
                P::StaffReadOwn,
            ],
            // Own attendance, evaluation and payslip only (`/me/staff/*`).
            Self::Technician => vec![P::CatalogRead, P::StaffReadBasic, P::StaffReadOwn],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_has_every_tenant_permission_but_not_platform() {
        let owner = SystemRole::Owner.default_permissions();
        assert!(owner.contains(&Permission::RecordVoidClosedDay));
        assert!(owner.contains(&Permission::CustomerAnonymize));
        assert!(!owner.contains(&Permission::PlatformTenantManage));
        assert_eq!(owner.len(), Permission::ALL.len() - 1);
    }

    #[test]
    fn manager_cannot_do_owner_only_actions() {
        let manager = SystemRole::Manager.default_permissions();
        for owner_only in [
            Permission::CatalogManage,
            Permission::CustomerAnonymize,
            Permission::PackageExtendExpiry,
            Permission::PackageCancelRefund,
            Permission::SaleDiscountUnlimited,
            Permission::RecordVoidClosedDay,
            Permission::AuditRead,
            Permission::SupportGrantManage,
            // Paper-book import is an operator migration, not a front-desk task (0013).
            Permission::OpeningBalanceImport,
            // 3–5 year reports: rare, slow, owner only (EM Q5, 2026-11-05).
            Permission::ReportLongRange,
            // Entering a past day's visits: owner only (EM round 4 item 5).
            Permission::VisitBackdate,
            // The advisory "max customers per slot" is the owner's call (EM round 5 item 6).
            Permission::AppointmentSettings,
            // Salaries: owner only by default (spec staff-management Q-S1).
            Permission::StaffManage,
            Permission::PayrollRead,
            Permission::PayrollClose,
        ] {
            assert!(!manager.contains(&owner_only), "{owner_only:?}");
        }
    }

    #[test]
    fn technician_never_sees_customer_data() {
        let technician = SystemRole::Technician.default_permissions();
        assert!(!technician.contains(&Permission::CustomerRead));
        assert!(!technician.contains(&Permission::CustomerHealthNotesRead));
        // EM Q7 (2026-11-05): technicians do not manage customers, so never follow up.
        assert!(!technician.contains(&Permission::CustomerFollowUp));
        // No user <-> staff link yet, so "own schedule only" is not safe: no appointments at all.
        assert!(!technician.contains(&Permission::AppointmentManage));
        assert_eq!(
            technician,
            vec![
                Permission::CatalogRead,
                Permission::StaffReadBasic,
                Permission::StaffReadOwn
            ]
        );
    }

    #[test]
    fn only_owner_requires_mfa_by_default() {
        assert!(SystemRole::Owner.requires_mfa());
        assert!(!SystemRole::Manager.requires_mfa());
        assert!(!SystemRole::Technician.requires_mfa());
    }

    #[test]
    fn keys_round_trip() {
        for role in SystemRole::ALL {
            assert_eq!(SystemRole::from_key(role.key()), Some(role));
        }
        assert_eq!(SystemRole::from_key("admin"), None);
    }
}
