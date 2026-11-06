//! Staff profiles and pay terms (spec staff-management US1). Profiles need `staff.manage`; pay
//! terms are salary data and are only ever read or written by holders of `payroll.read` as well
//! (non-negotiable 4: a caller without it never receives a salary figure, not even `null`).

use crate::repo::{self, AccountRow, PayRow, ProfileRow, ProfileWrite};
use crate::service::StaffStatus;
use chrono::{NaiveDate, NaiveTime};
use kernel::audit::{self, AuditEntry};
use kernel::{Actor, AppError, FieldError, Permission, PhoneE164, TenantTx};
use serde_json::json;
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
    pub joined_on: Option<NaiveDate>,
    pub left_on: Option<NaiveDate>,
    /// The login linked to this staff member (self-service), with its display name.
    pub user_id: Option<Uuid>,
    pub user_name: Option<String>,
    pub shift_start: Option<NaiveTime>,
    pub version: i64,
}

impl TryFrom<ProfileRow> for Profile {
    type Error = AppError;
    fn try_from(r: ProfileRow) -> Result<Self, AppError> {
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
            version: r.version,
        })
    }
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

/// A profile as its caller may see it: `pay` is `None` both when none is set and when the caller
/// lacks `payroll.read`; [`ProfileView::pay_visible`] tells the two apart for the DTO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileView {
    pub profile: Profile,
    pub pay: Option<PayTerms>,
    pub pay_visible: bool,
}

pub struct ProfileInput {
    pub full_name: String,
    pub position: String,
    pub phone: Option<String>,
    pub status: StaffStatus,
    pub joined_on: Option<NaiveDate>,
    pub left_on: Option<NaiveDate>,
    pub user_id: Option<Uuid>,
    pub shift_start: Option<NaiveTime>,
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
    match (input.status, input.left_on) {
        (StaffStatus::Left, None) => errors.push(FieldError::new("left_on", "required")),
        (StaffStatus::Active, Some(_)) => errors.push(FieldError::new("left_on", "must_be_empty")),
        _ => {}
    }
    if let (Some(joined), Some(left)) = (input.joined_on, input.left_on)
        && left < joined
    {
        errors.push(FieldError::new("left_on", "before_joined_on"));
    }
    if errors.is_empty() {
        Ok(CleanProfile {
            full_name,
            position,
            phone,
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

/// A login may be linked only if it is an active member of this spa and not linked to someone else.
async fn check_link(
    tx: &mut TenantTx,
    user_id: Option<Uuid>,
    staff_id: Uuid,
) -> Result<(), AppError> {
    let Some(user) = user_id else { return Ok(()) };
    if !repo::is_active_member(tx, user).await? {
        return Err(AppError::Validation(vec![FieldError::new(
            "user_id",
            "not_a_member",
        )]));
    }
    match repo::staff_of_user(tx, user).await? {
        Some(other) if other != staff_id => Err(AppError::ConflictWith {
            code: "staff.account_taken",
            existing_id: other,
        }),
        _ => Ok(()),
    }
}

fn audit_profile(p: &Profile) -> serde_json::Value {
    json!({
        "full_name": p.full_name, "position": p.position,
        "has_phone": p.phone.is_some(), "status": p.status.as_str(),
        "joined_on": p.joined_on, "left_on": p.left_on, "user_id": p.user_id,
        "shift_start": p.shift_start.map(|t| t.format("%H:%M").to_string()),
    })
}

fn audit_pay(p: &PayTerms) -> serde_json::Value {
    json!({
        "base_salary": p.base_salary, "commission_rate_bp": p.commission_rate_bp,
        "loyalty_bonus_monthly": p.loyalty_bonus_monthly, "meal_allowance": p.meal_allowance,
    })
}

fn view(actor: &Actor, profile: Profile, pay: Option<&PayRow>) -> ProfileView {
    let pay_visible = actor.can(Permission::PayrollRead);
    ProfileView {
        profile,
        pay: pay.filter(|_| pay_visible).map(PayTerms::from),
        pay_visible,
    }
}

/// `GET /staff/profiles` (`staff.manage`): everyone, active first; pay terms only for
/// `payroll.read` holders.
pub async fn list(pool: &PgPool, actor: &Actor) -> Result<Vec<ProfileView>, AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let rows = repo::profiles(&mut tx).await?;
    let pays = if actor.can(Permission::PayrollRead) {
        repo::pays(&mut tx).await?
    } else {
        Vec::new()
    };
    tx.commit().await?;
    rows.into_iter()
        .map(|r| {
            let pay = pays.iter().find(|p| p.staff_id == r.id);
            Ok(view(actor, Profile::try_from(r)?, pay))
        })
        .collect()
}

/// `GET /staff/{id}` (`staff.manage`).
pub async fn get(pool: &PgPool, actor: &Actor, id: Uuid) -> Result<ProfileView, AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let row = repo::profile(&mut tx, id, false)
        .await?
        .ok_or(AppError::NotFound)?;
    let pay = if actor.can(Permission::PayrollRead) {
        repo::pays(&mut tx)
            .await?
            .into_iter()
            .find(|p| p.staff_id == id)
    } else {
        None
    };
    tx.commit().await?;
    Ok(view(actor, Profile::try_from(row)?, pay.as_ref()))
}

/// `POST /staff` (`staff.manage`).
pub async fn create(
    pool: &PgPool,
    actor: &Actor,
    input: &ProfileInput,
) -> Result<ProfileView, AppError> {
    actor.require(Permission::StaffManage)?;
    let clean = validate_profile(input)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let id = Uuid::now_v7();
    check_link(&mut tx, input.user_id, id).await?;
    let branch = kernel::tenant::profile(&mut tx).await?.branch;
    repo::insert_profile(
        &mut tx,
        id,
        branch.as_uuid(),
        &ProfileWrite {
            full_name: &clean.full_name,
            position: &clean.position,
            phone_e164: clean.phone.as_ref().map(PhoneE164::as_str),
            status: input.status.as_str(),
            joined_on: input.joined_on,
            left_on: input.left_on,
            user_id: input.user_id,
            shift_start: input.shift_start,
        },
    )
    .await?;
    let created = Profile::try_from(
        repo::profile(&mut tx, id, false)
            .await?
            .ok_or_else(|| AppError::Internal("staff vanished".into()))?,
    )?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.create",
            entity_type: "staff",
            entity_id: id,
            before: None,
            after: Some(audit_profile(&created)),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(view(actor, created, None))
}

/// `PUT /staff/{id}` (`staff.manage`, `If-Match`).
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
    let before = Profile::try_from(
        repo::profile(&mut tx, id, true)
            .await?
            .ok_or(AppError::NotFound)?,
    )?;
    check_link(&mut tx, input.user_id, id).await?;
    let written = repo::update_profile(
        &mut tx,
        id,
        version,
        &ProfileWrite {
            full_name: &clean.full_name,
            position: &clean.position,
            phone_e164: clean.phone.as_ref().map(PhoneE164::as_str),
            status: input.status.as_str(),
            joined_on: input.joined_on,
            left_on: input.left_on,
            user_id: input.user_id,
            shift_start: input.shift_start,
        },
    )
    .await?;
    if !written {
        return Err(AppError::StaleVersion);
    }
    let after = Profile::try_from(
        repo::profile(&mut tx, id, false)
            .await?
            .ok_or_else(|| AppError::Internal("staff vanished".into()))?,
    )?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.update",
            entity_type: "staff",
            entity_id: id,
            before: Some(audit_profile(&before)),
            after: Some(audit_profile(&after)),
            reason: None,
        },
    )
    .await?;
    let pay = if actor.can(Permission::PayrollRead) {
        repo::pay_for_update(&mut tx, id).await?
    } else {
        None
    };
    tx.commit().await?;
    Ok(view(actor, after, pay.as_ref()))
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

/// Every profile, inside the caller's transaction. The caller authorizes.
pub async fn profiles_in(tx: &mut TenantTx) -> Result<Vec<Profile>, AppError> {
    repo::profiles(tx)
        .await?
        .into_iter()
        .map(Profile::try_from)
        .collect()
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
            status: StaffStatus::Active,
            joined_on: "2026-01-05".parse().ok(),
            left_on: None,
            user_id: None,
            shift_start: NaiveTime::from_hms_opt(9, 0, 0),
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
        Ok(())
    }

    #[test]
    fn leaving_needs_a_date_after_joining() {
        let mut left = input();
        left.status = StaffStatus::Left;
        assert!(validate_profile(&left).is_err(), "left without left_on");
        left.left_on = "2025-12-31".parse().ok();
        assert!(validate_profile(&left).is_err(), "left before joined");
        left.left_on = "2026-06-30".parse().ok();
        assert!(validate_profile(&left).is_ok());
        let mut active = input();
        active.left_on = "2026-06-30".parse().ok();
        assert!(validate_profile(&active).is_err(), "active with left_on");
        let mut phone = input();
        phone.phone = Some("12345".into());
        assert!(validate_profile(&phone).is_err());
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
