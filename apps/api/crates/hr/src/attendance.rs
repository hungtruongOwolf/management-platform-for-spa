//! Daily attendance (spec staff-management US2), mirroring the sheet's codes: ✓ đi làm,
//! P (có lý do, báo trước ≥ 3 ngày), ĐX (đột xuất), K (không lý do). "Late" is derived: a present
//! day whose check-in is after the shift start, in minutes (sheet column G).

use crate::calc::{self, AttendanceTotals, Day};
use crate::month::Month;
use crate::period;
use crate::repo::{self, AttendanceRow};
use crate::rules;
use crate::scope::RecordScope;
use chrono::{NaiveDate, NaiveTime, Timelike};
use kernel::audit::{self, AuditEntry};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use org::profile::Profile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::{BTreeSet, HashSet};
use utoipa::ToSchema;
use uuid::Uuid;

pub const MAX_ENTRIES: usize = 100;
pub const MAX_NOTE_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttendanceStatus {
    /// ✓ đi làm.
    Present,
    /// P: nghỉ có lý do, báo trước ≥ 3 ngày.
    LeaveP,
    /// ĐX: nghỉ đột xuất, bổ sung giấy tờ sau.
    LeaveDx,
    /// K: nghỉ không lý do / không báo / không bổ sung giấy tờ.
    AbsentK,
}

impl AttendanceStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::LeaveP => "leave_p",
            Self::LeaveDx => "leave_dx",
            Self::AbsentK => "absent_k",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Present, Self::LeaveP, Self::LeaveDx, Self::AbsentK]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

/// The absence codes, for rules that treat them differently (⚠️ #2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LeaveCode {
    LeaveP,
    LeaveDx,
    AbsentK,
}

/// One recorded day as the API shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AttendanceDay {
    pub staff_id: Uuid,
    pub date: NaiveDate,
    pub status: AttendanceStatus,
    /// `HH:MM`.
    pub check_in: Option<String>,
    pub check_out: Option<String>,
    pub late_minutes: u32,
    pub note: Option<String>,
}

fn hhmm(t: NaiveTime) -> String {
    t.format("%H:%M").to_string()
}

impl TryFrom<AttendanceRow> for AttendanceDay {
    type Error = AppError;
    fn try_from(r: AttendanceRow) -> Result<Self, AppError> {
        Ok(Self {
            status: AttendanceStatus::parse(&r.status).ok_or_else(|| {
                AppError::Internal(format!("attendance {} has status {}", r.staff_id, r.status))
            })?,
            staff_id: r.staff_id,
            date: r.work_date,
            check_in: r.check_in.map(hhmm),
            check_out: r.check_out.map(hhmm),
            late_minutes: u32::try_from(r.late_minutes).unwrap_or(0),
            note: r.note,
        })
    }
}

impl AttendanceDay {
    pub const fn as_calc(&self) -> Day {
        Day {
            date: self.date,
            status: self.status,
            late_minutes: self.late_minutes,
        }
    }
}

/// One cell of the grid: `status: None` clears the day.
#[derive(Debug, Clone)]
pub struct Entry {
    pub staff_id: Uuid,
    pub date: NaiveDate,
    pub status: Option<AttendanceStatus>,
    pub check_in: Option<NaiveTime>,
    pub check_out: Option<NaiveTime>,
    pub note: Option<String>,
}

/// Minutes from the shift start to the check-in, 0 when on time or unknown.
pub fn late_minutes(shift_start: NaiveTime, check_in: Option<NaiveTime>) -> u32 {
    check_in.map_or(0, |c| {
        let (c, s) = (
            c.num_seconds_from_midnight(),
            shift_start.num_seconds_from_midnight(),
        );
        c.saturating_sub(s) / 60
    })
}

/// Validates a batch against today and the roster; returns field errors indexed like the batch.
pub fn validate(entries: &[Entry], today: NaiveDate, staff: &[Profile]) -> Result<(), AppError> {
    let mut errors = Vec::new();
    if entries.is_empty() || entries.len() > MAX_ENTRIES {
        errors.push(FieldError::new("entries", "count"));
    }
    let mut seen = HashSet::new();
    for (i, e) in entries.iter().enumerate() {
        let f = |name: &str| format!("entries[{i}].{name}");
        if !seen.insert((e.staff_id, e.date)) {
            errors.push(FieldError::new(f("date"), "duplicate"));
        }
        if e.date > today {
            errors.push(FieldError::new(f("date"), "in_future"));
        }
        match staff.iter().find(|p| p.id == e.staff_id) {
            None => errors.push(FieldError::new(f("staff_id"), "unknown")),
            Some(p)
                if p.joined_on.is_some_and(|j| e.date < j)
                    || p.left_on.is_some_and(|l| e.date > l) =>
            {
                errors.push(FieldError::new(f("date"), "not_employed"));
            }
            Some(_) => {}
        }
        let timed = e.check_in.is_some() || e.check_out.is_some();
        if timed && e.status != Some(AttendanceStatus::Present) {
            errors.push(FieldError::new(f("check_in"), "only_when_present"));
        }
        if e.status.is_none() && e.note.is_some() {
            errors.push(FieldError::new(f("note"), "only_with_status"));
        }
        if e.note
            .as_ref()
            .is_some_and(|n| n.chars().count() > MAX_NOTE_CHARS)
        {
            errors.push(FieldError::new(f("note"), "too_long"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

/// The days before which an employed day without a record counts as missing.
pub fn recorded_until(month: Month, today: NaiveDate) -> Option<NaiveDate> {
    (month.last_day() >= today).then_some(today)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct StaffSheet {
    pub staff_id: Uuid,
    pub full_name: String,
    pub position: String,
    /// `HH:MM`, the person's own or the spa default.
    pub shift_start: String,
    pub joined_on: Option<NaiveDate>,
    pub left_on: Option<NaiveDate>,
    pub days: Vec<AttendanceDay>,
    pub totals: AttendanceTotals,
    /// Whether the caller may record this person's attendance, praise / complaints, bonuses /
    /// penalties and evaluation (decision C: a manager never for themselves or another manager).
    /// Always false on `/me/staff`.
    pub can_record: bool,
    /// Days of the month this person performed a service of a completed visit (a leave code on
    /// such a day gets a warning). Empty on `/me/staff`.
    pub visit_days: Vec<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AttendanceSheet {
    /// `YYYY-MM`.
    pub month: String,
    pub days_in_month: u32,
    pub today: NaiveDate,
    pub closed: bool,
    pub default_shift_start: String,
    pub staff: Vec<StaffSheet>,
}

/// Builds one person's sheet from their recorded days.
pub fn sheet_of(
    p: &Profile,
    month: Month,
    today: NaiveDate,
    default_shift: NaiveTime,
    days: Vec<AttendanceDay>,
) -> StaffSheet {
    let calc_days: Vec<Day> = days.iter().map(AttendanceDay::as_calc).collect();
    StaffSheet {
        staff_id: p.id,
        full_name: p.full_name.clone(),
        position: p.position.clone(),
        shift_start: hhmm(p.shift_start.unwrap_or(default_shift)),
        joined_on: p.joined_on,
        left_on: p.left_on,
        totals: calc::summarize(
            month,
            recorded_until(month, today),
            p.joined_on,
            p.left_on,
            &calc_days,
        ),
        days,
        can_record: false,
        visit_days: Vec::new(),
    }
}

/// Days of `month` (optionally one person's) inside the caller's transaction.
pub async fn days_in(
    tx: &mut TenantTx,
    month: Month,
    staff: Option<Uuid>,
) -> Result<Vec<AttendanceDay>, AppError> {
    repo::attendance_between(tx, month.first_day(), month.last_day(), staff)
        .await?
        .into_iter()
        .map(AttendanceDay::try_from)
        .collect()
}

/// `GET /attendance?month=` (`attendance.record`): month × staff grid of everyone employed in it.
pub async fn sheet(
    pool: &PgPool,
    actor: &Actor,
    month: Option<Month>,
) -> Result<AttendanceSheet, AppError> {
    actor.require(Permission::AttendanceRecord)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = period::today(&mut tx).await?;
    let month = month.unwrap_or_else(|| Month::of(today));
    let (rules, _) = rules::load(&mut tx).await?;
    let info = period::info(&mut tx, month).await?;
    let profiles = org::profile::profiles_in(&mut tx).await?;
    let all_days = days_in(&mut tx, month, None).await?;
    let visits = repo::visit_days(&mut tx, month.first_day(), month.last_day()).await?;
    let scope = RecordScope::load(&mut tx, actor).await?;
    tx.commit().await?;
    let staff = profiles
        .iter()
        .filter(|p| period::employed_in(p, month))
        .map(|p| {
            let days = all_days
                .iter()
                .filter(|d| d.staff_id == p.id)
                .cloned()
                .collect();
            StaffSheet {
                can_record: scope.may_record(p.id),
                visit_days: visits
                    .iter()
                    .filter(|v| v.staff_id == p.id)
                    .map(|v| v.day)
                    .collect(),
                ..sheet_of(p, month, today, rules.default_shift_start, days)
            }
        })
        .collect();
    Ok(AttendanceSheet {
        month: month.to_string(),
        days_in_month: month.days(),
        today,
        closed: info.closed,
        default_shift_start: hhmm(rules.default_shift_start),
        staff,
    })
}

fn audit_value(d: Option<&AttendanceDay>) -> Option<serde_json::Value> {
    d.map(|d| {
        json!({"date": d.date, "status": d.status.as_str(), "check_in": d.check_in,
               "check_out": d.check_out, "late_minutes": d.late_minutes, "has_note": d.note.is_some()})
    })
}

/// `PUT /attendance` (`attendance.record`): records or clears up to [`MAX_ENTRIES`] cells in one
/// transaction (a single edit, or "everyone present today"). Months touched must be open. A
/// manager may not record themselves or another manager (`403 staff.record_owner_only`).
pub async fn record(
    pool: &PgPool,
    actor: &Actor,
    entries: &[Entry],
) -> Result<Vec<AttendanceDay>, AppError> {
    actor.require(Permission::AttendanceRecord)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = period::today(&mut tx).await?;
    let profiles = org::profile::profiles_in(&mut tx).await?;
    RecordScope::load(&mut tx, actor)
        .await?
        .require(entries.iter().map(|e| e.staff_id))?;
    validate(entries, today, &profiles)?;
    // Lock months in a stable order so two batches never deadlock.
    let months: BTreeSet<Month> = entries.iter().map(|e| Month::of(e.date)).collect();
    for m in months {
        period::require_open(&mut tx, m).await?;
    }
    let (rules, _) = rules::load(&mut tx).await?;
    let by = actor.user_id.as_uuid();
    let mut out = Vec::new();
    for e in entries {
        let before = repo::attendance_day(&mut tx, e.staff_id, e.date)
            .await?
            .map(AttendanceDay::try_from)
            .transpose()?;
        let after = match e.status {
            None => {
                repo::delete_attendance(&mut tx, e.staff_id, e.date).await?;
                None
            }
            Some(status) => {
                let shift = profiles
                    .iter()
                    .find(|p| p.id == e.staff_id)
                    .and_then(|p| p.shift_start)
                    .unwrap_or(rules.default_shift_start);
                let late = if status == AttendanceStatus::Present {
                    late_minutes(shift, e.check_in)
                } else {
                    0
                };
                let row = AttendanceRow {
                    staff_id: e.staff_id,
                    work_date: e.date,
                    status: status.as_str().to_owned(),
                    check_in: e.check_in,
                    check_out: e.check_out,
                    late_minutes: i32::try_from(late).unwrap_or(0),
                    note: e.note.clone(),
                };
                repo::upsert_attendance(&mut tx, &row, by).await?;
                let day = AttendanceDay::try_from(row)?;
                out.push(day.clone());
                Some(day)
            }
        };
        if before != after {
            audit::record(
                &mut tx,
                actor,
                AuditEntry {
                    action: "attendance.record",
                    entity_type: "staff",
                    entity_id: e.staff_id,
                    before: audit_value(before.as_ref()),
                    after: audit_value(after.as_ref())
                        .or_else(|| Some(json!({"date": e.date, "cleared": true}))),
                    reason: None,
                },
            )
            .await?;
        }
    }
    tx.commit().await?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use org::StaffStatus;

    fn t(h: u32, m: u32) -> Option<NaiveTime> {
        NaiveTime::from_hms_opt(h, m, 0)
    }

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap_or_default()
    }

    fn mai() -> Profile {
        Profile {
            id: Uuid::nil(),
            full_name: "Mai".into(),
            position: "KTV".into(),
            phone: None,
            status: StaffStatus::Active,
            joined_on: Some(d("2026-11-03")),
            left_on: None,
            user_id: None,
            user_name: None,
            shift_start: None,
            version: 1,
        }
    }

    fn entry(date: &str, status: Option<AttendanceStatus>) -> Entry {
        Entry {
            staff_id: Uuid::nil(),
            date: d(date),
            status,
            check_in: None,
            check_out: None,
            note: None,
        }
    }

    #[test]
    fn late_is_minutes_after_the_shift_start() {
        let nine = NaiveTime::from_hms_opt(9, 0, 0).unwrap_or(NaiveTime::MIN);
        assert_eq!(late_minutes(nine, t(8, 55)), 0);
        assert_eq!(late_minutes(nine, t(9, 0)), 0);
        assert_eq!(late_minutes(nine, t(9, 17)), 17);
        assert_eq!(late_minutes(nine, None), 0);
    }

    #[test]
    fn batch_rules() {
        let today = d("2026-11-05");
        let staff = [mai()];
        let ok = [entry("2026-11-04", Some(AttendanceStatus::Present))];
        assert!(validate(&ok, today, &staff).is_ok());
        assert!(validate(&[], today, &staff).is_err(), "empty");
        let future = [entry("2026-11-06", Some(AttendanceStatus::Present))];
        assert!(validate(&future, today, &staff).is_err());
        let before_joining = [entry("2026-11-02", Some(AttendanceStatus::Present))];
        assert!(validate(&before_joining, today, &staff).is_err());
        let dup = [
            entry("2026-11-04", Some(AttendanceStatus::Present)),
            entry("2026-11-04", None),
        ];
        assert!(validate(&dup, today, &staff).is_err());
        let mut timed_leave = entry("2026-11-04", Some(AttendanceStatus::LeaveP));
        timed_leave.check_in = t(9, 0);
        assert!(validate(&[timed_leave], today, &staff).is_err());
        let mut stranger = entry("2026-11-04", Some(AttendanceStatus::Present));
        stranger.staff_id = Uuid::max();
        assert!(validate(&[stranger], today, &staff).is_err());
    }

    #[test]
    fn statuses_round_trip() {
        for s in [
            AttendanceStatus::Present,
            AttendanceStatus::LeaveP,
            AttendanceStatus::LeaveDx,
            AttendanceStatus::AbsentK,
        ] {
            assert_eq!(AttendanceStatus::parse(s.as_str()), Some(s));
        }
    }
}
