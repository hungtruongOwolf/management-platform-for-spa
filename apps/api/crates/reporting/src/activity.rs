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
        A::VisitUpdate => &[
            ("/check_in_at", "check_in_at", F::Instant),
            ("/staff", "staff", F::Text),
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

struct Row {
    id: Uuid,
    at: DateTime<Utc>,
    day: NaiveDate,
    actor_kind: String,
    actor_user_id: Option<Uuid>,
    actor_name: Option<String>,
    action: String,
    entity_type: String,
    entity_id: Uuid,
    before: Option<Value>,
    after: Option<Value>,
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
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT a.id AS "id!", a.at AS "at!", (a.at AT TIME ZONE $2)::date AS "day!",
                  a.actor_kind AS "actor_kind!", a.actor_user_id, u.full_name AS "actor_name?",
                  a.action AS "action!", a.entity_type AS "entity_type!", a.entity_id AS "entity_id!", a.before, a.after, a.reason,
                  s.customer_id AS "customer_id?", s.customer_name AS "customer_name?",
                  s.customer_gender AS "customer_gender?", s.customer_gone AS "customer_gone?",
                  s.name AS "target_name?", s.on_date AS "on_date?", s.package AS "package?",
                  sb.full_name AS "staff_before?", sa.full_name AS "staff_after?"
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
           WHERE a.tenant_id = $1
             AND a.at >= ($3::date::timestamp AT TIME ZONE $2)
             AND a.at < (($4::date + 1)::timestamp AT TIME ZONE $2)
             AND ($5::timestamptz IS NULL OR (a.at, a.id) < ($5, $6::uuid))
             AND ($7::text IS NULL OR a.actor_kind = $7)
             AND ($8::uuid IS NULL OR a.actor_user_id = $8)
             AND ($9::text[] IS NULL OR a.action = ANY($9))
             AND ($10::text[] IS NULL OR NOT (a.action = ANY($10)))
           ORDER BY a.at DESC, a.id DESC
           LIMIT $11"#,
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
    )
    .fetch_all(tx.conn())
    .await?;
    let people = people(&mut tx).await?;
    tx.commit().await?;

    let page = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = rows.len() > page;
    let items: Vec<ActivityEntry> = rows.into_iter().take(page).map(entry).collect();
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

    #[test]
    fn money_comes_from_the_action_shape() {
        let sale_void = json!({"reversals": [{"amount": 300_000}, {"amount": 200_000}]});
        assert_eq!(
            amount(Action::SaleVoid, Some(&sale_void)),
            Some(500_000)
        );
        let cancel = json!({"refund": {"amount": 450_000, "method": "cash"}});
        assert_eq!(
            amount(Action::CustomerPackageCancel, Some(&cancel)),
            Some(450_000)
        );
        assert_eq!(
            amount(Action::CustomerPackageCancel, Some(&json!({"refund": null}))
            ),
            None
        );
        assert_eq!(
            amount(Action::VisitCorrect, Some(&json!({"difference": -50_000}))
            ),
            Some(-50_000)
        );
        assert_eq!(
            amount(Action::CustomerUpdate, Some(&json!({"total": 1}))),
            None
        );
    }
}
