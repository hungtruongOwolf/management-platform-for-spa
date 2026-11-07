//! Whose records a caller may write (EM decision C, 2026-11-07): attendance, daily praise /
//! complaints, bonuses / penalties and the monthly evaluation.
//!
//! - The owner (`staff.manage`) records them for anyone.
//! - Anyone else holding the recording permissions (a manager) records them only for people who
//!   do not supervise: never for themselves, never for another manager or the owner.
//!
//! "Supervises" comes from the data, not from the free-text position: a staff profile linked to a
//! login whose roles grant `attendance.record`, `evaluation.record` or `staff.manage`. A profile
//! not linked to any login cannot supervise anyone in the app and counts as a technician.
//! Deny by default: a manager's own profile is always owner-only, linked or not to a role that
//! still grants the permissions.

use crate::repo;
use kernel::{Actor, AppError, Permission, TenantTx};
use std::collections::HashSet;
use uuid::Uuid;

/// `403` when a manager writes a record of a supervisor (or of themselves).
pub const OWNER_ONLY: &str = "staff.record_owner_only";

/// The permissions whose holders supervise staff.
const SUPERVISING: [Permission; 3] = [
    Permission::AttendanceRecord,
    Permission::EvaluationRecord,
    Permission::StaffManage,
];

/// Loaded once per use case, inside its transaction.
#[derive(Debug, Clone)]
pub struct RecordScope {
    /// The owner: may record for everyone.
    unrestricted: bool,
    /// Staff whose records only the owner may write.
    owner_only: HashSet<Uuid>,
}

impl RecordScope {
    pub async fn load(tx: &mut TenantTx, actor: &Actor) -> Result<Self, AppError> {
        if actor.can(Permission::StaffManage) {
            return Ok(Self {
                unrestricted: true,
                owner_only: HashSet::new(),
            });
        }
        let permissions: Vec<String> = SUPERVISING.iter().map(|p| p.as_str().to_owned()).collect();
        let mut owner_only: HashSet<Uuid> = repo::supervising_staff(tx, &permissions)
            .await?
            .into_iter()
            .collect();
        owner_only.extend(repo::staff_of_user(tx, actor.user_id.as_uuid()).await?);
        Ok(Self {
            unrestricted: false,
            owner_only,
        })
    }

    /// Whether the caller may write this person's records (the UI hides the controls otherwise).
    pub fn may_record(&self, staff_id: Uuid) -> bool {
        self.unrestricted || !self.owner_only.contains(&staff_id)
    }

    /// `403 staff.record_owner_only` when any of `staff` is outside the caller's scope.
    pub fn require(&self, staff: impl IntoIterator<Item = Uuid>) -> Result<(), AppError> {
        if staff.into_iter().all(|s| self.may_record(s)) {
            Ok(())
        } else {
            Err(AppError::Forbidden { code: OWNER_ONLY })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owner_records_everyone_a_manager_only_non_supervisors() {
        let (manager, other_manager, technician) =
            (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let owner = RecordScope {
            unrestricted: true,
            owner_only: HashSet::new(),
        };
        assert!(owner.require([manager, other_manager, technician]).is_ok());
        let scope = RecordScope {
            unrestricted: false,
            owner_only: [manager, other_manager].into_iter().collect(),
        };
        assert!(scope.may_record(technician));
        assert!(!scope.may_record(manager), "never themselves");
        assert!(!scope.may_record(other_manager), "never another manager");
        assert!(scope.require([technician]).is_ok());
        assert!(matches!(
            scope.require([technician, other_manager]),
            Err(AppError::Forbidden { code: OWNER_ONLY })
        ));
    }
}
