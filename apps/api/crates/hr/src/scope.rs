//! Whose records a caller may write (EM decision C, 2026-11-07): attendance, daily praise /
//! complaints, bonuses / penalties and the monthly evaluation.
//!
//! - The owner (`staff.manage`) records them for anyone.
//! - Anyone else holding the recording permissions (a manager) records them only for people who
//!   do not supervise: never for themselves, never for another manager or the owner.
//!
//! "Supervises" comes from the data: a staff profile linked to a login whose roles grant
//! `attendance.record`, `evaluation.record` or `staff.manage` ([`org::profile::SUPERVISING`]), or
//! a profile whose position names a manager ("Quản lý", [`org::profile::is_manager_position`]),
//! linked or not (F19, 2026-11-08: while a manager's profile is not linked to their login the app
//! cannot tell it is theirs, and an unlinked manager could mark their own attendance).
//! The caller's own profile (the one linked to their login) is always owner-only, and it is left
//! out of the books they read (EM 2026-11-08, [`RecordScope::lists`]): they read it in
//! "Công và lương của tôi" (`/me/*`), like a technician.

use crate::repo;
use kernel::{Actor, AppError, Permission, TenantTx};
use std::collections::HashSet;
use uuid::Uuid;

/// `403` when a manager writes a record of a supervisor (or of themselves).
pub const OWNER_ONLY: &str = "staff.record_owner_only";

/// Loaded once per use case, inside its transaction.
#[derive(Debug, Clone)]
pub struct RecordScope {
    /// The owner: may record for everyone.
    unrestricted: bool,
    /// Staff whose records only the owner may write.
    owner_only: HashSet<Uuid>,
    /// The caller's own profile, left out of the books they read; `None` for the owner.
    own: Option<Uuid>,
}

impl RecordScope {
    pub async fn load(tx: &mut TenantTx, actor: &Actor) -> Result<Self, AppError> {
        if actor.can(Permission::StaffManage) {
            return Ok(Self {
                unrestricted: true,
                owner_only: HashSet::new(),
                own: None,
            });
        }
        let permissions = org::profile::supervising_permissions();
        let mut owner_only: HashSet<Uuid> = repo::supervising_staff(tx, &permissions)
            .await?
            .into_iter()
            .collect();
        owner_only.extend(
            org::profile::profiles_in(tx)
                .await?
                .into_iter()
                .filter(|p| org::profile::is_manager_position(&p.position))
                .map(|p| p.id),
        );
        let own = repo::staff_of_user(tx, actor.user_id.as_uuid()).await?;
        owner_only.extend(own);
        Ok(Self {
            unrestricted: false,
            owner_only,
            own,
        })
    }

    /// Whether the caller may write this person's records (the UI hides the controls otherwise).
    pub fn may_record(&self, staff_id: Uuid) -> bool {
        self.unrestricted || !self.owner_only.contains(&staff_id)
    }

    /// Whether this person appears in the books the caller reads (attendance, daily reviews,
    /// evaluations and their history, bonuses / penalties): everyone but the caller's own
    /// profile; the owner sees everyone.
    pub fn lists(&self, staff_id: Uuid) -> bool {
        self.own != Some(staff_id)
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
            own: None,
        };
        assert!(owner.require([manager, other_manager, technician]).is_ok());
        assert!(owner.lists(manager) && owner.lists(technician));
        let scope = RecordScope {
            unrestricted: false,
            owner_only: [manager, other_manager].into_iter().collect(),
            own: Some(manager),
        };
        assert!(scope.may_record(technician));
        assert!(!scope.may_record(manager), "never themselves");
        assert!(!scope.may_record(other_manager), "never another manager");
        assert!(
            !scope.lists(manager),
            "their own profile is not in their books"
        );
        assert!(scope.lists(other_manager) && scope.lists(technician));
        assert!(scope.require([technician]).is_ok());
        assert!(matches!(
            scope.require([technician, other_manager]),
            Err(AppError::Forbidden { code: OWNER_ONLY })
        ));
    }
}
