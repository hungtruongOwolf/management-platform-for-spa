//! "Nhật ký thao tác" (spec management-views §11, ADR-0021, EM 2026-11-11): the spa's own audit
//! trail for the owner, newest first, one entry per thing someone did.
//!
//! Read only, over `app.audit_log`: the subject (customer, staff member, service…) is resolved by
//! join when read, so a renamed record shows its current name and an anonymised customer shows no
//! name. Each action keeps only the fields named for it here ([`fields`], deny by default): the raw
//! `before` / `after` never leave this module, so a health note, a session note or a staff phone
//! cannot reach the screen even if a writer starts storing one.
//!
//! Grouping and wording are exhaustive over [`Action`]: adding an action to the kernel does not
//! compile until it has a group here and a sentence in the web (`AuditAction` in the contract).
//!
//! One line per action (EM 2026-11-11: "một dòng thôi vì khi ghi là thu tiền luôn rồi"): the rows
//! one action writes together (a visit and the sales of its checkout, a void and the reversal of
//! its sale, a correction and the void + new visit it made) stay separate in `audit_log` and are
//! folded into one line when read ([`fold_rule`]), server-side, so a page never splits them.

use crate::activity_dto::{
    ActivityActor, ActivityChange, ActivityEntry, ActivityKind, ActivityPage, ActivityPerson,
    ActivityQuery, ActivitySubject, ActorKind, ChangeFormat, SubjectKind,
};
use crate::cursor;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use kernel::audit::Action;
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// Longest date range of one request (a year, plus a leap day).
const MAX_RANGE_DAYS: i64 = 366;
/// Default range: the last 7 days, today included.
const DEFAULT_RANGE_DAYS: i64 = 7;

fn field(name: &'static str, code: &'static str) -> AppError {
    AppError::Validation(vec![FieldError::new(name, code)])
}

/// The filter group of every action.
pub const fn kind_of(action: Action) -> ActivityKind {
    use Action as A;
    use ActivityKind as K;
    match action {
        A::VisitCreate
        | A::VisitAddition
        | A::VisitUpdate
        | A::VisitCorrect
        | A::VisitVoid
        | A::VisitConvertToPackage
        | A::SaleCreate
        | A::SaleVoid
        | A::SaleNoteUpdate => K::FrontDesk,
        A::CustomerPackageCreate
        | A::CustomerPackageExtend
        | A::CustomerPackageCancel
        | A::CustomerPackageUseExpiredOverride
        | A::OpeningBalanceImport
        | A::SalePaperBook => K::Packages,
        A::AppointmentCreate
        | A::AppointmentUpdate
        | A::AppointmentArrive
        | A::AppointmentUndoArrival
        | A::AppointmentComplete
        | A::AppointmentNoShow
        | A::AppointmentCancel => K::Appointments,
        A::CustomerCreate | A::CustomerUpdate | A::CustomerHealthNoteSet | A::CustomerFollowUp => {
            K::Customers
        }
        A::AttendanceRecord
        | A::EvaluationDailyReview
        | A::EvaluationRecord
        | A::StaffAdjustment
        | A::StaffAdjustmentVoid
        | A::StaffCreate
        | A::StaffUpdate
        | A::StaffEnd
        | A::StaffRehire => K::Staff,
        A::StaffPayUpdate | A::PayrollRulesUpdate | A::PayrollClose => K::Payroll,
        A::CatalogCategoryCreate
        | A::CatalogCategoryUpdate
        | A::CatalogServiceCreate
        | A::CatalogServiceUpdate
        | A::CatalogPlanCreate
        | A::CatalogPlanUpdate
        | A::AppointmentSettings => K::Settings,
        A::StaffAccountLink
        | A::StaffLoginCreate
        | A::StaffLoginPasswordReset
        | A::StaffLoginRole
        | A::AccountPasswordChange
        | A::AccountPasswordResetRequested
        | A::AccountPasswordResetDismissed
        | A::AccountLockedByPlatform
        | A::AccountUnlockedByPlatform
        | A::AccountMfaResetByPlatform
        | A::AccountSessionsRevokedByPlatform
        | A::AccountPasswordResetByPlatform
        | A::RolePermissionsChanged
        | A::RolePermissionsReset
        | A::FeatureChanged
        | A::TenantCreated
        | A::TenantSuspended
        | A::TenantUnsuspended
        | A::TenantOwnerMfaChanged => K::Accounts,
    }
}

/// Where a row finds the visits and sales it is about (its fold keys), in the row itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    /// The row's own `entity_id`.
    Entity,
    /// A uuid at this key of `after`.
    Field(&'static str),
    /// A list of uuids at this key of `after`.
    List(&'static str),
}

impl Link {
    const fn sql(self) -> (&'static str, &'static str) {
        match self {
            Link::Entity => ("entity", ""),
            Link::Field(key) => ("field", key),
            Link::List(key) => ("list", key),
        }
    }
}

/// How the rows of one action fold into one line: `(rank, links)`. A row is folded into a row
/// written by the same action (same instant, same person) with a higher rank that shares one of
/// its keys; a row nothing covers is a line. Rank 0 never folds and never takes a row in: every
/// action not named here is a line of its own (a package extended, a session past expiry, a
/// paper-book entry, a profile edit); so is a `customer_package.cancel` with a refund, which names
/// no sale.
///
/// What the writers store (`operations::service`, `sales`, `packages`): a checkout writes
/// `customer_package.create` per package, `sale.create` per sale, then `visit.create` /
/// `visit.addition` naming its `sale_id` and `package_sale_ids`; a void writes `sale.void` per
/// one-off sale, then `visit.void` naming its `sale_ids`; "Sửa lượt" writes that void, the new
/// visit's checkout (`corrects_visit_id` = the old visit), then `visit.correct` (`visit_id` = the
/// new one); "chuyển sang gói" all of it, then `visit.convert_to_package`; a package sale's void
/// writes `customer_package.cancel` per package (its `sale_id`), then `sale.void`.
const fn fold_rule(action: Action) -> (i32, &'static [Link]) {
    use Action as A;
    use Link as L;
    match action {
        A::VisitConvertToPackage => (5, &[L::Entity, L::Field("visit_id")]),
        A::VisitCorrect => (4, &[L::Entity, L::Field("visit_id")]),
        A::VisitCreate | A::VisitAddition => (
            3,
            &[
                L::Entity,
                L::Field("corrects_visit_id"),
                L::Field("sale_id"),
                L::List("package_sale_ids"),
            ],
        ),
        A::VisitVoid => (3, &[L::Entity, L::Field("sale_id"), L::List("sale_ids")]),
        A::SaleCreate | A::SaleVoid => (2, &[L::Entity]),
        A::CustomerPackageCreate | A::CustomerPackageCancel => (1, &[L::Field("sale_id")]),
        _ => (0, &[]),
    }
}

/// [`fold_rule`] as the query's table: one (action, rank, link, key) per link.
struct FoldTable {
    actions: Vec<String>,
    ranks: Vec<i32>,
    links: Vec<String>,
    keys: Vec<String>,
}

fn fold_table() -> FoldTable {
    let mut t = FoldTable {
        actions: Vec::new(),
        ranks: Vec::new(),
        links: Vec::new(),
        keys: Vec::new(),
    };
    for action in Action::ALL.iter().copied() {
        let (rank, links) = fold_rule(action);
        for link in links {
            let (kind, key) = link.sql();
            t.actions.push(action.as_str().to_owned());
            t.ranks.push(rank);
            t.links.push(kind.to_owned());
            t.keys.push(key.to_owned());
        }
    }
    t
}

/// For the group filter: a line whose own action is not in the group still matches through a row
/// folded into it, which has a lower rank; so only lines ranked above the lowest folding action of
/// the group need looking at (`i32::MAX`: no action of the group ever folds — "Khách hàng", …).
fn companion_floor(include: Option<&[String]>) -> i32 {
    include
        .into_iter()
        .flatten()
        .filter_map(|key| Action::parse(key))
        .map(|a| fold_rule(a).0)
        .filter(|rank| *rank > 0)
        .min()
        .unwrap_or(i32::MAX)
}

/// The fields an action shows as changed: (JSON pointer in `before` / `after`, field name,
/// format). Everything else in the row stays in the database. `customer.update` and the
/// appointment's technician are worded separately ([`customer_changes`], [`entry`]).
fn fields(action: Action) -> &'static [(&'static str, &'static str, ChangeFormat)] {
    use Action as A;
    use ChangeFormat as F;
    match action {
        A::CatalogServiceUpdate => &[
            ("/name", "name", F::Text),
            ("/duration_min", "duration_min", F::Number),
            ("/prices/all", "price", F::Money),
            ("/prices/female", "price_female", F::Money),
            ("/prices/male", "price_male", F::Money),
            ("/category_id", "category", F::Text),
            ("/recommended", "recommended", F::Flag),
            ("/active", "active", F::Flag),
        ],
        A::CatalogPlanUpdate => &[
            ("/name", "name", F::Text),
            ("/price", "price", F::Money),
            ("/validity_months", "validity_months", F::Number),
            ("/items", "items", F::Text),
            ("/active", "active", F::Flag),
        ],
        A::AppointmentSettings => &[("/max_per_slot", "max_per_slot", F::Number)],
        A::PayrollRulesUpdate => &[
            ("/free_leave_days", "free_leave_days", F::Number),
            ("/free_leave_codes", "free_leave_codes", F::Text),
            ("/default_shift_start", "default_shift_start", F::Time),
            ("/late_grace_minutes", "late_grace_minutes", F::Number),
            ("/late_penalty_amount", "late_penalty_amount", F::Money),
            (
                "/commission_on_bonus_sessions",
                "commission_on_bonus_sessions",
                F::Flag,
            ),
            ("/closing_commission", "closing_commission", F::Flag),
            ("/loyalty_partial_month", "loyalty_partial_month", F::Code),
            ("/meal_allowance_basis", "meal_allowance_basis", F::Code),
            ("/tet_level_amounts", "tet_level_amounts", F::Text),
        ],
        A::StaffPayUpdate => &[
            ("/base_salary", "base_salary", F::Money),
            ("/commission_rate_bp", "commission_rate", F::PercentBp),
            ("/loyalty_bonus_monthly", "loyalty_bonus", F::Money),
            ("/meal_allowance", "meal_allowance", F::Money),
        ],
        A::StaffUpdate => &[
            ("/full_name", "full_name", F::Text),
            ("/position", "position", F::Code),
            ("/shift_start", "shift_start", F::Time),
            ("/joined_on", "joined_on", F::Date),
            // A staff member's phone and HR note: whether one is set, never the value.
            ("/has_phone", "has_phone", F::Flag),
            ("/has_note", "has_note", F::Flag),
        ],
        A::AttendanceRecord => &[
            ("/status", "status", F::Code),
            ("/check_in", "check_in", F::Time),
            ("/check_out", "check_out", F::Time),
            ("/has_note", "has_note", F::Flag),
        ],
        A::EvaluationDailyReview => &[
            ("/compliments", "compliments", F::Number),
            ("/complaints", "complaints", F::Number),
        ],
        A::EvaluationRecord => &[
            ("/total_points", "total_points", F::Number),
            ("/result", "result", F::Code),
        ],
        A::StaffLoginRole => &[("/role", "role", F::Code)],
        A::CustomerPackageExtend => &[("/expires_on", "expires_on", F::Date)],
        A::AppointmentUpdate => &[("/starts_at", "starts_at", F::Instant)],
        // The technicians (`staff`, by name) and the session note (`session_note`, set or not)
        // are worded by [`visit_update_changes`].
        A::VisitUpdate => &[
            ("/check_in_at", "check_in_at", F::Instant),
            ("/companions", "companions", F::Number),
        ],
        A::TenantOwnerMfaChanged => &[("/owner_mfa_required", "owner_mfa_required", F::Flag)],
        A::FeatureChanged => &[("/enabled", "enabled", F::Flag)],
        A::TenantSuspended | A::TenantUnsuspended => &[("/status", "status", F::Code)],
        _ => &[],
    }
}

/// A JSON scalar as shown; `None` for null, a missing key, or an array / object (whose change is
/// still reported, without the value).
fn scalar(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// The named fields whose value differs between `before` and `after` (a missing side counts as
/// null). Arrays and objects are reported as changed without their values.
fn diff(action: Action, before: Option<&Value>, after: Option<&Value>) -> Vec<ActivityChange> {
    let pointer = |side: Option<&Value>, p: &str| side.and_then(|v| v.pointer(p)).cloned();
    fields(action)
        .iter()
        .filter_map(|(path, name, format)| {
            let old = pointer(before, path).filter(|v| !v.is_null());
            let new = pointer(after, path).filter(|v| !v.is_null());
            // A field absent from `after` was not part of this change (`visit.update` writes
            // only what it touched).
            if old == new || (new.is_none() && after.is_some_and(|a| a.pointer(path).is_none())) {
                return None;
            }
            let opaque =
                |v: &Option<Value>| v.as_ref().is_some_and(|v| v.is_array() || v.is_object());
            let hide_ids = *name == "category";
            Some(ActivityChange {
                field: (*name).to_owned(),
                format: *format,
                old: if opaque(&old) || hide_ids {
                    None
                } else {
                    scalar(old.as_ref())
                },
                new: if opaque(&new) || hide_ids {
                    None
                } else {
                    scalar(new.as_ref())
                },
            })
        })
        .collect()
}

/// `customer.update`: every field it changed (`after.changed`), with old → new only where the
/// trail keeps them: the full name and phone since ADR-0021, the gender (always a presence flag
/// set). Rows written before ADR-0021 name the fields only.
fn customer_changes(before: Option<&Value>, after: Option<&Value>) -> Vec<ActivityChange> {
    let Some(changed) = after
        .and_then(|a| a.get("changed"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    changed
        .iter()
        .filter_map(Value::as_str)
        .map(|f| {
            let (format, keeps) = match f {
                "full_name" => (ChangeFormat::Text, true),
                "phone" => (ChangeFormat::Phone, true),
                "gender" => (ChangeFormat::Code, true),
                _ => (ChangeFormat::Text, false),
            };
            let value =
                |side: Option<&Value>| keeps.then(|| scalar(side.and_then(|s| s.get(f)))).flatten();
            ActivityChange {
                field: f.to_owned(),
                format,
                old: value(before),
                new: value(after),
            }
        })
        .collect()
}

/// `visit.update` beyond [`fields`] (spec record-history S2, S3): one `staff` change per line whose
/// technician changed, by name (`moves`: the resolved `(from, to)` of exactly those lines; the
/// writer stores every line it was sent, unchanged ones included), and the session note as a
/// presence flag `session_note` (the writer stores `"[redacted]"` when set, `"cleared"` when
/// removed, null when untouched; never the text).
fn visit_update_changes(after: Option<&Value>, moves: &[(Option<String>, Option<String>)]) -> Vec<ActivityChange> {
    let mut changes: Vec<ActivityChange> = moves
        .iter()
        .map(|(from, to)| ActivityChange {
            field: "staff".to_owned(),
            format: ChangeFormat::Text,
            old: from.clone(),
            new: to.clone(),
        })
        .collect();
    let note = after
        .and_then(|a| a.get("note"))
        .and_then(Value::as_str)
        .map(|n| n != "cleared");
    if let Some(set) = note {
        changes.push(ActivityChange {
            field: "session_note".to_owned(),
            format: ChangeFormat::Flag,
            old: None,
            new: Some(set.to_string()),
        });
    }
    changes
}

fn int(v: Option<&Value>) -> Option<i64> {
    v.and_then(Value::as_i64)
}

/// The money an action moved (see [`ActivityEntry::amount`]).
fn amount(action: Action, after: Option<&Value>) -> Option<i64> {
    use Action as A;
    let at = |key: &str| int(after.and_then(|a| a.get(key)));
    match action {
        A::VisitCreate | A::VisitAddition | A::SaleCreate => at("total"),
        A::VisitVoid => at("reversed").filter(|n| *n != 0),
        A::SaleVoid => {
            let reversals = after?.get("reversals")?.as_array()?;
            let sum: i64 = reversals.iter().filter_map(|r| int(r.get("amount"))).sum();
            (sum != 0).then_some(sum)
        }
        A::VisitCorrect | A::VisitConvertToPackage => at("difference"),
        A::CustomerPackageCreate => at("sold_price"),
        A::CustomerPackageCancel => int(after?.get("refund")?.get("amount")),
        A::SalePaperBook | A::StaffAdjustment => at("amount"),
        A::PayrollClose => at("net_pay"),
        _ => None,
    }
}

fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::to_owned)
}

/// Extra context by action (see [`ActivityEntry::detail`]).
fn detail(action: Action, after: Option<&Value>) -> Option<String> {
    use Action as A;
    let at = |key: &str| after.and_then(|a| a.get(key));
    match action {
        A::StaffAdjustment => text(at("kind")),
        A::PayrollClose => text(at("month")),
        A::EvaluationRecord => text(at("month")).map(|m| m.chars().take(7).collect()),
        A::FeatureChanged => text(at("feature")),
        A::CustomerFollowUp => text(at("outcome")),
        A::StaffLoginCreate => text(at("role")),
        A::VisitCreate => at("backdated")
            .and_then(Value::as_bool)
            .filter(|b| *b)
            .map(|_| "backdated".to_owned()),
        A::AttendanceRecord => at("cleared")
            .and_then(Value::as_bool)
            .filter(|b| *b)
            .map(|_| "cleared".to_owned()),
        _ => None,
    }
}

/// The day a staff-book entry is about (it is filed on the staff member, not on the day).
fn day_about(action: Action, before: Option<&Value>, after: Option<&Value>) -> Option<NaiveDate> {
    use Action as A;
    let date = |side: Option<&Value>, key: &str| {
        side.and_then(|s| s.get(key))
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok())
    };
    match action {
        A::AttendanceRecord | A::EvaluationDailyReview => {
            date(after, "date").or_else(|| date(before, "date"))
        }
        A::StaffAdjustment => date(after, "occurred_on"),
        A::StaffAdjustmentVoid => date(before, "occurred_on"),
        _ => None,
    }
}

const fn subject_kind(entity_type: &str) -> Option<SubjectKind> {
    match entity_type.as_bytes() {
        b"customer" | b"visit" | b"sale" | b"customer_package" | b"appointment" => {
            Some(SubjectKind::Customer)
        }
        b"staff" => Some(SubjectKind::Staff),
        b"service" => Some(SubjectKind::Service),
        b"service_category" => Some(SubjectKind::ServiceCategory),
        b"package_plan" => Some(SubjectKind::PackagePlan),
        b"user" => Some(SubjectKind::Account),
        b"role" => Some(SubjectKind::Role),
        b"tenant" => Some(SubjectKind::Spa),
        _ => None,
    }
}

/// `actor` → (actor_kind, user id).
fn actor_filter(raw: Option<&str>) -> Result<(Option<&'static str>, Option<Uuid>), AppError> {
    match raw.map(str::trim) {
        None | Some("") => Ok((None, None)),
        Some("platform") => Ok((Some("platform_admin"), None)),
        Some("system") => Ok((Some("system"), None)),
        Some(id) => Uuid::parse_str(id)
            .map(|id| (Some("user"), Some(id)))
            .map_err(|_| field("actor", "invalid")),
    }
}

/// `kind` → (actions to include, actions to exclude); `other` = every key no code writes.
fn kind_filter(kind: Option<ActivityKind>) -> (Option<Vec<String>>, Option<Vec<String>>) {
    let keys = |keep: &dyn Fn(Action) -> bool| {
        Action::ALL
            .iter()
            .copied()
            .filter(|a| keep(*a))
            .map(|a| a.as_str().to_owned())
            .collect::<Vec<_>>()
    };
    match kind {
        None => (None, None),
        Some(ActivityKind::Other) => (None, Some(keys(&|_| true))),
        Some(k) => (Some(keys(&|a| kind_of(a) == k)), None),
    }
}

fn range(
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    today: NaiveDate,
) -> Result<(NaiveDate, NaiveDate), AppError> {
    let to = to.unwrap_or(today);
    let from = from.unwrap_or(to - Duration::days(DEFAULT_RANGE_DAYS - 1));
    if from > to {
        return Err(field("from", "after_to"));
    }
    if (to - from).num_days() >= MAX_RANGE_DAYS {
        return Err(field("to", "range_too_long"));
    }
    Ok((from, to))
}

/// One audit row as [`rows_by_ids`] reads it: the row, the names it is about, and its fold rank
/// and keys ([`fold_rule`]).
pub(crate) struct Row {
    pub(crate) id: Uuid,
    pub(crate) at: DateTime<Utc>,
    day: NaiveDate,
    actor_kind: String,
    actor_user_id: Option<Uuid>,
    actor_name: Option<String>,
    /// The person no longer works here (same rule as the person filter, [`people`]).
    actor_left: bool,
    pub(crate) action: String,
    pub(crate) entity_type: String,
    pub(crate) entity_id: Uuid,
    before: Option<Value>,
    pub(crate) after: Option<Value>,
    reason: Option<String>,
    customer_id: Option<Uuid>,
    customer_name: Option<String>,
    customer_gender: Option<String>,
    customer_gone: Option<bool>,
    target_name: Option<String>,
    on_date: Option<NaiveDate>,
    package: Option<String>,
    staff_before: Option<String>,
    staff_after: Option<String>,
    /// `visit.update`: the technicians of the lines that changed, by name (`""` = none), in order.
    staff_from: Vec<String>,
    staff_to: Vec<String>,
    /// A line of the page; otherwise a row of the same instant that may fold into one.
    pub(crate) root: bool,
    pub(crate) fold_rank: i32,
    fold_keys: Vec<String>,
}

impl Row {
    fn same_moment(&self, other: &Row) -> bool {
        self.at == other.at
            && self.actor_kind == other.actor_kind
            && self.actor_user_id == other.actor_user_id
    }

    /// `self` folds into `over`: same action, lower rank, a shared visit or sale.
    pub(crate) fn folds_into(&self, over: &Row) -> bool {
        self.fold_rank > 0
            && self.fold_rank < over.fold_rank
            && self.same_moment(over)
            && self.fold_keys.iter().any(|k| over.fold_keys.contains(k))
    }
}

/// The page's lines, in order, each with the rows folded into it (in the order they were
/// written). A row that folds into no line of this page belongs to a line of the next one.
pub(crate) fn fold(rows: Vec<Row>, lines: usize) -> Vec<(Row, Vec<Row>)> {
    let (roots, mut rest): (Vec<Row>, Vec<Row>) = rows.into_iter().partition(|r| r.root);
    rest.sort_by_key(|r| r.id);
    roots
        .into_iter()
        .take(lines)
        .map(|root| {
            let mut group: Vec<Row> = Vec::new();
            // Down the chain: a correction takes its void and its new visit, which take their
            // sales, which take their packages.
            while let Some(i) = rest
                .iter()
                .position(|c| c.folds_into(&root) || group.iter().any(|g| c.folds_into(g)))
            {
                group.push(rest.remove(i));
            }
            group.sort_by_key(|r| r.id);
            (root, group)
        })
        .collect()
}

/// One line: the entry of the row that is the line, with what the rows folded into it add —
/// a checkout's money is every sale it made (one-off services and packages), the packages sold or
/// cancelled are named, a discount reason on a package sale shows when the visit has none.
pub(crate) fn line(root: Row, folded: &[Row]) -> ActivityEntry {
    let action = Action::parse(&root.action);
    let mut e = entry(root);
    let sales: Vec<&Value> = folded
        .iter()
        .filter(|r| Action::parse(&r.action) == Some(Action::SaleCreate))
        .filter_map(|r| r.after.as_ref())
        .collect();
    if matches!(action, Some(Action::VisitCreate | Action::VisitAddition)) && !sales.is_empty() {
        let sum = |key: &str| -> i64 { sales.iter().filter_map(|s| int(s.get(key))).sum() };
        e.amount = Some(sum("total"));
        e.discount = Some(sum("discount_amount")).filter(|d| *d > 0);
    }
    if e.reason.is_none() {
        e.reason = folded
            .iter()
            .filter(|r| r.entity_type == "sale")
            .filter_map(|r| r.reason.as_deref())
            .find(|s| !s.trim().is_empty())
            .map(str::to_owned);
    }
    for r in folded {
        if r.entity_type == "customer_package"
            && let Some(name) = &r.package
            && !e.packages.contains(name)
        {
            e.packages.push(name.clone());
        }
    }
    e.folded = folded.iter().map(|r| r.action.clone()).collect();
    e
}

fn entry(r: Row) -> ActivityEntry {
    let action = Action::parse(&r.action);
    let (before, after) = (r.before.as_ref(), r.after.as_ref());
    let actor = ActivityActor {
        kind: match r.actor_kind.as_str() {
            "user" => ActorKind::User,
            "platform_admin" => ActorKind::Platform,
            _ => ActorKind::System,
        },
        // An operator's id is not a spa login: never sent.
        user_id: r.actor_user_id.filter(|_| r.actor_kind == "user"),
        name: r.actor_name.filter(|_| r.actor_kind == "user"),
        left: (r.actor_kind == "user" && r.actor_left).then_some(true),
    };
    let subject = subject_kind(&r.entity_type).map(|kind| {
        let gone = r.customer_gone.unwrap_or(false);
        let (id, name) = match kind {
            // A guest booking has a name and no profile; an anonymised profile has neither.
            SubjectKind::Customer if gone => (None, None),
            SubjectKind::Customer => (
                r.customer_id,
                r.customer_name
                    .clone()
                    .or_else(|| r.customer_id.map(|_| "Khách vãng lai".to_owned())),
            ),
            SubjectKind::Staff => (
                r.target_name.as_ref().map(|_| r.entity_id),
                r.target_name.clone(),
            ),
            SubjectKind::Spa => (None, None),
            _ => (None, r.target_name.clone()),
        };
        ActivitySubject {
            kind,
            id,
            name,
            gender: r
                .customer_gender
                .clone()
                .filter(|_| kind == SubjectKind::Customer && !gone),
            on: r
                .on_date
                .or_else(|| action.and_then(|a| day_about(a, before, after))),
            package: r.package.clone(),
        }
    });
    let mut changes = match action {
        // An anonymised customer's old / new name and phone stay hidden even before
        // anonymisation learns to scrub them from the trail (ADR-0021).
        Some(Action::CustomerUpdate) if r.customer_gone == Some(true) => {
            customer_changes(before, after)
                .into_iter()
                .map(|c| ActivityChange {
                    old: None,
                    new: None,
                    ..c
                })
                .collect()
        }
        Some(Action::CustomerUpdate) => customer_changes(before, after),
        Some(Action::VisitUpdate) => {
            let name = |n: &String| Some(n.clone()).filter(|n| !n.is_empty());
            let moves: Vec<(Option<String>, Option<String>)> = r
                .staff_from
                .iter()
                .zip(&r.staff_to)
                .map(|(from, to)| (name(from), name(to)))
                .collect();
            let mut changes = diff(Action::VisitUpdate, before, after);
            changes.extend(visit_update_changes(after, &moves));
            changes
        }
        Some(a) => diff(a, before, after),
        None => Vec::new(),
    };
    // The booking's technician, by name (the ids are resolved by the query).
    if matches!(
        action,
        Some(Action::AppointmentUpdate | Action::AppointmentArrive)
    ) && r.staff_before != r.staff_after
        && after.is_some_and(|a| a.get("staff_id").is_some())
    {
        changes.push(ActivityChange {
            field: "staff".to_owned(),
            format: ChangeFormat::Text,
            old: r.staff_before.clone(),
            new: r.staff_after.clone(),
        });
    }
    ActivityEntry {
        id: r.id,
        at: r.at,
        day: r.day,
        actor,
        kind: action.map_or(ActivityKind::Other, kind_of),
        action: r.action,
        subject,
        changes,
        amount: action.and_then(|a| amount(a, after)),
        discount: matches!(
            action,
            Some(Action::VisitCreate | Action::VisitAddition | Action::SaleCreate)
        )
        .then(|| int(after.and_then(|a| a.get("discount_amount"))))
        .flatten()
        .filter(|d| *d > 0),
        detail: action.and_then(|a| detail(a, after)),
        // `customer.update` never asks for a reason: what it stores is the code's own note.
        reason: r
            .reason
            .filter(|_| action != Some(Action::CustomerUpdate))
            .filter(|s| !s.trim().is_empty()),
        packages: Vec::new(),
        folded: Vec::new(),
    }
}

/// `GET /audit-log` (`audit.read`, owner only): one page of the spa's trail, newest first.
pub async fn list(
    pool: &PgPool,
    actor: &Actor,
    query: &ActivityQuery,
) -> Result<ActivityPage, AppError> {
    actor.require(Permission::AuditRead)?;
    let limit = cursor::limit(query.limit)?;
    let after = match query.cursor.as_deref() {
        Some(raw) => {
            let p = cursor::decode(raw, 2)?;
            Some((cursor::instant(&p[0])?, cursor::uuid(&p[1])?))
        }
        None => None,
    };
    let (actor_kind, actor_user) = actor_filter(query.actor.as_deref())?;
    let (include, exclude) = kind_filter(query.kind);

    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let profile = kernel::tenant::profile(&mut tx).await?;
    let (from, to) = range(query.from, query.to, profile.business_date.as_naive())?;
    let tz = profile.timezone.name();
    let tenant = tx.tenant().as_uuid();
    let fold_by = fold_table();
    // `k`: every row of the spa with its fold rank and keys ([`fold_rule`]); inlined, so each use
    // reads only the instant it asks for through the (tenant, at, id) index.
    // `page`: the lines — rows nothing of the same action covers — matching the filters, a group
    // matching a line by any row folded into it (a package sold in a visit's checkout; a chain is
    // at most two folds deep; `hits`, the instants holding a row of the group, keeps a rare group
    // over a long range from probing every row). `lines`: the page plus the rows of the same
    // instants that may fold into it; `fold` attaches them.
    let lines = sqlx::query!(
        r#"WITH fold AS (
             SELECT f.action, f.rank, f.link, f.key
             FROM unnest($12::text[], $13::int4[], $14::text[], $15::text[]) AS f(action, rank, link, key)
           ),
           k AS NOT MATERIALIZED (
             SELECT a.id, a.at, a.actor_kind, a.actor_user_id, a.action,
                    coalesce((SELECT max(f.rank) FROM fold f WHERE f.action = a.action), 0) AS rank,
                    ARRAY(
                      SELECT l.v FROM fold f
                      CROSS JOIN LATERAL (
                        SELECT a.entity_id::text WHERE f.link = 'entity'
                        UNION ALL
                        SELECT a.after ->> f.key WHERE f.link = 'field'
                        UNION ALL
                        SELECT e FROM jsonb_array_elements_text(
                          CASE WHEN f.link = 'list' AND jsonb_typeof(a.after -> f.key) = 'array'
                               THEN a.after -> f.key ELSE '[]'::jsonb END) e
                      ) l(v)
                      WHERE f.action = a.action AND l.v IS NOT NULL
                    ) AS keys
             FROM app.audit_log a
             WHERE a.tenant_id = $1
           ),
           hits AS MATERIALIZED (
             SELECT DISTINCT h.at, h.actor_kind,
                    coalesce(h.actor_user_id, '00000000-0000-0000-0000-000000000000'::uuid) AS actor
             FROM app.audit_log h
             WHERE h.tenant_id = $1 AND h.action = ANY($9)
               AND h.at >= ($3::date::timestamp AT TIME ZONE $2)
               AND h.at < (($4::date + 1)::timestamp AT TIME ZONE $2)
           ),
           page AS (
             SELECT a.id, a.at, a.actor_kind, a.actor_user_id, a.rank, a.keys
             FROM k a
             WHERE a.at >= ($3::date::timestamp AT TIME ZONE $2)
               AND a.at < (($4::date + 1)::timestamp AT TIME ZONE $2)
               AND ($5::timestamptz IS NULL OR (a.at, a.id) < ($5, $6::uuid))
               AND ($7::text IS NULL OR a.actor_kind = $7)
               AND ($8::uuid IS NULL OR a.actor_user_id = $8)
               AND ($10::text[] IS NULL OR NOT (a.action = ANY($10)))
               -- CASE fixes the order: the group first (cheap), then whether the row is a line.
               AND CASE
                 WHEN NOT ($9::text[] IS NULL OR a.action = ANY($9) OR (
                   a.rank > $16::int4
                   AND (a.at, a.actor_kind,
                        coalesce(a.actor_user_id, '00000000-0000-0000-0000-000000000000'::uuid))
                     IN (SELECT h.at, h.actor_kind, h.actor FROM hits h)
                   AND EXISTS (
                     SELECT 1 FROM k c
                     WHERE c.at = a.at AND c.actor_kind = a.actor_kind
                       AND c.actor_user_id IS NOT DISTINCT FROM a.actor_user_id
                       AND c.rank > 0 AND c.rank < a.rank AND c.action = ANY($9)
                       AND (c.keys && a.keys OR EXISTS (
                         SELECT 1 FROM k m
                         WHERE m.at = a.at AND m.actor_kind = a.actor_kind
                           AND m.actor_user_id IS NOT DISTINCT FROM a.actor_user_id
                           AND m.rank > c.rank AND m.rank < a.rank
                           AND m.keys && a.keys AND m.keys && c.keys)))))
                 THEN false
                 ELSE a.rank = 0 OR NOT EXISTS (
                   SELECT 1 FROM k p
                   WHERE p.at = a.at AND p.actor_kind = a.actor_kind
                     AND p.actor_user_id IS NOT DISTINCT FROM a.actor_user_id
                     AND p.rank > a.rank AND p.keys && a.keys)
               END
             ORDER BY a.at DESC, a.id DESC
             LIMIT $11
           ),
           lines AS (
             SELECT p.id, true AS root, p.rank, p.keys FROM page p
             UNION
             SELECT c.id, false, c.rank, c.keys
             FROM page p
             JOIN k c ON c.at = p.at AND c.actor_kind = p.actor_kind
               AND c.actor_user_id IS NOT DISTINCT FROM p.actor_user_id
               AND c.rank > 0 AND c.rank < p.rank
             WHERE c.id NOT IN (SELECT id FROM page)
           )
           SELECT l.id AS "id!", l.root AS "root!" FROM lines l"#,
        tenant,
        tz,
        from,
        to,
        after.map(|(t, _)| t),
        after.map(|(_, id)| id),
        actor_kind,
        actor_user,
        include.as_deref(),
        exclude.as_deref(),
        limit + 1,
        &fold_by.actions,
        &fold_by.ranks,
        &fold_by.links,
        &fold_by.keys,
        companion_floor(include.as_deref()),
    )
    .fetch_all(tx.conn())
    .await?;
    let ids: Vec<Uuid> = lines.iter().map(|l| l.id).collect();
    let mut rows = rows_by_ids(&mut tx, tz, &ids).await?;
    for row in &mut rows {
        row.root = lines.iter().any(|l| l.id == row.id && l.root);
    }
    let people = people(&mut tx).await?;
    tx.commit().await?;

    let page = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = rows.iter().filter(|r| r.root).count() > page;
    let items: Vec<ActivityEntry> = fold(rows, page)
        .into_iter()
        .map(|(root, folded)| line(root, &folded))
        .collect();
    let next_cursor = more
        .then(|| items.last())
        .flatten()
        .map(|e| cursor::encode(&[cursor::instant_part(e.at), e.id.to_string()]));
    Ok(ActivityPage {
        from,
        to,
        items,
        next_cursor,
        people,
    })
}

/// The audit rows `ids` of the transaction's spa, newest first, with the names they are about
/// (resolved now: a renamed record shows its current name, an anonymised customer none) and their
/// fold rank and keys; `root` is false (the caller decides). Shared by the trail
/// ([`list`]) and each record's history (`crate::history`), so both read a row the same way.
pub(crate) async fn rows_by_ids(
    tx: &mut TenantTx,
    tz: &str,
    ids: &[Uuid],
) -> Result<Vec<Row>, AppError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let tenant = tx.tenant().as_uuid();
    let fold_by = fold_table();
    let rows = sqlx::query_as!(
        Row,
        r#"WITH fold AS (
             SELECT f.action, f.rank, f.link, f.key
             FROM unnest($4::text[], $5::int4[], $6::text[], $7::text[]) AS f(action, rank, link, key)
           )
           SELECT a.id AS "id!", a.at AS "at!", (a.at AT TIME ZONE $2)::date AS "day!",
                  a.actor_kind AS "actor_kind!", a.actor_user_id, u.full_name AS "actor_name?",
                  (a.actor_kind = 'user' AND (
                     EXISTS (SELECT 1 FROM app.membership m
                             WHERE m.tenant_id = a.tenant_id AND m.user_id = a.actor_user_id
                               AND m.status <> 'active')
                     OR EXISTS (SELECT 1 FROM app.staff st
                                WHERE st.tenant_id = a.tenant_id AND st.user_id = a.actor_user_id
                                  AND NOT EXISTS (
                                    SELECT 1 FROM app.staff_employment e
                                    WHERE e.tenant_id = st.tenant_id AND e.staff_id = st.id
                                      AND e.ended_on IS NULL)))) AS "actor_left!",
                  a.action AS "action!", a.entity_type AS "entity_type!", a.entity_id AS "entity_id!", a.before, a.after, a.reason,
                  s.customer_id AS "customer_id?", s.customer_name AS "customer_name?",
                  s.customer_gender AS "customer_gender?", s.customer_gone AS "customer_gone?",
                  s.name AS "target_name?", s.on_date AS "on_date?", s.package AS "package?",
                  sb.full_name AS "staff_before?", sa.full_name AS "staff_after?",
                  ARRAY(SELECT coalesce(sf.full_name, '')
                        FROM jsonb_array_elements(
                          CASE WHEN a.action = 'visit.update' AND jsonb_typeof(a.after -> 'staff') = 'array'
                               THEN a.after -> 'staff' ELSE '[]'::jsonb END) WITH ORDINALITY e(v, n)
                        LEFT JOIN app.staff sf ON sf.tenant_id = a.tenant_id
                          AND sf.id = (e.v ->> 'from')::uuid
                        WHERE (e.v ->> 'from') IS DISTINCT FROM (e.v ->> 'to')
                        ORDER BY e.n) AS "staff_from!",
                  ARRAY(SELECT coalesce(sf.full_name, '')
                        FROM jsonb_array_elements(
                          CASE WHEN a.action = 'visit.update' AND jsonb_typeof(a.after -> 'staff') = 'array'
                               THEN a.after -> 'staff' ELSE '[]'::jsonb END) WITH ORDINALITY e(v, n)
                        LEFT JOIN app.staff sf ON sf.tenant_id = a.tenant_id
                          AND sf.id = (e.v ->> 'to')::uuid
                        WHERE (e.v ->> 'from') IS DISTINCT FROM (e.v ->> 'to')
                        ORDER BY e.n) AS "staff_to!",
                  false AS "root!",
                  coalesce((SELECT max(f.rank) FROM fold f WHERE f.action = a.action), 0) AS "fold_rank!",
                  ARRAY(
                    SELECT l.v FROM fold f
                    CROSS JOIN LATERAL (
                      SELECT a.entity_id::text WHERE f.link = 'entity'
                      UNION ALL
                      SELECT a.after ->> f.key WHERE f.link = 'field'
                      UNION ALL
                      SELECT e FROM jsonb_array_elements_text(
                        CASE WHEN f.link = 'list' AND jsonb_typeof(a.after -> f.key) = 'array'
                             THEN a.after -> f.key ELSE '[]'::jsonb END) e
                    ) l(v)
                    WHERE f.action = a.action AND l.v IS NOT NULL
                  ) AS "fold_keys!"
           FROM app.audit_log a
           LEFT JOIN app.user_account u ON a.actor_kind = 'user' AND u.id = a.actor_user_id
           LEFT JOIN LATERAL (
             SELECT c.id AS customer_id, c.full_name AS customer_name, c.gender AS customer_gender,
                    c.anonymized_at IS NOT NULL AS customer_gone, NULL::text AS name,
                    NULL::date AS on_date, NULL::text AS package
             FROM app.customer c
             WHERE a.entity_type = 'customer' AND c.tenant_id = a.tenant_id AND c.id = a.entity_id
             UNION ALL
             SELECT c.id, c.full_name, c.gender, c.anonymized_at IS NOT NULL, NULL, v.business_date, NULL
             FROM app.visit v
             LEFT JOIN app.customer c ON c.tenant_id = v.tenant_id AND c.id = v.customer_id
             WHERE a.entity_type = 'visit' AND v.tenant_id = a.tenant_id AND v.id = a.entity_id
             UNION ALL
             SELECT c.id, c.full_name, c.gender, c.anonymized_at IS NOT NULL, NULL, sl.business_date, NULL
             FROM app.sale sl
             LEFT JOIN app.customer c ON c.tenant_id = sl.tenant_id AND c.id = sl.customer_id
             WHERE a.entity_type = 'sale' AND sl.tenant_id = a.tenant_id AND sl.id = a.entity_id
             UNION ALL
             SELECT c.id, c.full_name, c.gender, c.anonymized_at IS NOT NULL, NULL, NULL, cp.name_snapshot
             FROM app.customer_package cp
             LEFT JOIN app.customer c ON c.tenant_id = cp.tenant_id AND c.id = cp.customer_id
             WHERE a.entity_type = 'customer_package' AND cp.tenant_id = a.tenant_id AND cp.id = a.entity_id
             UNION ALL
             SELECT c.id, coalesce(c.full_name, ap.guest_name), c.gender, c.anonymized_at IS NOT NULL,
                    NULL, ap.business_date, NULL
             FROM app.appointment ap
             LEFT JOIN app.customer c ON c.tenant_id = ap.tenant_id AND c.id = ap.customer_id
             WHERE a.entity_type = 'appointment' AND ap.tenant_id = a.tenant_id AND ap.id = a.entity_id
             UNION ALL
             SELECT NULL, NULL, NULL, NULL, st.full_name, NULL, NULL FROM app.staff st
             WHERE a.entity_type = 'staff' AND st.tenant_id = a.tenant_id AND st.id = a.entity_id
             UNION ALL
             SELECT NULL, NULL, NULL, NULL, sv.name, NULL, NULL FROM app.service sv
             WHERE a.entity_type = 'service' AND sv.tenant_id = a.tenant_id AND sv.id = a.entity_id
             UNION ALL
             SELECT NULL, NULL, NULL, NULL, pp.name, NULL, NULL FROM app.package_plan pp
             WHERE a.entity_type = 'package_plan' AND pp.tenant_id = a.tenant_id AND pp.id = a.entity_id
             UNION ALL
             SELECT NULL, NULL, NULL, NULL, sc.name, NULL, NULL FROM app.service_category sc
             WHERE a.entity_type = 'service_category' AND sc.tenant_id = a.tenant_id AND sc.id = a.entity_id
             UNION ALL
             SELECT NULL, NULL, NULL, NULL, ua.full_name, NULL, NULL FROM app.user_account ua
             WHERE a.entity_type = 'user' AND ua.id = a.entity_id
             UNION ALL
             SELECT NULL, NULL, NULL, NULL, r.key, NULL, NULL FROM app.role r
             WHERE a.entity_type = 'role' AND r.tenant_id = a.tenant_id AND r.id = a.entity_id
           ) s ON true
           -- A booking's technician before / after (ids written by the appointments crate).
           LEFT JOIN app.staff sb ON sb.tenant_id = a.tenant_id
             AND sb.id = CASE WHEN a.entity_type = 'appointment' THEN (a.before ->> 'staff_id')::uuid END
           LEFT JOIN app.staff sa ON sa.tenant_id = a.tenant_id
             AND sa.id = CASE WHEN a.entity_type = 'appointment' THEN (a.after ->> 'staff_id')::uuid END
           WHERE a.tenant_id = $1 AND a.id = ANY($3)
           ORDER BY a.at DESC, a.id DESC"#,
        tenant,
        tz,
        ids,
        &fold_by.actions,
        &fold_by.ranks,
        &fold_by.links,
        &fold_by.keys,
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}

/// Every login of the spa (the owner, staff with a login, staff who left), for the person filter.
async fn people(tx: &mut TenantTx) -> Result<Vec<ActivityPerson>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query!(
        r#"SELECT u.id AS "user_id!", u.full_name AS "name!",
                  (m.status <> 'active'
                   OR (st.id IS NOT NULL AND NOT EXISTS (
                         SELECT 1 FROM app.staff_employment e
                         WHERE e.tenant_id = st.tenant_id AND e.staff_id = st.id AND e.ended_on IS NULL))
                  ) AS "left!"
           FROM app.membership m
           JOIN app.user_account u ON u.id = m.user_id
           LEFT JOIN app.staff st ON st.tenant_id = m.tenant_id AND st.user_id = m.user_id
           WHERE m.tenant_id = $1
           ORDER BY 3, u.full_name, u.id"#,
        tenant
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ActivityPerson {
            user_id: r.user_id,
            name: r.name,
            left: r.left,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    #[test]
    fn every_group_but_other_has_actions_and_other_has_none() {
        let used: BTreeSet<ActivityKind> = Action::ALL.iter().map(|a| kind_of(*a)).collect();
        let expected: BTreeSet<ActivityKind> = ActivityKind::ALL
            .into_iter()
            .filter(|k| *k != ActivityKind::Other)
            .collect();
        assert_eq!(used, expected);
    }

    #[test]
    fn the_kind_filter_splits_known_from_legacy_keys() {
        let (include, exclude) = kind_filter(Some(ActivityKind::FrontDesk));
        let include = include.unwrap_or_default();
        assert!(include.contains(&"visit.void".to_owned()));
        assert!(!include.contains(&"customer.update".to_owned()));
        assert_eq!(exclude, None);
        let (include, exclude) = kind_filter(Some(ActivityKind::Other));
        assert_eq!(include, None);
        assert_eq!(exclude.map(|e| e.len()), Some(Action::ALL.len()));
        assert_eq!(kind_filter(None), (None, None));
    }

    #[test]
    fn the_person_filter_takes_an_id_platform_or_system() {
        let id = Uuid::now_v7();
        assert_eq!(
            actor_filter(Some(&id.to_string())).ok(),
            Some((Some("user"), Some(id)))
        );
        assert_eq!(
            actor_filter(Some("platform")).ok(),
            Some((Some("platform_admin"), None))
        );
        assert_eq!(
            actor_filter(Some("system")).ok(),
            Some((Some("system"), None))
        );
        assert_eq!(actor_filter(None).ok(), Some((None, None)));
        assert!(actor_filter(Some("0905123456")).is_err());
    }

    #[test]
    fn the_range_is_bounded() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap_or_default();
        let (from, to) = range(None, None, today).unwrap_or((today, today));
        assert_eq!((to - from).num_days(), 6);
        assert!(range(Some(today), Some(today - Duration::days(1)), today).is_err());
        assert!(range(Some(today - Duration::days(365)), Some(today), today).is_ok());
        assert!(range(Some(today - Duration::days(366)), Some(today), today).is_err());
    }

    #[test]
    fn only_named_fields_leave_and_never_a_note() {
        // A writer that one day stored a note's text would still not leak it.
        let before = json!({"status": "present", "check_in": "08:55:00", "note": "Ốm"});
        let after = json!({"status": "leave_dx", "check_in": null, "note": "Đau dạ dày"});
        let changes = diff(Action::AttendanceRecord, Some(&before), Some(&after));
        let text = format!("{changes:?}");
        assert!(!text.contains("Ốm") && !text.contains("dạ dày"), "{text}");
        assert_eq!(
            changes.iter().map(|c| c.field.as_str()).collect::<Vec<_>>(),
            ["status", "check_in"]
        );
        assert_eq!(changes[0].new.as_deref(), Some("leave_dx"));
        assert_eq!(changes[1].new, None);
    }

    #[test]
    fn a_visit_update_names_technicians_and_flags_the_session_note() {
        // S2: one change per line whose technician changed (the query resolved those names);
        // S3: the session note is `session_note`, set or not, never its text.
        let after = json!({"staff": [{"line_id": "l1", "from": "a", "to": "a"}],
                           "check_in_at": null, "companions": null, "note": "[redacted]"});
        let moves = [(Some("Thu".to_owned()), Some("Hoa".to_owned())), (None, Some("Lan".to_owned()))];
        let changes = visit_update_changes(Some(&after), &moves);
        let got: Vec<(&str, Option<&str>, Option<&str>)> = changes
            .iter()
            .map(|c| (c.field.as_str(), c.old.as_deref(), c.new.as_deref()))
            .collect();
        assert_eq!(
            got,
            [
                ("staff", Some("Thu"), Some("Hoa")),
                ("staff", None, Some("Lan")),
                ("session_note", None, Some("true")),
            ]
        );
        let cleared = visit_update_changes(Some(&json!({"note": "cleared"})), &[]);
        assert_eq!(cleared[0].new.as_deref(), Some("false"));
        assert!(visit_update_changes(Some(&json!({"note": null})), &[]).is_empty());
        // Unchanged companions / time stay out; a new companion count shows.
        let changes = diff(
            Action::VisitUpdate,
            Some(&json!({"check_in_at": null})),
            Some(&json!({"check_in_at": null, "companions": 2, "staff": [], "note": "[redacted]"})),
        );
        assert_eq!(
            changes.iter().map(|c| (c.field.as_str(), c.new.as_deref())).collect::<Vec<_>>(),
            [("companions", Some("2"))]
        );
    }

    #[test]
    fn a_customer_update_keeps_old_and_new_only_for_name_phone_and_gender() {
        let before =
            json!({"full_name": "Lê Mai", "phone": "+84905111222", "has_birth_date": false});
        let after = json!({"full_name": "Lê Thị Mai", "phone": "+84912333444", "has_birth_date": true,
                           "changed": ["full_name", "phone", "birth_date", "address"]});
        let changes = customer_changes(Some(&before), Some(&after));
        let pairs: Vec<(&str, Option<&str>, Option<&str>)> = changes
            .iter()
            .map(|c| (c.field.as_str(), c.old.as_deref(), c.new.as_deref()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("full_name", Some("Lê Mai"), Some("Lê Thị Mai")),
                ("phone", Some("+84905111222"), Some("+84912333444")),
                ("birth_date", None, None),
                ("address", None, None),
            ]
        );
    }

    /// A row of one instant and person, as the query returns it.
    fn row(action: Action, root: bool, keys: &[&str], after: Option<Value>) -> Row {
        Row {
            id: Uuid::now_v7(),
            at: DateTime::<Utc>::UNIX_EPOCH,
            day: NaiveDate::default(),
            actor_kind: "user".to_owned(),
            actor_user_id: Some(Uuid::nil()),
            actor_name: None,
            actor_left: false,
            action: action.as_str().to_owned(),
            entity_type: action
                .as_str()
                .split('.')
                .next()
                .unwrap_or_default()
                .to_owned(),
            entity_id: Uuid::nil(),
            before: None,
            after,
            reason: None,
            customer_id: None,
            customer_name: None,
            customer_gender: None,
            customer_gone: None,
            target_name: None,
            on_date: None,
            package: None,
            staff_before: None,
            staff_after: None,
            staff_from: Vec::new(),
            staff_to: Vec::new(),
            root,
            fold_rank: fold_rule(action).0,
            fold_keys: keys.iter().map(|k| (*k).to_owned()).collect(),
        }
    }

    fn actions(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|r| r.action.as_str()).collect()
    }

    #[test]
    fn a_correction_takes_its_void_its_new_visit_and_their_sales() {
        let rows = vec![
            row(Action::SaleVoid, false, &["s1"], None),
            row(Action::VisitVoid, false, &["v1", "s1"], None),
            row(Action::CustomerPackageCreate, false, &["p2"], None),
            row(Action::SaleCreate, false, &["p2"], None),
            row(Action::SaleCreate, false, &["s2"], None),
            row(Action::VisitCreate, false, &["v2", "v1", "s2", "p2"], None),
            row(Action::VisitCorrect, true, &["v1", "v2"], None),
            // Another visit recorded at the same instant by the same person: its own line.
            row(Action::SaleCreate, false, &["s3"], None),
            row(Action::VisitCreate, true, &["v3", "s3"], None),
            // A sale of the same instant no line of this page covers (its line is on the next).
            row(Action::SaleCreate, false, &["s9"], None),
        ];
        let lines = fold(rows, 10);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0.action, "visit.correct");
        assert_eq!(
            actions(&lines[0].1),
            [
                "sale.void",
                "visit.void",
                "customer_package.create",
                "sale.create",
                "sale.create",
                "visit.create"
            ]
        );
        assert_eq!(actions(&lines[1].1), ["sale.create"]);
    }

    #[test]
    fn rows_of_another_instant_or_person_never_fold() {
        let mut later = row(Action::SaleCreate, false, &["s1"], None);
        later.at += Duration::seconds(1);
        let mut by_other = row(Action::SaleCreate, false, &["s1"], None);
        by_other.actor_user_id = Some(Uuid::now_v7());
        let lines = fold(
            vec![
                row(Action::VisitCreate, true, &["v1", "s1"], None),
                later,
                by_other,
            ],
            10,
        );
        assert!(lines[0].1.is_empty());
        // A rank-0 action takes nothing in, even with a shared key.
        let lines = fold(
            vec![
                row(Action::VisitUpdate, true, &["v1"], None),
                row(Action::SaleCreate, false, &["v1"], None),
            ],
            10,
        );
        assert!(lines[0].1.is_empty());
    }

    #[test]
    fn a_checkout_line_carries_every_sale_and_names_its_packages() {
        let mut package = row(Action::CustomerPackageCreate, false, &["p1"], None);
        package.package = Some("Gội đầu 10 buổi".to_owned());
        let mut package_sale = row(
            Action::SaleCreate,
            false,
            &["p1"],
            Some(json!({"total": 1_400_000, "discount_amount": 100_000})),
        );
        package_sale.reason = Some("Khách quen".to_owned());
        let services = row(
            Action::SaleCreate,
            false,
            &["s1"],
            Some(json!({"total": 150_000, "discount_amount": 0})),
        );
        let visit = row(
            Action::VisitCreate,
            true,
            &["v1", "s1", "p1"],
            Some(json!({"total": 150_000, "discount_amount": 0})),
        );
        let lines = fold(vec![package, package_sale, services, visit], 10);
        let (root, folded) = lines.into_iter().next().unwrap_or_else(|| unreachable!());
        let e = line(root, &folded);
        assert_eq!(e.amount, Some(1_550_000));
        assert_eq!(e.discount, Some(100_000));
        assert_eq!(e.reason.as_deref(), Some("Khách quen"));
        assert_eq!(e.packages, ["Gội đầu 10 buổi"]);
        assert_eq!(
            e.folded,
            ["customer_package.create", "sale.create", "sale.create"]
        );
    }

    #[test]
    fn the_fold_table_lists_every_link_of_every_folding_action() {
        let t = fold_table();
        assert_eq!(t.actions.len(), t.ranks.len());
        assert_eq!(t.actions.len(), t.links.len());
        assert_eq!(t.actions.len(), t.keys.len());
        let rank = |a: Action| fold_rule(a).0;
        // A correction above its visits, visits above their sales, sales above their packages.
        assert!(rank(Action::VisitConvertToPackage) > rank(Action::VisitCorrect));
        assert!(rank(Action::VisitCorrect) > rank(Action::VisitCreate));
        assert_eq!(rank(Action::VisitCreate), rank(Action::VisitVoid));
        assert!(rank(Action::VisitVoid) > rank(Action::SaleVoid));
        assert!(rank(Action::SaleCreate) > rank(Action::CustomerPackageCreate));
        // Standalone actions stay lines of their own.
        for a in [
            Action::VisitUpdate,
            Action::SaleNoteUpdate,
            Action::SalePaperBook,
            Action::CustomerPackageUseExpiredOverride,
            Action::CustomerUpdate,
        ] {
            assert_eq!(fold_rule(a), (0, &[][..]), "{a:?}");
            assert!(!t.actions.contains(&a.as_str().to_owned()));
        }
    }

    #[test]
    fn only_groups_with_folding_actions_look_inside_a_line() {
        let floor = |k: ActivityKind| companion_floor(kind_filter(Some(k)).0.as_deref());
        assert_eq!(floor(ActivityKind::Packages), 1);
        assert_eq!(floor(ActivityKind::FrontDesk), 2);
        assert_eq!(floor(ActivityKind::Customers), i32::MAX);
        assert_eq!(companion_floor(None), i32::MAX);
    }

    #[test]
    fn money_comes_from_the_action_shape() {
        let sale_void = json!({"reversals": [{"amount": 300_000}, {"amount": 200_000}]});
        assert_eq!(amount(Action::SaleVoid, Some(&sale_void)), Some(500_000));
        let cancel = json!({"refund": {"amount": 450_000, "method": "cash"}});
        assert_eq!(
            amount(Action::CustomerPackageCancel, Some(&cancel)),
            Some(450_000)
        );
        assert_eq!(
            amount(
                Action::CustomerPackageCancel,
                Some(&json!({"refund": null}))
            ),
            None
        );
        assert_eq!(
            amount(Action::VisitCorrect, Some(&json!({"difference": -50_000}))),
            Some(-50_000)
        );
        assert_eq!(
            amount(Action::CustomerUpdate, Some(&json!({"total": 1}))),
            None
        );
    }
}
