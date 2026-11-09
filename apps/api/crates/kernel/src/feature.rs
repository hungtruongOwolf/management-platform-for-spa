//! Per-spa feature switches (ADR-0018 D4, spec platform-admin US5). A feature is a set of
//! permissions and a set of routes: when a spa has it off, its permissions are stripped from every
//! member's effective permissions (so embedded parts vanish and `/me` hides it), and its routes
//! answer `403 feature.disabled` before any handler runs. All features are on unless a row in
//! `app.tenant_feature` turns one off; only a platform operator writes those rows.

use crate::permission::Permission;

macro_rules! features {
    ($($variant:ident => $key:literal, $name:literal, [$($perm:ident),*], [$($route:literal),*]),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Feature { $($variant),+ }

        impl Feature {
            pub const ALL: &'static [Feature] = &[$(Feature::$variant),+];

            pub const fn key(self) -> &'static str {
                match self { $(Feature::$variant => $key),+ }
            }

            /// Vietnamese name shown in the console.
            pub const fn name(self) -> &'static str {
                match self { $(Feature::$variant => $name),+ }
            }

            /// Permissions a spa loses while the feature is off.
            pub const fn permissions(self) -> &'static [Permission] {
                match self { $(Feature::$variant => &[$(Permission::$perm),*]),+ }
            }

            /// Route prefixes (as mounted, `/api/v1/…`) refused while the feature is off.
            pub const fn routes(self) -> &'static [&'static str] {
                match self { $(Feature::$variant => &[$($route),*]),+ }
            }

            pub fn from_key(key: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|f| f.key() == key)
            }
        }
    };
}

features! {
    Appointments => "appointments", "Lịch hẹn",
        [AppointmentManage, AppointmentSettings],
        ["/api/v1/appointments", "/api/v1/appointment-settings", "/api/v1/me/appointments"],
    Payroll => "payroll", "Bảng lương",
        [PayrollRead, PayrollClose],
        ["/api/v1/payroll"],
    // Monthly evaluations only: `evaluation.record` also keeps the daily reviews and the
    // bonuses / penalties, which stay on.
    Evaluations => "evaluations", "Đánh giá tháng",
        [],
        ["/api/v1/evaluations", "/api/v1/me/evaluations"],
    FollowUps => "follow_ups", "Gọi lại khách vắng",
        [CustomerFollowUp],
        [],
    NotesJournal => "notes_journal", "Nhật ký ghi chú",
        [],
        ["/api/v1/notes"],
    Reports => "reports", "Báo cáo kinh doanh",
        [ReportLongRange],
        ["/api/v1/reports/range", "/api/v1/reports/ledger"],
}

/// `path` is `prefix` or below it (a segment boundary, so `/notes` never matches `/notesx`).
fn under(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The feature that owns a mounted route (its matched path, e.g. `/api/v1/appointments/{id}`).
pub fn for_route(path: &str) -> Option<Feature> {
    Feature::ALL
        .iter()
        .copied()
        .find(|f| f.routes().iter().any(|prefix| under(path, prefix)))
}

/// The feature whose being off removes `permission`, if any.
pub fn owning(permission: Permission) -> Option<Feature> {
    Feature::ALL
        .iter()
        .copied()
        .find(|f| f.permissions().contains(&permission))
}

/// `permissions` without those of the switched-off features.
pub fn strip(
    permissions: Vec<Permission>,
    disabled: &std::collections::BTreeSet<Feature>,
) -> Vec<Permission> {
    permissions
        .into_iter()
        .filter(|p| owning(*p).is_none_or(|f| !disabled.contains(&f)))
        .collect()
}

/// Keys read from the database; unknown ones (a feature since removed from code) are ignored.
pub fn parse_keys(keys: &[String]) -> Vec<Feature> {
    keys.iter().filter_map(|k| Feature::from_key(k)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn keys_are_unique_and_round_trip() {
        let keys: BTreeSet<&str> = Feature::ALL.iter().map(|f| f.key()).collect();
        assert_eq!(keys.len(), Feature::ALL.len());
        for f in Feature::ALL {
            assert_eq!(Feature::from_key(f.key()), Some(*f));
        }
        assert_eq!(Feature::from_key("packages"), None);
    }

    #[test]
    fn a_permission_belongs_to_at_most_one_feature() {
        let mut seen = BTreeSet::new();
        for f in Feature::ALL {
            for p in f.permissions() {
                assert!(seen.insert(*p), "{p:?} in two features");
            }
        }
    }

    #[test]
    fn administration_is_never_a_switchable_feature() {
        for p in [
            Permission::StaffManage,
            Permission::StaffRead,
            Permission::StaffReadBasic,
            Permission::AuditRead,
            Permission::StaffReadOwnPast,
        ] {
            assert_eq!(owning(p), None, "{p:?}");
        }
    }

    #[test]
    fn routes_match_on_segment_boundaries() {
        assert_eq!(
            for_route("/api/v1/appointments"),
            Some(Feature::Appointments)
        );
        assert_eq!(
            for_route("/api/v1/appointments/{id}/arrive"),
            Some(Feature::Appointments)
        );
        assert_eq!(
            for_route("/api/v1/me/appointments"),
            Some(Feature::Appointments)
        );
        assert_eq!(
            for_route("/api/v1/payroll/payslips/{id}"),
            Some(Feature::Payroll)
        );
        assert_eq!(
            for_route("/api/v1/me/evaluations/history"),
            Some(Feature::Evaluations)
        );
        assert_eq!(for_route("/api/v1/notes"), Some(Feature::NotesJournal));
        assert_eq!(for_route("/api/v1/reports/daily"), None);
        assert_eq!(for_route("/api/v1/me/staff"), None);
        assert_eq!(for_route("/api/v1/notesx"), None);
    }
}
