//! Whose records a caller may write (EM decision C, 2026-11-07): attendance, daily praise /
//! complaints, bonuses / penalties and the monthly evaluation.
//!
//! - The owner (`staff.manage`) records them for anyone.
//! - Anyone else holding the recording permissions (a manager) records them only for people who
//!   do not supervise: never for themselves, never for another manager or the owner.
//!
//! Who supervises comes from **account roles only** (EM 2026-11-08,
//! [raw](../../../../memory/raw/2026-11-08-em-decision-role-by-account.md)): a staff profile linked
//! to a login whose roles grant `attendance.record`, `evaluation.record` or `staff.manage`
//! ([`org::profile::SUPERVISING`]). The free-text position never decides anything. A profile not
//! linked to any login counts as a technician.
//!
//! A manager must be linked to their own profile before they record anything
//! ([`LINK_REQUIRED`]): until then the app cannot tell which profile is theirs, so it could not
//! stop them recording for themselves (live-test F19). The caller's own profile is always
//! owner-only, and it is left out of the books they read ([`RecordScope::lists`]): they read it in
//! "Công và lương của tôi" (`/me/*`), like a technician.

use crate::repo;
use kernel::{Actor, AppError, Permission, TenantTx};
use std::collections::HashSet;
use uuid::Uuid;

/// `403` when a manager writes a record of a supervisor (or of themselves).
pub const OWNER_ONLY: &str = "staff.record_owner_only";

/// `403` when a supervising login that is not the owner records anything while it is not linked
/// to a staff profile (ask the owner to link the account).
pub const LINK_REQUIRED: &str = "staff.link_required";

/// Loaded once per use case, inside its transaction.
#[derive(Debug, Clone)]
pub struct RecordScope {
    /// The owner: may record for everyone.
    unrestricted: bool,
    /// Staff whose records only the owner may write.
    owner_only: HashSet<Uuid>,
    /// The caller's own profile, left out of the books they read; `None` for the owner.
    own: Option<Uuid>,
    /// A non-owner whose login is linked to no profile: records nothing.
    link_required: bool,
}

impl RecordScope {
    pub async fn load(tx: &mut TenantTx, actor: &Actor) -> Result<Self, AppError> {
        if actor.can(Permission::StaffManage) {
            return Ok(Self {
                unrestricted: true,
                owner_only: HashSet::new(),
                own: None,
                link_required: false,
            });
        }
        let permissions = org::profile::supervising_permissions();
        let mut owner_only: HashSet<Uuid> = repo::supervising_staff(tx, &permissions)
            .await?
            .into_iter()
            .collect();
        let own = org::profile::own_staff_id(tx, actor).await?;
        owner_only.extend(own);
        Ok(Self {
            unrestricted: false,
            owner_only,
            own,
            link_required: own.is_none(),
        })
    }

    /// Whether the caller may write this person's records (the UI hides the controls otherwise).
    pub fn may_record(&self, staff_id: Uuid) -> bool {
        self.unrestricted || (!self.link_required && !self.owner_only.contains(&staff_id))
    }

    /// The caller must have their login linked to a profile before recording (the sheets say so,
    /// so the UI can tell them to ask the owner).
    pub const fn link_required(&self) -> bool {
        self.link_required
    }

    /// Whether this person appears in the books the caller reads (attendance, daily reviews,
    /// evaluations and their history, bonuses / penalties): everyone but the caller's own
    /// profile; the owner sees everyone.
    pub fn lists(&self, staff_id: Uuid) -> bool {
        self.own != Some(staff_id)
    }

    /// `403 staff.link_required` when the caller is not linked to a profile, else
    /// `403 staff.record_owner_only` when any of `staff` is outside the caller's scope.
    pub fn require(&self, staff: impl IntoIterator<Item = Uuid>) -> Result<(), AppError> {
        if self.link_required {
            return Err(AppError::Forbidden {
                code: LINK_REQUIRED,
            });
        }
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
            link_required: false,
        };
        assert!(owner.require([manager, other_manager, technician]).is_ok());
        assert!(owner.lists(manager) && owner.lists(technician));
        let scope = RecordScope {
            unrestricted: false,
            owner_only: [manager, other_manager].into_iter().collect(),
            own: Some(manager),
            link_required: false,
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

    #[test]
    fn an_unlinked_manager_records_nobody() {
        let technician = Uuid::from_u128(3);
        let scope = RecordScope {
            unrestricted: false,
            owner_only: HashSet::new(),
            own: None,
            link_required: true,
        };
        assert!(!scope.may_record(technician));
        assert!(matches!(
            scope.require([technician]),
            Err(AppError::Forbidden {
                code: LINK_REQUIRED
            })
        ));
        assert!(scope.lists(technician), "still reads the books");
    }
}
