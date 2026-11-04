//! Permission catalog (constants in code) and the acting user. Roles are data: a role is a set of
//! these permissions stored per tenant. Source of truth for defaults: memory/wiki/security/authz-matrix.md.

use crate::error::AppError;
use crate::ids::{TenantId, UserId};
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
    CustomerRead => "customer.read",
    CustomerWrite => "customer.write",
    CustomerHealthNotesRead => "customer.health_notes.read",
    CustomerHealthNotesWrite => "customer.health_notes.write",
    CustomerAnonymize => "customer.anonymize",
    PackageSell => "package.sell",
    PackageExtendExpiry => "package.extend_expiry",
    PackageCancelRefund => "package.cancel_refund",
    PackageUseExpiredOverride => "package.use_expired_override",
    OpeningBalanceImport => "opening_balance.import",
    VisitRecord => "visit.record",
    VisitRead => "visit.read",
    PaymentCollect => "payment.collect",
    SaleRead => "sale.read",
    SaleDiscount => "sale.discount",
    SaleDiscountUnlimited => "sale.discount_unlimited",
    RecordVoidSameDay => "record.void_same_day",
    RecordVoidClosedDay => "record.void_closed_day",
    AuditRead => "audit.read",
    SupportGrantManage => "support_grant.manage",
    PlatformTenantManage => "platform.tenant.manage",
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
    /// Password (and second factor) confirmed within the re-auth window (🔒 actions). Off unless
    /// the session layer says so: deny by default.
    recent_auth: bool,
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
            recent_auth: false,
        }
    }

    /// Set by the session extractor from `session.recent_auth_at`.
    #[must_use]
    pub fn with_recent_auth(mut self, recent: bool) -> Self {
        self.recent_auth = recent;
        self
    }

    /// Guard for 🔒 actions: `401 auth.reauth_required` unless the user re-entered their
    /// password (and code) within the last few minutes.
    pub fn require_recent_auth(&self) -> Result<(), AppError> {
        if self.recent_auth {
            Ok(())
        } else {
            Err(AppError::ReauthRequired)
        }
    }

    pub fn can(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    /// Deny by default: every use case calls this before touching data.
    pub fn require(&self, permission: Permission) -> Result<(), AppError> {
        if self.can(permission) {
            Ok(())
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
    fn recent_auth_is_off_unless_the_session_says_so() {
        let actor = Actor::new(UserId::new(), TenantId::new(), [], None);
        assert!(matches!(
            actor.require_recent_auth(),
            Err(AppError::ReauthRequired)
        ));
        assert!(actor.with_recent_auth(true).require_recent_auth().is_ok());
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert!(Permission::from_str("customer.delete_everything").is_err());
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
