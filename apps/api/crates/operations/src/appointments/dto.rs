//! Wire types of the appointments API (spec appointments.md).

use crate::dto::{VisitDto, VisitLineCreate};
use chrono::{DateTime, NaiveDate, Utc};
use sales::dto::{PaymentCreate, ServiceDiscountCreate};
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// A newcomer known only by name and phone until they arrive.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GuestCreate {
    pub full_name: String,
    pub phone: String,
}

/// Body of `POST /appointments`. Exactly one of `customer_id` / `guest`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AppointmentCreate {
    pub customer_id: Option<Uuid>,
    pub guest: Option<GuestCreate>,
    /// 1–5 services, in the order they will be done.
    pub service_ids: Vec<Uuid>,
    /// Preferred technician (optional while booked).
    pub staff_id: Option<Uuid>,
    /// On the spa's slot grid, inside opening hours; today up to 90 days ahead (past days: owner).
    pub starts_at: DateTime<Utc>,
    /// Default: the services' durations (empty = 60) rounded up to the slot. 1 slot – 480.
    pub duration_min: Option<u32>,
    /// Booking note, ≤ 500 characters. Not for health data.
    pub note: Option<String>,
}

/// Absent = unchanged, `null` = cleared.
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Body of `PATCH /appointments/{id}` (If-Match): reschedule or edit an open appointment.
#[derive(Debug, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AppointmentUpdate {
    pub starts_at: Option<DateTime<Utc>>,
    pub duration_min: Option<u32>,
    /// Absent = unchanged, null = no preferred technician (not allowed once arrived).
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    pub staff_id: Option<Option<Uuid>>,
    pub service_ids: Option<Vec<Uuid>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    pub note: Option<Option<String>>,
    /// Link a newcomer to the customer record created for them (replaces the guest name/phone).
    pub customer_id: Option<Uuid>,
}

/// Body of `POST /appointments/{id}/arrive`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AppointmentArrive {
    /// The technician serving the customer (required).
    pub staff_id: Uuid,
}

/// Body of `POST /appointments/{id}/cancel` and `/no-show`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AppointmentClose {
    /// Required, 1–500 characters.
    pub reason: Option<String>,
}

/// Body of `POST /appointments/{id}/complete`: the visit, exactly as `POST /visits` minus the
/// customer (the appointment's). Same rules: server prices, full payment, discount (D10).
#[derive(Debug, Deserialize, ToSchema)]
pub struct AppointmentComplete {
    /// When the service happened. Default: arrival time, else the booked start if already
    /// past, else now. Same day rules as `POST /visits` (owner may back-date).
    pub check_in_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
    pub lines: Vec<VisitLineCreate>,
    #[serde(default)]
    pub payments: Vec<PaymentCreate>,
    pub discount: Option<ServiceDiscountCreate>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct CalendarQuery {
    /// First business day `YYYY-MM-DD`; default today.
    pub from: Option<NaiveDate>,
    /// Last business day (inclusive); default `from`. At most 14 days.
    pub to: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppointmentStatusDto {
    Booked,
    Arrived,
    Done,
    NoShow,
    Cancelled,
}

/// "Đã thanh toán chưa": R1 collects in full when the visit is recorded.
#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStateDto {
    /// Booked / arrived: nothing collected yet.
    Unpaid,
    /// Done with one-off services: collected `paid_amount` in full.
    Paid,
    /// Done with package sessions only.
    NoCharge,
    /// Done, but its visit was voided since (money reversed).
    Voided,
    /// No-show / cancelled.
    None,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AppointmentServiceDto {
    pub service_id: Uuid,
    pub service_name: String,
    pub category_name: String,
    /// Current catalog duration; null = "theo tư vấn".
    pub duration_min: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AppointmentDto {
    pub id: Uuid,
    pub status: AppointmentStatusDto,
    pub business_date: NaiveDate,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub duration_min: i64,
    /// Null for a newcomer not yet linked to a customer record.
    pub customer_id: Option<Uuid>,
    /// The customer's name, or the newcomer's.
    pub customer_name: String,
    /// E.164.
    pub customer_phone: String,
    pub customer_gender: Option<crm::http::GenderDto>,
    pub is_guest: bool,
    pub staff_id: Option<Uuid>,
    pub staff_name: Option<String>,
    pub services: Vec<AppointmentServiceDto>,
    pub note: Option<String>,
    /// Why it was cancelled / missed.
    pub status_reason: Option<String>,
    pub arrived_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub visit_id: Option<Uuid>,
    pub payment_state: PaymentStateDto,
    /// Collected by the visit (after discount); 0 until done.
    pub paid_amount: i64,
    /// Send back as `If-Match` when editing.
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CalendarTechnician {
    pub id: Uuid,
    pub full_name: String,
}

/// `GET /appointments`: the spa's grid and its appointments over `from..=to`.
#[derive(Debug, Serialize, ToSchema)]
pub struct CalendarDto {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub today: NaiveDate,
    /// "HH:MM", spa time.
    pub opens_at: String,
    pub closes_at: String,
    pub slot_minutes: u32,
    pub time_zone: String,
    /// Columns of the day view; their count is the spa's capacity per instant.
    pub technicians: Vec<CalendarTechnician>,
    pub capacity: u32,
    /// Every status, earliest first.
    pub appointments: Vec<AppointmentDto>,
}

/// `POST /appointments/{id}/complete`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AppointmentCompleted {
    pub appointment: AppointmentDto,
    pub visit: VisitDto,
}
