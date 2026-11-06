//! Self-service (spec staff-management US6): a staff member whose profile is linked to their
//! login sees **their own** month — attendance, daily reviews, evaluation, bonuses/penalties and
//! payslip — and nobody else's. The staff id comes from the session's user, never from the
//! request (non-negotiable 3).

use crate::adjustment::{self, Adjustment};
use crate::attendance::{self, StaffSheet};
use crate::calc::Payslip;
use crate::evaluation::{self, Evaluation};
use crate::month::Month;
use crate::payroll;
use crate::period;
use crate::review::{self, DailyReview};
use crate::rules;
use kernel::{Actor, AppError, Permission, TenantTx};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyMonth {
    pub month: String,
    pub closed: bool,
    pub attendance: StaffSheet,
    pub reviews: Vec<DailyReview>,
    pub evaluation: Option<Evaluation>,
    pub adjustments: Vec<Adjustment>,
    /// `None` before the person's pay terms are set.
    pub payslip: Option<Payslip>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyStaff {
    /// `false`: this login is not linked to a staff profile (ask the owner).
    pub linked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<MyMonth>,
}

/// `GET /me/staff?month=` (`staff.read_own`).
pub async fn my_month(
    pool: &PgPool,
    actor: &Actor,
    month: Option<Month>,
) -> Result<MyStaff, AppError> {
    actor.require(Permission::StaffReadOwn)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let Some(staff_id) = org::profile::own_staff_id(&mut tx, actor).await? else {
        tx.commit().await?;
        return Ok(MyStaff {
            linked: false,
            data: None,
        });
    };
    let profile = org::profile::profiles_in(&mut tx)
        .await?
        .into_iter()
        .find(|p| p.id == staff_id)
        .ok_or_else(|| AppError::Internal("linked staff vanished".into()))?;
    let today = period::today(&mut tx).await?;
    let month = month.unwrap_or_else(|| Month::of(today));
    let (rules, _) = rules::load(&mut tx).await?;
    let info = period::info(&mut tx, month).await?;
    let days = attendance::days_in(&mut tx, month, Some(staff_id)).await?;
    let reviews = review::reviews_in(&mut tx, month, Some(staff_id)).await?;
    let evaluation = evaluation::evaluations_in(&mut tx, month, Some(staff_id))
        .await?
        .pop();
    let adjustments = adjustment::adjustments_in(&mut tx, month, Some(staff_id)).await?;
    let has_pay = org::profile::pay_terms(&mut tx)
        .await?
        .iter()
        .any(|(id, _)| *id == staff_id);
    let payslip = if has_pay && period::employed_in(&profile, month) {
        payroll::payslips_in(&mut tx, month, Some(staff_id), &info, today)
            .await?
            .pop()
    } else {
        None
    };
    tx.commit().await?;
    Ok(MyStaff {
        linked: true,
        data: Some(MyMonth {
            month: month.to_string(),
            closed: info.closed,
            attendance: attendance::sheet_of(
                &profile,
                month,
                today,
                rules.default_shift_start,
                days,
            ),
            reviews,
            evaluation,
            adjustments,
            payslip,
        }),
    })
}
