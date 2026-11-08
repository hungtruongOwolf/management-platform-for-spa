//! Wire types of the appointments API (spec appointments.md).

use crate::dto::{VisitDto, VisitLineCreate, VisitPackageSaleCreate};
use chrono::{DateTime, NaiveDate, Utc};
use sales::dto::{PaymentCreate, ServiceDiscountCreate};
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// A newcomer known by name (and phone) until they arrive and get a customer record.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GuestCreate {
    /// 1–200 characters; for a companion it may be a description ("Chồng của chị Hà").
    pub full_name: String,
    /// Required for the person who books (`POST /appointments` without `group_with`, and
    /// `people[0]` of a group); optional for a served companion (`people[1..]`, `group_with`).
    pub phone: Option<String>,
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
    /// On the spa's slot grid, inside opening hours; today up to 90 days ahead. A past day, or a
    /// slot of today already over (the slot under way is fine), is `422 appointment.in_the_past`
    /// for everyone: a service already done is a back-dated visit.
    pub starts_at: DateTime<Utc>,
    /// Default: the services' exact durations (untimed alone = 60). 5 – 480 minutes.
    pub duration_min: Option<u32>,
    /// Booking note, ≤ 500 characters. Not for health data.
    pub note: Option<String>,
    /// 0–5 companions known only by a note ("chồng của chị Lan", 1–100 characters): shown on
    /// the booking, never a customer, never served or charged.
    #[serde(default)]
    pub companions: Vec<String>,
    /// Book a served companion of this open appointment (spec §10.10): the new appointment joins
    /// its group (one is opened if it had none) and must be on the same business day. The guest
    /// phone becomes optional.
    pub group_with: Option<Uuid>,
}

/// One person of a group booking: like `AppointmentCreate`, without the shared note.
/// `people[1..]` are the served companions of `people[0]`: their guest phone is optional.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupPersonCreate {
    pub customer_id: Option<Uuid>,
    pub guest: Option<GuestCreate>,
    /// 1–5 services, in the order they will be done.
    pub service_ids: Vec<Uuid>,
    pub staff_id: Option<Uuid>,
    /// Default: the group's `starts_at`. Must be on the same business day.
    pub starts_at: Option<DateTime<Utc>>,
    /// Default: this person's services' exact durations.
    pub duration_min: Option<u32>,
}

/// Body of `POST /appointments/group`: a family booked at once, saved atomically. Each person
/// becomes an appointment of their own sharing one `group_id`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AppointmentGroupCreate {
    /// Shared start, same rules as `POST /appointments`.
    pub starts_at: DateTime<Utc>,
    /// Booking note for everyone, ≤ 500 characters. Not for health data.
    pub note: Option<String>,
    /// 1–6 people; an existing customer appears at most once. With no `companions`, at least 2.
    pub people: Vec<GroupPersonCreate>,
    /// 0–5 people who come along known only by a note ("chồng của chị Lan", "gia đình của …",
    /// Q-U5): kept on the first person's booking, never served or charged.
    #[serde(default)]
    pub companions: Vec<String>,
}

/// `POST /appointments/group` → 201.
#[derive(Debug, Serialize, ToSchema)]
pub struct AppointmentGroupDto {
    pub group_id: Uuid,
    /// In the order of `people`.
    pub appointments: Vec<AppointmentDto>,
}

/// Body of `PUT /appointment-settings` (owner): the advisory "số khách tối đa mỗi khung giờ".
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AppointmentSettingsUpdate {
    /// 1–200, or `null` = no limit. Required (send `null` to clear).
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<u32>, required = true)]
    pub max_per_slot: Option<Option<u32>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AppointmentSettingsDto {
    /// Null = no limit.
    pub max_per_slot: Option<u32>,
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
    /// Note-only companions, the full list (absent = unchanged, `[]` = none). Same rules as
    /// `POST /appointments`.
    pub companions: Option<Vec<String>>,
}

/// Body of `POST /appointments/{id}/move-group` (If-Match of `{id}`): "Đổi cả nhóm".
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AppointmentGroupMove {
    /// The new start of `{id}`; every other member still booked shifts by the same amount.
    pub starts_at: DateTime<Utc>,
}

/// `POST /appointments/{id}/move-group` → 200.
#[derive(Debug, Serialize, ToSchema)]
pub struct AppointmentGroupMoved {
    pub group_id: Uuid,
    /// The members that moved (`{id}` included), earliest first.
    pub moved: Vec<AppointmentDto>,
    /// Members already arrived or done: left where they were.
    pub not_moved: Vec<AppointmentDto>,
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
    /// Packages sold in this checkout, as in `POST /visits` (spec §10.4.1, AC9.9).
    #[serde(default)]
    pub package_sales: Vec<VisitPackageSaleCreate>,
    /// Companions; omitted or empty = the booking's.
    #[serde(default)]
    pub companions: Vec<String>,
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

/// Where it stands on the desk's timeline (spec §10.1), derived when read: `waiting` (booked),
/// `in_service` (arrived, or its visit is within its service time), `done`, `no_show`,
/// `cancelled`, `voided` (the visit was voided).
#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseDto {
    Waiting,
    InService,
    Done,
    NoShow,
    Cancelled,
    Voided,
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
    pub phase: PhaseDto,
    /// Shared by the people of one group (family) booking; null = booked alone.
    pub group_id: Option<Uuid>,
    /// Companions known only by a note (never customers, never served or charged).
    pub companions: Vec<String>,
    pub business_date: NaiveDate,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub duration_min: i64,
    /// Null for a newcomer not yet linked to a customer record.
    pub customer_id: Option<Uuid>,
    /// The customer's name, or the newcomer's.
    pub customer_name: String,
    /// E.164; null for a served companion booked by name only (spec §10.10).
    pub customer_phone: Option<String>,
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
    /// Columns of the day view (active technicians).
    pub technicians: Vec<CalendarTechnician>,
    /// The owner's advisory "max customers per slot"; null = no limit. Bookings are never
    /// refused on it: the calendar shows "n / max" and warns.
    pub max_per_slot: Option<u32>,
    /// Every status, earliest first.
    pub appointments: Vec<AppointmentDto>,
    /// Visits of the days recorded without a booking (walk-ins), earliest first. Visits that
    /// completed an appointment are that appointment. Empty without `visit.read`.
    pub visits: Vec<CalendarVisitDto>,
}

/// A walk-in visit on the timeline: when it was served, by whom, and where it stands. Carries no
/// session note (health data) and no phone.
#[derive(Debug, Serialize, ToSchema)]
pub struct CalendarVisitDto {
    pub id: Uuid,
    pub phase: PhaseDto,
    pub business_date: NaiveDate,
    /// The service time (`check_in_at`).
    pub starts_at: DateTime<Utc>,
    /// `starts_at` + the services' exact durations (untimed alone = 60), 5 min – 8 h.
    pub ends_at: DateTime<Utc>,
    pub duration_min: i64,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub customer_gender: Option<crm::http::GenderDto>,
    /// The first line's technician (the calendar column).
    pub staff_id: Uuid,
    pub staff_name: String,
    pub services: Vec<AppointmentServiceDto>,
    /// `paid` / `no_charge` / `voided`.
    pub payment_state: PaymentStateDto,
    pub paid_amount: i64,
}

/// What an entry of "Lịch của tôi" is: a booking assigned to me, or a visit I served that no
/// booking of mine stands for.
#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MyEntryKind {
    Appointment,
    Visit,
}

/// A service on a technician's own calendar entry.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyEntryService {
    pub service_name: String,
    pub category_name: String,
    /// A session of the customer's package (visits only; bookings are always `false`).
    pub from_package: bool,
}

/// One block of a technician's own calendar. An allowlist: times, phase, the customer's name, the
/// services and the session note of a visit they worked on — no phone, no booking or customer
/// note, no money, no customer or staff ids.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyCalendarEntry {
    pub id: Uuid,
    pub kind: MyEntryKind,
    pub business_date: NaiveDate,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub duration_min: i64,
    pub phase: PhaseDto,
    pub customer_name: String,
    /// Bookings: what was booked; visits: only the lines I performed.
    pub services: Vec<MyEntryService>,
    /// People booked together in this booking's group (ADR-0009), this one included; `null` =
    /// booked alone, or a visit.
    pub group_size: Option<i64>,
    /// A visit recorded without any booking ("vãng lai").
    pub walk_in: bool,
    /// The session note of the visit (EM 2026-11-07 item 3), sent only when I performed one of
    /// its lines; `null` for a booking not yet done or a visit without a note.
    pub note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyCalendarData {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub today: NaiveDate,
    /// "HH:MM", spa time.
    pub opens_at: String,
    pub closes_at: String,
    pub slot_minutes: u32,
    /// Every phase, earliest first.
    pub entries: Vec<MyCalendarEntry>,
}

/// `GET /me/appointments`: the signed-in technician's own calendar.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyCalendar {
    /// `false`: this login is not linked to a staff profile (ask the owner).
    pub linked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<MyCalendarData>,
}

/// `POST /appointments/{id}/complete`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AppointmentCompleted {
    pub appointment: AppointmentDto,
    pub visit: VisitDto,
}
