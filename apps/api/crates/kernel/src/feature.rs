//! Per-spa feature switches (ADR-0018 D4, spec platform-admin US5). A feature is a set of
//! permissions and a set of routes: when a spa has it off, its permissions are stripped from every
//! member's effective permissions (so embedded parts vanish and `/me` hides it), and its routes
//! answer `403 feature.disabled` before any handler runs. A spa without a row in
//! `app.tenant_feature` for a feature gets the feature's default (`default_enabled`, ADR-0022:
//! "Nhật ký thao tác" is the first one off by default); only a platform operator writes rows.

use crate::permission::Permission;

macro_rules! features {
    ($($variant:ident => $key:literal, $name:literal, default_enabled: $default:literal, [$($perm:ident),*], [$($route:literal),*]),+ $(,)?) => {
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

            /// The state of a spa that has no row for it. Changing one changes every such spa.
            pub const fn default_enabled(self) -> bool {
                match self { $(Feature::$variant => $default),+ }
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
    Appointments => "appointments", "Lịch hẹn", default_enabled: true,
        [AppointmentManage, AppointmentSettings],
        ["/api/v1/appointments", "/api/v1/appointment-settings", "/api/v1/me/appointments",
         // A booking's history and the settings' history go with the books (ADR-0022, S12).
         "/api/v1/record-history/appointment",
         "/api/v1/record-history/settings/appointment-settings"],
    Payroll => "payroll", "Bảng lương", default_enabled: true,
        [PayrollRead, PayrollClose],
        ["/api/v1/payroll", "/api/v1/record-history/settings/payroll-rules"],
    // Monthly evaluations only: `evaluation.record` also keeps the daily reviews and the
    // bonuses / penalties, which stay on.
    Evaluations => "evaluations", "Đánh giá tháng", default_enabled: true,
        [],
        ["/api/v1/evaluations", "/api/v1/me/evaluations"],
    FollowUps => "follow_ups", "Gọi lại khách vắng", default_enabled: true,
        [CustomerFollowUp],
        [],
    NotesJournal => "notes_journal", "Nhật ký ghi chú", default_enabled: true,
        [],
        ["/api/v1/notes"],
    Reports => "reports", "Báo cáo kinh doanh", default_enabled: true,
        [ReportLongRange],
        ["/api/v1/reports/range", "/api/v1/reports/ledger"],
    // The "Nhật ký thao tác" screen, hidden by default (ADR-0022 D5): it owns the route only.
    // `audit.read` stays the owner's (owner floor), so switching it on needs no role edit.
    ActivityLog => "activity_log", "Nhật ký thao tác", default_enabled: false,
        [],
        ["/api/v1/audit-log"],
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

/// The spa's switched-off features, from its rows (`on`: keys switched on, `off`: keys switched
/// off): every row that turns one off, plus every feature off by default that no row turns on.
/// Both request paths (`app.session_actor` and `identity::repo::member_access`) use this.
pub fn disabled(on: &[String], off: &[String]) -> std::collections::BTreeSet<Feature> {
    let on = parse_keys(on);
    let mut set: std::collections::BTreeSet<Feature> = parse_keys(off).into_iter().collect();
    set.extend(
        Feature::ALL
            .iter()
            .copied()
            .filter(|f| !f.default_enabled() && !on.contains(f)),
    );
    set
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
        // ADR-0022 S12: a history route of a switched-off book is refused with it.
        assert_eq!(
            for_route("/api/v1/record-history/appointment/{id}"),
            Some(Feature::Appointments)
        );
        assert_eq!(
            for_route("/api/v1/record-history/settings/appointment-settings"),
            Some(Feature::Appointments)
        );
        assert_eq!(
            for_route("/api/v1/record-history/settings/payroll-rules"),
            Some(Feature::Payroll)
        );
        assert_eq!(for_route("/api/v1/record-history/visit/{id}"), None);
    }

    #[test]
    fn the_activity_screen_switch_owns_its_route_not_audit_read() {
        assert!(Feature::ActivityLog.permissions().is_empty());
        assert_eq!(owning(Permission::AuditRead), None);
        assert_eq!(owning(Permission::RecordHistoryRead), None);
        assert_eq!(for_route("/api/v1/audit-log"), Some(Feature::ActivityLog));
    }

    /// Changing a default changes every spa without a row: a deliberate, reviewed edit.
    #[test]
    fn the_defaults_are_pinned() {
        let defaults: Vec<(&str, bool)> = Feature::ALL
            .iter()
            .map(|f| (f.key(), f.default_enabled()))
            .collect();
        assert_eq!(
            defaults,
            [
                ("appointments", true),
                ("payroll", true),
                ("evaluations", true),
                ("follow_ups", true),
                ("notes_journal", true),
                ("reports", true),
                ("activity_log", false),
            ]
        );
    }

    #[test]
    fn disabled_is_rows_off_plus_defaults_off_without_a_row_on() {
        let keys = |k: &[&str]| k.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let set = |f: &[Feature]| f.iter().copied().collect::<BTreeSet<_>>();
        // No rows: only the features off by default.
        assert_eq!(disabled(&[], &[]), set(&[Feature::ActivityLog]));
        // A row switching it on.
        assert_eq!(disabled(&keys(&["activity_log"]), &[]), set(&[]));
        // A row switching a default-on feature off.
        assert_eq!(
            disabled(&[], &keys(&["payroll"])),
            set(&[Feature::Payroll, Feature::ActivityLog])
        );
        // A row switching the default-off feature off (an operator turned it on, then off).
        assert_eq!(
            disabled(&[], &keys(&["activity_log"])),
            set(&[Feature::ActivityLog])
        );
        // Both, and an unknown key ignored on either side.
        assert_eq!(
            disabled(&keys(&["activity_log", "gone"]), &keys(&["payroll", "gone"])),
            set(&[Feature::Payroll])
        );
    }
}
