//! Contract of "ai ghi, ai sửa" on each record (`GET /record-history/...`, ADR-0022).

use crate::activity_dto::{ActivityActor, ActivityEntry};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Which staff book a day's cell is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StaffBook {
    /// "Chấm công" (`attendance.record`).
    Attendance,
    /// "Khen, phàn nàn" (`evaluation.record`).
    DailyReview,
}

/// `GET /record-history/staff-day/{staff_id}/{date}?book=`.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct StaffDayQuery {
    pub book: StaffBook,
}

/// The kind of record a history is about, after re-anchoring (a sale's history is its visit's or
/// its package's; a refund's is its package's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Visit,
    CustomerPackage,
    Customer,
    Appointment,
    /// One staff member's attendance on one day (a cell of the book; it has no id).
    Attendance,
    /// One staff member's praise / complaints on one day (a cell of the book; it has no id).
    DailyReview,
    /// A bonus / penalty, with the entries its edits replaced.
    Adjustment,
    /// A service of the price list.
    Service,
    /// A package plan ("mẫu gói").
    PackagePlan,
    /// A group of services of the price list.
    ServiceCategory,
    /// The spa's payroll rules (one per spa: no id).
    PayrollRules,
    /// The spa's appointment settings, "số khách tối đa" (one per spa: no id).
    AppointmentSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RecordRef {
    #[serde(rename = "type")]
    pub kind: RecordKind,
    /// Absent for a cell of a staff book (`attendance`, `daily_review`), a person and a day, and
    /// for the spa's own settings (`payroll_rules`, `appointment_settings`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
}

/// Where "Ghi bởi / Tạo bởi" comes from (spec D12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordSource {
    /// The creating action's audit row.
    Audit,
    /// No audit row: the record's own `created_by` / `created_at`.
    Record,
    /// Imported from the paper books: a day, no time ("Nhập từ sổ giấy · dd/mm/yyyy").
    PaperBook,
}

/// Who recorded it. Absent when nobody is known ("Chưa rõ ai ghi").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RecordCreated {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<ActivityActor>,
    /// Absent for a paper-book entry (its day is the book's, it has no time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<DateTime<Utc>>,
    /// The spa's day of `at`, or the paper book's day.
    pub day: NaiveDate,
    pub source: RecordSource,
}

/// The latest change, when it is not the creation itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RecordLast {
    pub actor: ActivityActor,
    pub at: DateTime<Utc>,
    pub day: NaiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordState {
    Active,
    /// A visit voided ("Đã hủy"); a bonus / penalty removed.
    Voided,
    /// A visit replaced by "Sửa lượt" ("Đã thay"); a bonus / penalty replaced by its edit.
    Replaced,
    /// A package or a booking cancelled.
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RecordSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<RecordCreated>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<RecordLast>,
    /// "đã sửa N lần": the lines below (as this caller sees them) whose action counts as an edit
    /// (spec D9, D10). The lists' `edited` is true exactly when this is at least 1.
    pub edits: u32,
    pub state: RecordState,
}

/// `GET /record-history/...`: one record's history, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RecordHistory {
    pub record: RecordRef,
    pub summary: RecordSummary,
    /// One line per action (a visit's checkout, a correction, a package sale fold into one), the
    /// 100 newest, oldest first. Only what the caller may read: no money without `sale.read`, no
    /// note change without `customer.health_notes.read`, no phone value without `audit.read`.
    pub items: Vec<ActivityEntry>,
    /// Older lines left out ("Còn {n} thay đổi cũ hơn.").
    pub omitted: u32,
}
