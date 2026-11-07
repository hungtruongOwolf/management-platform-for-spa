//! Staff profiles, the roster and pay terms (spec staff-management US1 + §13). Reading the roster
//! needs `staff.read` (owner, manager); personal details (phone, note, login, end reasons) and
//! every write need `staff.manage` (owner); pay terms are salary data and are only ever read or
//! written by holders of `payroll.read` as well (non-negotiable 4: a caller without the
//! permission never receives the field, not even `null`).
//!
//! Employment is a list of periods ([`Employment`], ADR-0012): hiring opens one, ending
//! employment closes it (soft: nothing is deleted), rehiring opens a new one.

use crate::employment::{Employment, Period, Tenure};
use crate::repo::{self, AccountRow, HistoryRow, PayRow, PeriodRow, ProfileRow, ProfileWrite};
use crate::service::StaffStatus;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use kernel::audit::{self, AuditEntry};
use kernel::{Actor, AppError, FieldError, Permission, PhoneE164, TenantTx};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

pub const MAX_BASE_SALARY: i64 = 1_000_000_000;
pub const MAX_ALLOWANCE: i64 = 100_000_000;
pub const MAX_COMMISSION_BP: i32 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: Uuid,
    pub full_name: String,
    pub position: String,
    pub phone: Option<PhoneE164>,
    pub status: StaffStatus,
    /// Start of the latest period (mirrored on the staff row).
    pub joined_on: Option<NaiveDate>,
    /// End of the latest period, when the person has left.
    pub left_on: Option<NaiveDate>,
    /// The login linked to this staff member (self-service), with its display name.
    pub user_id: Option<Uuid>,
    pub user_name: Option<String>,
    pub shift_start: Option<NaiveTime>,
    pub note: Option<String>,
    pub version: i64,
    pub employment: Employment,
}

impl Profile {
    fn from_rows(r: ProfileRow, periods: Vec<Period>) -> Result<Self, AppError> {
        let status = StaffStatus::parse(&r.status)
            .ok_or_else(|| AppError::Internal(format!("staff {} has status {}", r.id, r.status)))?;
        Ok(Self {
            id: r.id,
            full_name: r.full_name,
            position: r.position,
            phone: r.phone_e164.and_then(|p| PhoneE164::parse(&p).ok()),
            status,
            joined_on: r.joined_on,
            left_on: r.left_on,
            user_id: r.user_id,
            user_name: r.user_name,
            shift_start: r.shift_start,
            note: r.note,
            version: r.version,
            employment: Employment::new(periods),
        })
    }
}

fn period_of(r: PeriodRow) -> Period {
    Period {
        id: r.id,
        started_on: r.started_on,
        ended_on: r.ended_on,
        end_reason: r.end_reason,
    }
}

/// Profiles with their periods (everyone, or one person).
async fn load(tx: &mut TenantTx, only: Option<Uuid>, lock: bool) -> Result<Vec<Profile>, AppError> {
    let rows = match only {
        Some(id) => repo::profile(tx, id, lock).await?.into_iter().collect(),
        None => repo::profiles(tx).await?,
    };
    let mut periods = repo::periods(tx, only).await?;
    rows.into_iter()
        .map(|r| {
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut periods)
                .into_iter()
                .partition(|p| p.staff_id == r.id);
            periods = rest;
            Profile::from_rows(r, mine.into_iter().map(period_of).collect())
        })
        .collect()
}

async fn load_one(tx: &mut TenantTx, id: Uuid, lock: bool) -> Result<Profile, AppError> {
    load(tx, Some(id), lock)
        .await?
        .pop()
        .ok_or(AppError::NotFound)
}

/// Pay terms of one staff member (salary data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayTerms {
    pub base_salary: i64,
    /// Basis points: 500 = 5 %.
    pub commission_rate_bp: i32,
    pub loyalty_bonus_monthly: i64,
    pub meal_allowance: i64,
    pub version: i64,
}

impl From<&PayRow> for PayTerms {
    fn from(r: &PayRow) -> Self {
        Self {
            base_salary: r.base_salary,
            commission_rate_bp: r.commission_rate_bp,
            loyalty_bonus_monthly: r.loyalty_bonus_monthly,
            meal_allowance: r.meal_allowance,
            version: r.version,
        }
    }
}

/// A profile as its caller may see it. `details_visible` (`staff.manage`): phone, note, login and
/// end reasons; `pay_visible` (`payroll.read`): pay terms. A field the caller may not see is
/// absent, never sent and hidden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileView {
    pub profile: Profile,
    pub tenure: Option<Tenure>,
    pub details_visible: bool,
    pub pay: Option<PayTerms>,
    pub pay_visible: bool,
}

pub struct ProfileInput {
    pub full_name: String,
    pub position: String,
    pub phone: Option<String>,
    /// Start of the current period ("ngày vào làm"); `None` = not recorded (tenure unknown).
    /// The UI fills in today when hiring.
    pub joined_on: Option<NaiveDate>,
    pub shift_start: Option<NaiveTime>,
    pub note: Option<String>,
}

pub struct PayInput {
    pub base_salary: i64,
    pub commission_rate_bp: i32,
    pub loyalty_bonus_monthly: i64,
    pub meal_allowance: i64,
}

struct CleanProfile {
    full_name: String,
    position: String,
    phone: Option<PhoneE164>,
    note: Option<String>,
}

fn validate_profile(input: &ProfileInput) -> Result<CleanProfile, AppError> {
    let mut errors = Vec::new();
    let full_name = input.full_name.trim().to_owned();
    if full_name.is_empty() || full_name.chars().count() > 200 {
        errors.push(FieldError::new("full_name", "invalid"));
    }
    let position = input.position.trim().to_owned();
    if position.is_empty() || position.chars().count() > 100 {
        errors.push(FieldError::new("position", "invalid"));
    }
    let phone = match input
        .phone
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        None => None,
        Some(p) => match PhoneE164::parse(p) {
            Ok(phone) => Some(phone),
            Err(_) => {
                errors.push(FieldError::new("phone", "invalid"));
                None
            }
        },
    };
    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned);
    if note.as_ref().is_some_and(|n| n.chars().count() > 1000) {
        errors.push(FieldError::new("note", "too_long"));
    }
    if errors.is_empty() {
        Ok(CleanProfile {
            full_name,
            position,
            phone,
            note,
        })
    } else {
        Err(AppError::Validation(errors))
    }
}

fn validate_pay(input: &PayInput) -> Result<(), AppError> {
    let mut errors = Vec::new();
    if !(0..=MAX_BASE_SALARY).contains(&input.base_salary) {
        errors.push(FieldError::new("base_salary", "out_of_range"));
    }
    if !(0..=MAX_COMMISSION_BP).contains(&input.commission_rate_bp) {
        errors.push(FieldError::new("commission_rate_bp", "out_of_range"));
    }
    if !(0..=MAX_ALLOWANCE).contains(&input.loyalty_bonus_monthly) {
        errors.push(FieldError::new("loyalty_bonus_monthly", "out_of_range"));
    }
    if !(0..=MAX_ALLOWANCE).contains(&input.meal_allowance) {
        errors.push(FieldError::new("meal_allowance", "out_of_range"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

fn field(name: &'static str, code: &'static str) -> AppError {
    AppError::Validation(vec![FieldError::new(name, code)])
}

/// A login may be linked only if it is an active member of this spa and not linked to someone else.
async fn check_link(
    tx: &mut TenantTx,
    user_id: Option<Uuid>,
    staff_id: Uuid,
) -> Result<(), AppError> {
    let Some(user) = user_id else { return Ok(()) };
    if !repo::is_active_member(tx, user).await? {
        return Err(field("user_id", "not_a_member"));
    }
    match repo::staff_of_user(tx, user).await? {
        Some(other) if other != staff_id => Err(AppError::ConflictWith {
            code: "staff.account_taken",
            existing_id: other,
        }),
        _ => Ok(()),
    }
}

fn audit_profile(p: &Profile) -> Value {
    json!({
        "full_name": p.full_name, "position": p.position,
        "has_phone": p.phone.is_some(), "has_note": p.note.is_some(), "status": p.status.as_str(),
        "joined_on": p.joined_on, "left_on": p.left_on, "user_id": p.user_id,
        "shift_start": p.shift_start.map(|t| t.format("%H:%M").to_string()),
    })
}

fn audit_pay(p: &PayTerms) -> Value {
    json!({
        "base_salary": p.base_salary, "commission_rate_bp": p.commission_rate_bp,
        "loyalty_bonus_monthly": p.loyalty_bonus_monthly, "meal_allowance": p.meal_allowance,
    })
}

fn view(actor: &Actor, profile: Profile, pay: Option<&PayRow>, today: NaiveDate) -> ProfileView {
    let pay_visible = actor.can(Permission::PayrollRead);
    ProfileView {
        tenure: profile.employment.tenure(today),
        details_visible: actor.can(Permission::StaffManage),
        pay: pay.filter(|_| pay_visible).map(PayTerms::from),
        pay_visible,
        profile,
    }
}

/// `412` before any other answer when the client edited an older version (the row is locked).
fn fresh(p: &Profile, version: i64) -> Result<(), AppError> {
    if p.version == version {
        Ok(())
    } else {
        Err(AppError::StaleVersion)
    }
}

async fn today(tx: &mut TenantTx) -> Result<NaiveDate, AppError> {
    Ok(kernel::tenant::profile(tx).await?.business_date.as_naive())
}

/// The caller's view of one profile, read inside the write's transaction.
async fn reread(tx: &mut TenantTx, actor: &Actor, id: Uuid) -> Result<ProfileView, AppError> {
    let profile = load_one(tx, id, false).await?;
    let pay = if actor.can(Permission::PayrollRead) {
        repo::pay_for_update(tx, id).await?
    } else {
        None
    };
    let today = today(tx).await?;
    Ok(view(actor, profile, pay.as_ref(), today))
}

/// `GET /staff/profiles?status=` (`staff.read`): the roster, `None` = everyone, working first.
pub async fn list(
    pool: &PgPool,
    actor: &Actor,
    status: Option<StaffStatus>,
) -> Result<Vec<ProfileView>, AppError> {
    actor.require(Permission::StaffRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let profiles = load(&mut tx, None, false).await?;
    let pays = if actor.can(Permission::PayrollRead) {
        repo::pays(&mut tx).await?
    } else {
        Vec::new()
    };
    let today = today(&mut tx).await?;
    tx.commit().await?;
    Ok(profiles
        .into_iter()
        .filter(|p| status.is_none_or(|s| p.status == s))
        .map(|p| {
            let pay = pays.iter().find(|row| row.staff_id == p.id);
            view(actor, p, pay, today)
        })
        .collect())
}

/// `GET /staff/{id}` (`staff.read`).
pub async fn get(pool: &PgPool, actor: &Actor, id: Uuid) -> Result<ProfileView, AppError> {
    actor.require(Permission::StaffRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let profile = load_one(&mut tx, id, false).await?;
    let pay = if actor.can(Permission::PayrollRead) {
        repo::pays(&mut tx)
            .await?
            .into_iter()
            .find(|p| p.staff_id == id)
    } else {
        None
    };
    let today = today(&mut tx).await?;
    tx.commit().await?;
    Ok(view(actor, profile, pay.as_ref(), today))
}

/// `POST /staff` (`staff.manage`): hires someone — an active profile with its first period.
pub async fn create(
    pool: &PgPool,
    actor: &Actor,
    input: &ProfileInput,
    user_id: Option<Uuid>,
) -> Result<ProfileView, AppError> {
    actor.require(Permission::StaffManage)?;
    let clean = validate_profile(input)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let id = Uuid::now_v7();
    check_link(&mut tx, user_id, id).await?;
    let branch = kernel::tenant::profile(&mut tx).await?.branch;
    repo::insert_profile(
        &mut tx,
        id,
        branch.as_uuid(),
        &ProfileWrite {
            full_name: &clean.full_name,
            position: &clean.position,
            phone_e164: clean.phone.as_ref().map(PhoneE164::as_str),
            joined_on: input.joined_on,
            shift_start: input.shift_start,
            note: clean.note.as_deref(),
        },
        user_id,
    )
    .await?;
    let created = reread(&mut tx, actor, id).await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.create",
            entity_type: "staff",
            entity_id: id,
            before: None,
            after: Some(audit_profile(&created.profile)),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(created)
}

/// `PUT /staff/{id}` (`staff.manage`, `If-Match`): name, position, phone, note, shift start and
/// the start of the current period. Ending, rehiring and the login have their own use cases.
pub async fn update(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    version: i64,
    input: &ProfileInput,
) -> Result<ProfileView, AppError> {
    actor.require(Permission::StaffManage)?;
    let clean = validate_profile(input)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let before = load_one(&mut tx, id, true).await?;
    fresh(&before, version)?;
    let periods = before.employment.periods();
    let latest = periods
        .last()
        .ok_or_else(|| AppError::Internal(format!("staff {id} has no period")))?;
    if latest.started_on != input.joined_on {
        if latest.ended_on.is_some() {
            // An ended period is history (0025 trigger): rehire to start a new one.
            return Err(field("joined_on", "period_ended"));
        }
        let previous_end = periods.iter().rev().nth(1).and_then(|p| p.ended_on);
        match (input.joined_on, previous_end) {
            (None, Some(_)) => return Err(field("joined_on", "required")),
            (Some(start), Some(end)) if start <= end => {
                return Err(field("joined_on", "overlaps_previous"));
            }
            _ => {}
        }
        repo::set_period_start(&mut tx, latest.id, input.joined_on).await?;
    }
    let written = repo::update_profile(
        &mut tx,
        id,
        version,
        &ProfileWrite {
            full_name: &clean.full_name,
            position: &clean.position,
            phone_e164: clean.phone.as_ref().map(PhoneE164::as_str),
            joined_on: input.joined_on,
            shift_start: input.shift_start,
            note: clean.note.as_deref(),
        },
    )
    .await?;
    if !written {
        return Err(AppError::StaleVersion);
    }
    let after = reread(&mut tx, actor, id).await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.update",
            entity_type: "staff",
            entity_id: id,
            before: Some(audit_profile(&before)),
            after: Some(audit_profile(&after.profile)),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// `PUT /staff/{id}/account` (`staff.manage`, `If-Match`): links a login to the profile, or
/// unlinks it (`None`). Decision C reads this link: a manager's profile without one counts as a
/// technician's. Passwords are never set or shown here.
pub async fn set_account(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    version: i64,
    user_id: Option<Uuid>,
) -> Result<ProfileView, AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let before = load_one(&mut tx, id, true).await?;
    fresh(&before, version)?;
    check_link(&mut tx, user_id, id).await?;
    if !repo::set_link(&mut tx, id, version, user_id).await? {
        return Err(AppError::StaleVersion);
    }
    let after = reread(&mut tx, actor, id).await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.account_link",
            entity_type: "staff",
            entity_id: id,
            before: Some(json!({"user_id": before.user_id})),
            after: Some(json!({"user_id": after.profile.user_id})),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

pub struct EndInput {
    pub ended_on: NaiveDate,
    pub reason: String,
}

/// `POST /staff/{id}/end` (`staff.manage`, `If-Match`): nghỉ việc. Closes the open period with
/// its last day and reason; the person leaves every picker and every later month, keeps every
/// past record, and their linked login is locked in this spa.
pub async fn end(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    version: i64,
    input: &EndInput,
) -> Result<ProfileView, AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let before = load_one(&mut tx, id, true).await?;
    fresh(&before, version)?;
    let reason = input.reason.trim();
    if reason.is_empty() {
        return Err(field("reason", "required"));
    }
    if reason.chars().count() > 500 {
        return Err(field("reason", "too_long"));
    }
    let Some(current) = before.employment.current().cloned() else {
        return Err(AppError::Conflict {
            code: "staff.already_left",
        });
    };
    if input.ended_on > today(&mut tx).await? {
        return Err(field("ended_on", "in_future"));
    }
    if current.started_on.is_some_and(|s| input.ended_on < s) {
        return Err(field("ended_on", "before_joined_on"));
    }
    if let Some(user) = before.user_id {
        if user == actor.user_id.as_uuid() {
            return Err(AppError::Unprocessable {
                code: "staff.cannot_end_self",
            });
        }
        if repo::login_manages_staff(&mut tx, user).await? {
            return Err(AppError::Unprocessable {
                code: "staff.linked_to_owner",
            });
        }
    }
    if repo::worked_after(&mut tx, id, input.ended_on).await? {
        return Err(field("ended_on", "has_later_records"));
    }
    if repo::bookings_after(&mut tx, id, input.ended_on).await? > 0 {
        return Err(AppError::Conflict {
            code: "staff.has_upcoming_bookings",
        });
    }
    repo::end_period(
        &mut tx,
        current.id,
        input.ended_on,
        reason,
        actor.user_id.as_uuid(),
    )
    .await?;
    if !repo::set_mirror(
        &mut tx,
        id,
        version,
        current.started_on,
        Some(input.ended_on),
    )
    .await?
    {
        return Err(AppError::StaleVersion);
    }
    if let Some(user) = before.user_id {
        repo::set_membership_active(&mut tx, user, false).await?;
    }
    let after = reread(&mut tx, actor, id).await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.end",
            entity_type: "staff",
            entity_id: id,
            before: Some(json!({"status": "active", "joined_on": current.started_on})),
            after: Some(json!({
                "status": "left", "left_on": input.ended_on,
                "login_locked": before.user_id.is_some(),
            })),
            reason: Some(reason),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// `POST /staff/{id}/rehire` (`staff.manage`, `If-Match`): opens a new period after the last one;
/// the earlier periods stay as they were. A linked login is unlocked again.
pub async fn rehire(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    version: i64,
    started_on: NaiveDate,
) -> Result<ProfileView, AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let before = load_one(&mut tx, id, true).await?;
    fresh(&before, version)?;
    if before.employment.current().is_some() {
        return Err(AppError::Conflict {
            code: "staff.already_active",
        });
    }
    if before
        .employment
        .latest()
        .and_then(|p| p.ended_on)
        .is_some_and(|end| started_on <= end)
    {
        return Err(field("started_on", "before_last_end"));
    }
    repo::insert_period(&mut tx, id, started_on).await?;
    if !repo::set_mirror(&mut tx, id, version, Some(started_on), None).await? {
        return Err(AppError::StaleVersion);
    }
    if let Some(user) = before.user_id {
        repo::set_membership_active(&mut tx, user, true).await?;
    }
    let after = reread(&mut tx, actor, id).await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.rehire",
            entity_type: "staff",
            entity_id: id,
            before: Some(json!({"status": "left", "left_on": before.left_on})),
            after: Some(json!({
                "status": "active", "joined_on": started_on,
                "login_unlocked": before.user_id.is_some(),
            })),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// `PUT /staff/{id}/pay` (`staff.manage` + `payroll.read`: whoever sets a salary sees it).
/// Takes effect for every month not yet closed; closed payslips keep the terms they used.
pub async fn set_pay(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    input: &PayInput,
) -> Result<PayTerms, AppError> {
    actor.require(Permission::StaffManage)?;
    actor.require(Permission::PayrollRead)?;
    validate_pay(input)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    repo::profile(&mut tx, id, true)
        .await?
        .ok_or(AppError::NotFound)?;
    let before = repo::pay_for_update(&mut tx, id).await?;
    repo::upsert_pay(
        &mut tx,
        &PayRow {
            staff_id: id,
            base_salary: input.base_salary,
            commission_rate_bp: input.commission_rate_bp,
            loyalty_bonus_monthly: input.loyalty_bonus_monthly,
            meal_allowance: input.meal_allowance,
            version: 0,
        },
    )
    .await?;
    let after = repo::pay_for_update(&mut tx, id)
        .await?
        .ok_or_else(|| AppError::Internal("pay terms vanished".into()))?;
    let after = PayTerms::from(&after);
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.pay_update",
            entity_type: "staff",
            entity_id: id,
            before: before.as_ref().map(|b| audit_pay(&PayTerms::from(b))),
            after: Some(audit_pay(&after)),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// One changed field of a history entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub field: String,
    pub before: Value,
    pub after: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub at: DateTime<Utc>,
    /// `staff.create`, `staff.update`, `staff.account_link`, `staff.end`, `staff.rehire`,
    /// `staff.pay_update`.
    pub action: String,
    pub actor_name: Option<String>,
    pub changes: Vec<Change>,
    pub reason: Option<String>,
}

fn changes(before: Option<&Value>, after: Option<&Value>) -> Vec<Change> {
    let empty = serde_json::Map::new();
    let b = before.and_then(Value::as_object).unwrap_or(&empty);
    let a = after.and_then(Value::as_object).unwrap_or(&empty);
    let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter_map(|k| {
            let (old, new) = (
                b.get(k).cloned().unwrap_or(Value::Null),
                a.get(k).cloned().unwrap_or(Value::Null),
            );
            (old != new).then(|| Change {
                field: k.clone(),
                before: old,
                after: new,
            })
        })
        .collect()
}

/// `GET /staff/{id}/history` (`staff.manage`): who changed what on the profile — position, start
/// date, login, end / rehire with reason — and, for `payroll.read` holders only, pay changes.
pub async fn history(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
) -> Result<Vec<HistoryEntry>, AppError> {
    actor.require(Permission::StaffManage)?;
    let pay_visible = actor.can(Permission::PayrollRead);
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    repo::profile(&mut tx, id, false)
        .await?
        .ok_or(AppError::NotFound)?;
    let rows = repo::history(&mut tx, id).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .filter(|r| pay_visible || r.action != "staff.pay_update")
        .map(|r: HistoryRow| HistoryEntry {
            changes: changes(r.before.as_ref(), r.after.as_ref()),
            at: r.at,
            action: r.action,
            actor_name: r.actor_name,
            reason: r.reason,
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub user_id: Uuid,
    pub full_name: String,
    pub roles: Vec<String>,
    pub staff_id: Option<Uuid>,
}

impl From<AccountRow> for Account {
    fn from(r: AccountRow) -> Self {
        Self {
            user_id: r.user_id,
            full_name: r.full_name,
            roles: r.roles,
            staff_id: r.staff_id,
        }
    }
}

/// `GET /staff/accounts` (`staff.manage`): the spa's logins, for linking a profile.
pub async fn accounts(pool: &PgPool, actor: &Actor) -> Result<Vec<Account>, AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let rows = repo::accounts(&mut tx).await?;
    tx.commit().await?;
    Ok(rows.into_iter().map(Account::from).collect())
}

// ------------------------------------------------------------ for other contexts (no authz)

/// Everyone's pay terms by staff id, inside the caller's transaction. The caller authorizes
/// (`payroll.read`); this only reads.
pub async fn pay_terms(tx: &mut TenantTx) -> Result<Vec<(Uuid, PayTerms)>, AppError> {
    Ok(repo::pays(tx)
        .await?
        .iter()
        .map(|p| (p.staff_id, PayTerms::from(p)))
        .collect())
}

/// Every profile with its employment periods, inside the caller's transaction. The caller
/// authorizes.
pub async fn profiles_in(tx: &mut TenantTx) -> Result<Vec<Profile>, AppError> {
    load(tx, None, false).await
}

/// The staff member linked to the actor's login in this spa (self-service), if any.
pub async fn own_staff_id(tx: &mut TenantTx, actor: &Actor) -> Result<Option<Uuid>, AppError> {
    repo::staff_of_user(tx, actor.user_id.as_uuid()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ProfileInput {
        ProfileInput {
            full_name: "  Lê Thị Mai ".into(),
            position: "Kỹ thuật viên".into(),
            phone: Some("0905 111 222".into()),
            joined_on: "2026-01-05".parse().ok(),
            shift_start: NaiveTime::from_hms_opt(9, 0, 0),
            note: Some("  ".into()),
        }
    }

    #[test]
    fn profile_is_trimmed_and_phone_normalised() -> Result<(), AppError> {
        let c = validate_profile(&input())?;
        assert_eq!(c.full_name, "Lê Thị Mai");
        assert_eq!(
            c.phone.map(|p| p.as_str().to_owned()).as_deref(),
            Some("+84905111222")
        );
        assert_eq!(c.note, None, "a blank note is no note");
        let mut bad = input();
        bad.phone = Some("12345".into());
        assert!(validate_profile(&bad).is_err());
        bad = input();
        bad.note = Some("x".repeat(1001));
        assert!(validate_profile(&bad).is_err());
        Ok(())
    }

    #[test]
    fn history_lists_only_what_changed() {
        let c = changes(
            Some(&json!({"position": "Kỹ thuật viên", "full_name": "Lan"})),
            Some(&json!({"position": "Quản lý", "full_name": "Lan"})),
        );
        assert_eq!(
            c,
            vec![Change {
                field: "position".into(),
                before: json!("Kỹ thuật viên"),
                after: json!("Quản lý"),
            }]
        );
        assert_eq!(changes(None, Some(&json!({"a": 1}))).len(), 1);
    }

    #[test]
    fn pay_terms_are_bounded() {
        let ok = PayInput {
            base_salary: 5_000_000,
            commission_rate_bp: 500,
            loyalty_bonus_monthly: 500_000,
            meal_allowance: 300_000,
        };
        assert!(validate_pay(&ok).is_ok());
        assert!(
            validate_pay(&PayInput {
                base_salary: -1,
                ..ok
            })
            .is_err()
        );
        assert!(
            validate_pay(&PayInput {
                commission_rate_bp: 10_001,
                ..ok
            })
            .is_err()
        );
        assert!(
            validate_pay(&PayInput {
                meal_allowance: MAX_ALLOWANCE + 1,
                ..ok
            })
            .is_err()
        );
    }
}
