//! Shapes of `GET /audit-log` ("Nhật ký thao tác", spec management-views §11, ADR-0021).
//!
//! The server sends structured entries (who, what, about whom, which values changed, how much,
//! why); the web words the sentence. Nothing here carries a raw `before` / `after`: each action
//! keeps only the fields [`crate::activity`] names for it, so a health note or a session note can
//! never reach the screen.

use chrono::{DateTime, NaiveDate, Utc};
use kernel::audit::Action;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::{ObjectBuilder, Schema, Type};
use utoipa::{IntoParams, PartialSchema, ToSchema};
use uuid::Uuid;

/// The filter groups of the screen. Every [`Action`] belongs to exactly one
/// (`crate::activity::kind_of`, an exhaustive `match`); a stored key no code writes any more is
/// `other`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    /// Quầy: visits, sales, discounts, payments, voids and corrections.
    FrontDesk,
    /// Gói liệu trình: sold, extended, used past expiry, cancelled with a refund, paper books.
    Packages,
    /// Lịch hẹn.
    Appointments,
    /// Khách hàng: created, edited, contacted.
    Customers,
    /// Nhân sự: attendance, praise / complaints, evaluations, bonuses / penalties, profiles.
    Staff,
    /// Lương: pay terms, payroll rules, month close.
    Payroll,
    /// Danh mục & cài đặt: price list, package plans, booking settings.
    Settings,
    /// Tài khoản & bảo mật: logins, passwords, roles, what the platform team did.
    Accounts,
    /// A legacy action no code writes any more.
    Other,
}

impl ActivityKind {
    pub const ALL: [ActivityKind; 9] = [
        ActivityKind::FrontDesk,
        ActivityKind::Packages,
        ActivityKind::Appointments,
        ActivityKind::Customers,
        ActivityKind::Staff,
        ActivityKind::Payroll,
        ActivityKind::Settings,
        ActivityKind::Accounts,
        ActivityKind::Other,
    ];
}

/// Every action key the code writes, as one string enum in the contract, so the web's sentence
/// table is checked against it at compile time (`Record<AuditAction, …>`). An entry's `action`
/// stays a plain string: rows written by older code may hold a key that is not listed.
pub struct AuditAction;

impl PartialSchema for AuditAction {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .enum_values(Some(Action::ALL.iter().map(|a| a.as_str())))
            .into()
    }
}

impl ToSchema for AuditAction {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("AuditAction")
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ActivityQuery {
    /// First day (spa time zone, inclusive). Default: 6 days before `to`.
    pub from: Option<NaiveDate>,
    /// Last day (inclusive). Default: today. At most 366 days after `from`.
    pub to: Option<NaiveDate>,
    /// Who did it: a member's user id, `platform` (the platform team) or `system`.
    pub actor: Option<String>,
    /// Only this group of actions.
    pub kind: Option<ActivityKind>,
    pub cursor: Option<String>,
    /// 1–100, default 20.
    pub limit: Option<u32>,
}

/// Who did it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    /// A login of the spa (`user_id`, `name`).
    User,
    /// The platform team ("Đội vận hành"): never named.
    Platform,
    /// Nobody signed in (a forgotten-password request from the login screen).
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ActivityActor {
    pub kind: ActorKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Uuid>,
    /// The login's full name (`user` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// What an entry is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    Customer,
    Staff,
    Service,
    ServiceCategory,
    PackagePlan,
    Account,
    Role,
    Spa,
}

/// The record an entry is about, resolved when read (a rename shows the current name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ActivitySubject {
    pub kind: SubjectKind,
    /// A customer's or a staff member's id, where the web has a page for it. Never for an
    /// anonymised customer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    /// The customer's name ("Khách vãng lai" for a nameless profile, a booking guest's name), the
    /// staff member's, the service's, the plan's, the login's, or the role key (`owner` …).
    /// Absent when the record is gone or the customer was anonymised: the web says so neutrally.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The customer's recorded gender (`female` | `male`), for "chị" / "anh".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    /// The business day the record belongs to: a visit's, a sale's, a booking's, an attendance
    /// or review day, the day a bonus / penalty happened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<NaiveDate>,
    /// A customer package's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

/// How the web shows a changed value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeFormat {
    Text,
    /// Whole đồng.
    Money,
    Number,
    /// `YYYY-MM-DD`.
    Date,
    /// `HH:MM` or `HH:MM:SS`.
    Time,
    /// RFC 3339; the web shows it in the spa's time.
    Instant,
    /// `true` | `false`.
    Flag,
    /// Basis points (1 % = 100).
    PercentBp,
    /// E.164 (`+84…`).
    Phone,
    /// A code the web words per field (attendance status, role, evaluation result…).
    Code,
}

/// One field an action changed. `old` / `new` are absent when the trail does not keep the value
/// (a customer's address, a staff member's phone) or the side did not exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ActivityChange {
    pub field: String,
    pub format: ChangeFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ActivityEntry {
    pub id: Uuid,
    pub at: DateTime<Utc>,
    /// The day `at` falls on in the spa's time zone (the screen groups by it).
    pub day: NaiveDate,
    pub actor: ActivityActor,
    pub kind: ActivityKind,
    /// The action key (see `AuditAction`; a legacy row may hold another).
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<ActivitySubject>,
    pub changes: Vec<ActivityChange>,
    /// The money it moved, in đồng: a visit's or sale's total, a refund, a reversal, a
    /// correction's difference (signed), a bonus / penalty, a closed month's net pay.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i64>,
    /// A discount given on a visit or a sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discount: Option<i64>,
    /// Extra context by action: `staff.adjustment` → `bonus` | `penalty`; `payroll.close`,
    /// `evaluation.record` → the month (`YYYY-MM`); `feature.changed` → the feature key;
    /// `customer.follow_up` → the outcome; `staff.login_create` → the role;
    /// `visit.create` → `backdated` when recorded for a past day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// What the person typed as the reason (void, cancel, discount, override, extension; the
    /// platform team's reason).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A member the person filter offers (the owner, staff with a login, staff who left).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ActivityPerson {
    pub user_id: Uuid,
    pub name: String,
    /// Their staff profile has no open employment period, or the membership is disabled.
    pub left: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ActivityPage {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub items: Vec<ActivityEntry>,
    pub next_cursor: Option<String>,
    /// For the person filter (with "Đội vận hành" and "Hệ thống", which the web adds).
    pub people: Vec<ActivityPerson>,
}
