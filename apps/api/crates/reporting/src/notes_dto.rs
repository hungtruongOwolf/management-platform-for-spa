//! Shapes of `GET /notes` ("Nhật ký ghi chú", spec management-views §10).

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Where a note was written. Each kind keeps the read rule it already had on its own screen.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// A visit's session note (health data, encrypted): `visit.read` + `customer.health_notes.read`.
    VisitNote,
    /// The customer profile's note: `customer.read`.
    CustomerNote,
    /// The customer's health note (encrypted): `customer.read` + `customer.health_notes.read`.
    HealthNote,
    /// The note of a day's attendance: `attendance.record`.
    AttendanceNote,
    /// The note of a day's praise / complaints: `evaluation.record`.
    ReviewNote,
    /// The reason of a bonus / penalty still in the book: `evaluation.record`.
    Adjustment,
    /// The note of a follow-up call: `customer.read`.
    FollowUp,
    /// Why a visit, a sale or a package was voided, corrected or cancelled: `customer.read` +
    /// `visit.read` + `sale.read` (the customer timeline's rule).
    Void,
    /// Why someone's employment ended: `staff.manage` (owner).
    EmploymentEnd,
}

impl NoteKind {
    pub const ALL: [NoteKind; 9] = [
        NoteKind::VisitNote,
        NoteKind::CustomerNote,
        NoteKind::HealthNote,
        NoteKind::AttendanceNote,
        NoteKind::ReviewNote,
        NoteKind::Adjustment,
        NoteKind::FollowUp,
        NoteKind::Void,
        NoteKind::EmploymentEnd,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            NoteKind::VisitNote => "visit_note",
            NoteKind::CustomerNote => "customer_note",
            NoteKind::HealthNote => "health_note",
            NoteKind::AttendanceNote => "attendance_note",
            NoteKind::ReviewNote => "review_note",
            NoteKind::Adjustment => "adjustment",
            NoteKind::FollowUp => "follow_up",
            NoteKind::Void => "void",
            NoteKind::EmploymentEnd => "employment_end",
        }
    }

    pub fn parse(s: &str) -> Option<NoteKind> {
        NoteKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct NotesQuery {
    /// First business day (inclusive). Default: 29 days before `to`.
    pub from: Option<NaiveDate>,
    /// Last business day (inclusive). Default: today. At most 366 days after `from`.
    pub to: Option<NaiveDate>,
    /// Only notes of this kind (still only the kinds the caller may read).
    pub kind: Option<NoteKind>,
    /// Notes about this staff member, written by their login, or on visits they served.
    pub staff_id: Option<Uuid>,
    /// Notes about this customer.
    pub customer_id: Option<Uuid>,
    /// Accent-insensitive text in the note or the customer / staff name (2–100 characters).
    /// Encrypted notes are searched after decryption, within at most 500 scanned notes per page.
    pub q: Option<String>,
    pub cursor: Option<String>,
    /// 1–100, default 20.
    pub limit: Option<u32>,
}

/// One note. `customer_*` for notes about a customer, `staff_*` for notes about a staff member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct NoteEntry {
    pub kind: NoteKind,
    /// When it was written (a profile / health note: the profile's last change; an end of
    /// employment: the start of its last day).
    pub at: DateTime<Utc>,
    /// The business day it belongs to (the visit's, the attendance day, the contact day…).
    pub business_date: NaiveDate,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<Uuid>,
    /// The profile's name, or "Khách vãng lai".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staff_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staff_name: Option<String>,
    /// Who wrote it; absent when the source does not record it (profile and health notes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    /// The visit, for `visit_note` and visit voids.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_id: Option<Uuid>,
    /// Extra context by kind: `adjustment` → `bonus` | `penalty`; `void` → `visit` |
    /// `visit_corrected` | `sale` | `package`; `attendance_note` → the day's status;
    /// `follow_up` → the call's outcome.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct NotesPage {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub items: Vec<NoteEntry>,
    /// The kinds this caller may read (the type filter offers only these).
    pub kinds: Vec<NoteKind>,
    pub next_cursor: Option<String>,
    /// True when a search stopped after scanning its cap of notes before filling the page;
    /// `next_cursor` then continues the search further back.
    pub scan_capped: bool,
}
