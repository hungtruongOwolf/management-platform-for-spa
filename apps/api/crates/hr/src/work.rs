//! "Khách của tôi" (spec staff-management §10 US9): a staff member whose login is linked to a
//! profile sees the customers they served in a month and the appointments assigned to them.
//!
//! Scope is the session's own staff link (`staff.user_id = session user`), never a staff id from
//! the request (non-negotiable 3). Fields are an allowlist: dates, customer name, service names
//! and quantities. No prices, line values, phone numbers, session/health/booking notes or
//! customer ids — a technician holds none of `customer.read`, `visit.read`, `sale.read`.

use crate::month::Month;
use crate::period;
use crate::repo;
use chrono::{DateTime, Days, NaiveDate, Utc};
use kernel::{Actor, AppError, Permission, TenantTx};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

/// How far ahead "Lịch hẹn sắp tới" looks (today + 30 days).
const UPCOMING_DAYS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ServedLine {
    pub service_name: String,
    pub qty: i32,
    /// `true`: a session of the customer's package; `false`: a one-off service.
    pub from_package: bool,
}

/// A completed visit in which this person performed at least one line; `services` lists only
/// **their** lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ServedVisit {
    pub visit_id: Uuid,
    pub business_date: NaiveDate,
    pub check_in_at: DateTime<Utc>,
    pub customer_name: String,
    pub services: Vec<ServedLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpcomingStatus {
    Booked,
    Arrived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct UpcomingAppointment {
    pub appointment_id: Uuid,
    pub business_date: NaiveDate,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub status: UpcomingStatus,
    /// The customer's profile name, or the newcomer's name given at booking.
    pub customer_name: String,
    pub services: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyWorkData {
    pub month: String,
    pub today: NaiveDate,
    /// Newest first.
    pub served: Vec<ServedVisit>,
    /// Booked / arrived, from today to today + 30 days, earliest first.
    pub upcoming: Vec<UpcomingAppointment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyWork {
    /// `false`: this login is not linked to a staff profile (ask the owner).
    pub linked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<MyWorkData>,
}

/// `GET /me/work?month=` (`staff.read_own`).
pub async fn my_work(
    pool: &PgPool,
    actor: &Actor,
    month: Option<Month>,
) -> Result<MyWork, AppError> {
    actor.require(Permission::StaffReadOwn)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let Some(staff_id) = org::profile::own_staff_id(&mut tx, actor).await? else {
        tx.commit().await?;
        return Ok(MyWork {
            linked: false,
            data: None,
        });
    };
    let today = period::today(&mut tx).await?;
    let month = month.unwrap_or_else(|| Month::of(today));
    let lines = repo::served_lines(&mut tx, staff_id, month.first_day(), month.last_day()).await?;
    let horizon = today
        .checked_add_days(Days::new(UPCOMING_DAYS))
        .unwrap_or(today);
    let upcoming = repo::upcoming_appointments(&mut tx, staff_id, today, horizon).await?;
    tx.commit().await?;
    Ok(MyWork {
        linked: true,
        data: Some(MyWorkData {
            month: month.to_string(),
            today,
            served: group_visits(lines),
            upcoming: upcoming
                .into_iter()
                .filter_map(|a| {
                    let status = match a.status.as_str() {
                        "booked" => UpcomingStatus::Booked,
                        "arrived" => UpcomingStatus::Arrived,
                        _ => return None,
                    };
                    Some(UpcomingAppointment {
                        appointment_id: a.id,
                        business_date: a.business_date,
                        starts_at: a.starts_at,
                        ends_at: a.ends_at,
                        status,
                        customer_name: a.customer_name,
                        services: a.services,
                    })
                })
                .collect(),
        }),
    })
}

/// Rows arrive ordered by visit (newest first), then line position: fold consecutive rows of
/// one visit into one entry.
fn group_visits(rows: Vec<repo::ServedLineRow>) -> Vec<ServedVisit> {
    let mut out: Vec<ServedVisit> = Vec::new();
    for r in rows {
        let line = ServedLine {
            service_name: r.service_name,
            qty: r.qty,
            from_package: r.source == "package",
        };
        match out.last_mut() {
            Some(v) if v.visit_id == r.visit_id => v.services.push(line),
            _ => out.push(ServedVisit {
                visit_id: r.visit_id,
                business_date: r.business_date,
                check_in_at: r.check_in_at,
                customer_name: r.customer_name,
                services: vec![line],
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(visit: u128, name: &str) -> repo::ServedLineRow {
        repo::ServedLineRow {
            visit_id: Uuid::from_u128(visit),
            business_date: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap_or_default(),
            check_in_at: DateTime::<Utc>::UNIX_EPOCH,
            customer_name: "Nguyễn Thị Hoa".into(),
            service_name: name.into(),
            qty: 1,
            source: "one_off".into(),
        }
    }

    #[test]
    fn lines_of_one_visit_are_grouped_in_order() {
        let visits = group_visits(vec![
            row(2, "Gội đầu"),
            row(2, "Massage"),
            row(1, "Xông hơi"),
        ]);
        assert_eq!(visits.len(), 2);
        assert_eq!(
            visits.first().map(|v| v
                .services
                .iter()
                .map(|s| s.service_name.as_str())
                .collect::<Vec<_>>()),
            Some(vec!["Gội đầu", "Massage"])
        );
        assert_eq!(visits.get(1).map(|v| v.visit_id), Some(Uuid::from_u128(1)));
    }
}
