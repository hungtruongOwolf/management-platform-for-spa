//! Permission catalog (constants in code) and the acting user. Roles are data: a role is a set of
//! these permissions stored per tenant. Source of truth for defaults: memory/wiki/security/authz-matrix.md.

use crate::error::AppError;
use crate::feature::Feature;
use crate::ids::{TenantId, UserId};
use crate::tenant::TenantFacts;
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;
use std::net::IpAddr;

macro_rules! permissions {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Permission { $($variant),+ }

        impl Permission {
            pub const ALL: &'static [Permission] = &[$(Permission::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self { $(Permission::$variant => $name),+ }
            }
        }

        impl std::str::FromStr for Permission {
            type Err = UnknownPermission;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($name => Ok(Permission::$variant),)+
                    _ => Err(UnknownPermission(s.to_owned())),
                }
            }
        }
    };
}

permissions! {
    CatalogRead => "catalog.read",
    CatalogManage => "catalog.manage",
    StaffReadBasic => "staff.read_basic",
    StaffManage => "staff.manage",
    StaffRead => "staff.read",
    StaffReadOwn => "staff.read_own",
    StaffReadOwnPast => "staff.read_own_past",
    AttendanceRecord => "attendance.record",
    EvaluationRecord => "evaluation.record",
    PayrollRead => "payroll.read",
    PayrollClose => "payroll.close",
    CustomerRead => "customer.read",
    CustomerWrite => "customer.write",
    CustomerHealthNotesRead => "customer.health_notes.read",
    CustomerHealthNotesWrite => "customer.health_notes.write",
    CustomerAnonymize => "customer.anonymize",
    CustomerFollowUp => "customer.follow_up",
    PackageSell => "package.sell",
    PackageExtendExpiry => "package.extend_expiry",
    PackageCancelRefund => "package.cancel_refund",
    PackageUseExpiredOverride => "package.use_expired_override",
    OpeningBalanceImport => "opening_balance.import",
    VisitRecord => "visit.record",
    VisitRead => "visit.read",
    VisitBackdate => "visit.backdate",
    AppointmentManage => "appointment.manage",
    AppointmentSettings => "appointment.settings",
    PaymentCollect => "payment.collect",
    SaleRead => "sale.read",
    SaleDiscount => "sale.discount",
    SaleDiscountUnlimited => "sale.discount_unlimited",
    RecordVoidSameDay => "record.void_same_day",
    RecordVoidClosedDay => "record.void_closed_day",
    ReportRead => "report.read",
    ReportRevenueRead => "report.revenue.read",
    ReportLongRange => "report.long_range",
    AuditRead => "audit.read",
    SupportGrantManage => "support_grant.manage",
    PlatformTenantManage => "platform.tenant.manage",
}

/// What a former staff member's login may do (EM 2026-11-07 Q-S6, ADR-0014): read their own
/// attendance and payslips of the months they worked, nothing else. Never granted by a role: it
/// is derived at request time from the employment state ([`effective_permissions`]).
pub const FORMER_STAFF: &[Permission] = &[Permission::StaffReadOwnPast];

/// The permissions a member actually holds: their roles', unless their linked staff profile has
/// left (no open employment period), in which case only [`FORMER_STAFF`] — deny by default for
/// everything else, whatever the roles say. A login that manages staff (the owner) is never
/// downgraded: the roster refuses to end such a profile, and linking one later must not lock the
/// spa out.
pub fn effective_permissions(roles: Vec<Permission>, former_staff: bool) -> Vec<Permission> {
    if former_staff && !roles.contains(&Permission::StaffManage) {
        FORMER_STAFF.to_vec()
    } else {
        roles
            .into_iter()
            .filter(|p| *p != Permission::StaffReadOwnPast)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown permission: {0}")]
pub struct UnknownPermission(pub String);

/// The authenticated user acting within one tenant. Built from the session, never from the request body.
#[derive(Debug, Clone)]
pub struct Actor {
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub ip: Option<IpAddr>,
    permissions: BTreeSet<Permission>,
    /// The spa's settings, read with the permissions (db review A4); `None` for actors built by
    /// tooling and tests, whose transactions then read them.
    tenant_facts: Option<TenantFacts>,
    /// Features the spa has switched off (ADR-0018 D4): their permissions are not in
    /// `permissions`, and requiring one answers `403 feature.disabled`.
    disabled_features: BTreeSet<Feature>,
    /// Tooling only (`app seed-demo`): the instant this actor's audit entries are stamped with,
    /// so back-filled history reads as written on its own day (ADR-0021). A session's actor never
    /// has one: nothing a request sends reaches it.
    clock: Option<DateTime<Utc>>,
}

impl Actor {
    pub fn new(
        user_id: UserId,
        tenant_id: TenantId,
        permissions: impl IntoIterator<Item = Permission>,
        ip: Option<IpAddr>,
    ) -> Self {
        Self {
            user_id,
            tenant_id,
            ip,
            permissions: permissions.into_iter().collect(),
            tenant_facts: None,
            disabled_features: BTreeSet::new(),
            clock: None,
        }
    }

    /// Tooling only: this actor's audit entries are stamped `at` instead of the database clock
    /// (see [`Actor::clock`]). Use cases that take a clock (`*_as_of`) stamp their own `now`.
    #[must_use]
    pub const fn at_clock(mut self, at: DateTime<Utc>) -> Self {
        self.clock = Some(at);
        self
    }

    /// The instant set by [`Actor::at_clock`], if any.
    pub const fn clock(&self) -> Option<DateTime<Utc>> {
        self.clock
    }

    /// The spa's switched-off features: their permissions are removed, whatever the roles say.
    #[must_use]
    pub fn with_disabled_features(mut self, features: impl IntoIterator<Item = Feature>) -> Self {
        self.disabled_features = features.into_iter().collect();
        let stripped = crate::feature::strip(
            self.permissions.iter().copied().collect(),
            &self.disabled_features,
        );
        self.permissions = stripped.into_iter().collect();
        self
    }

    pub fn feature_enabled(&self, feature: Feature) -> bool {
        !self.disabled_features.contains(&feature)
    }

    /// For a route of a feature the spa switched off: `403 feature.disabled`.
    pub fn require_feature(&self, feature: Feature) -> Result<(), AppError> {
        if self.feature_enabled(feature) {
            Ok(())
        } else {
            Err(AppError::Forbidden {
                code: "feature.disabled",
            })
        }
    }

    /// Carries the spa's settings, so the request's transactions do not read them again.
    /// They must be the settings of `self.tenant_id`.
    #[must_use]
    pub fn with_tenant_facts(mut self, facts: TenantFacts) -> Self {
        self.tenant_facts = Some(facts);
        self
    }

    pub const fn tenant_facts(&self) -> Option<&TenantFacts> {
        self.tenant_facts.as_ref()
    }

    pub fn can(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    /// Deny by default: every use case calls this before touching data.
    pub fn require(&self, permission: Permission) -> Result<(), AppError> {
        if self.can(permission) {
            Ok(())
        } else if crate::feature::owning(permission)
            .is_some_and(|f| self.disabled_features.contains(&f))
        {
            Err(AppError::Forbidden {
                code: "feature.disabled",
            })
        } else {
            Err(AppError::forbidden())
        }
    }

    pub fn permissions(&self) -> impl Iterator<Item = Permission> + '_ {
        self.permissions.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn every_permission_round_trips_and_is_unique() {
        let names: BTreeSet<&str> = Permission::ALL.iter().map(|p| p.as_str()).collect();
        assert_eq!(names.len(), Permission::ALL.len());
        for p in Permission::ALL {
            assert_eq!(Permission::from_str(p.as_str()), Ok(*p));
        }
    }

    #[test]
    fn a_switched_off_feature_removes_its_permissions() {
        let actor = Actor::new(
            UserId::new(),
            TenantId::new(),
            [
                Permission::AppointmentManage,
                Permission::CustomerRead,
                Permission::PayrollRead,
            ],
            None,
        )
        .with_disabled_features([Feature::Appointments]);
        assert!(!actor.can(Permission::AppointmentManage));
        assert!(actor.can(Permission::CustomerRead));
        assert!(matches!(
            actor.require(Permission::AppointmentManage),
            Err(AppError::Forbidden {
                code: "feature.disabled"
            })
        ));
        // A permission the role never had stays an ordinary refusal.
        assert!(matches!(
            actor.require(Permission::CatalogManage),
            Err(AppError::Forbidden {
                code: "auth.forbidden"
            })
        ));
        assert!(actor.require_feature(Feature::Payroll).is_ok());
        assert!(actor.require_feature(Feature::Appointments).is_err());
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert!(Permission::from_str("customer.delete_everything").is_err());
    }

    #[test]
    fn a_former_staff_member_keeps_only_their_past_pay() {
        let technician = vec![
            Permission::CatalogRead,
            Permission::StaffReadBasic,
            Permission::StaffReadOwn,
        ];
        assert_eq!(
            effective_permissions(technician.clone(), true),
            [Permission::StaffReadOwnPast]
        );
        assert_eq!(effective_permissions(technician.clone(), false), technician);
        let manager = vec![Permission::VisitRecord, Permission::CustomerRead];
        assert_eq!(
            effective_permissions(manager, true),
            [Permission::StaffReadOwnPast]
        );
        let owner = vec![Permission::StaffManage, Permission::PayrollClose];
        assert_eq!(
            effective_permissions(owner.clone(), true),
            owner,
            "the owner is never downgraded"
        );
        assert_eq!(
            effective_permissions(vec![Permission::StaffReadOwnPast], false),
            [],
            "a role cannot grant the derived permission"
        );
    }

    #[test]
    fn require_denies_by_default() {
        let actor = Actor::new(
            UserId::new(),
            TenantId::new(),
            [Permission::CatalogRead],
            None,
        );
        assert!(actor.require(Permission::CatalogRead).is_ok());
        assert!(matches!(
            actor.require(Permission::CatalogManage),
            Err(AppError::Forbidden {
                code: "auth.forbidden"
            })
        ));
    }
}
