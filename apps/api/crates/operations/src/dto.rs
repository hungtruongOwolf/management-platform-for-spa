//! Wire types of the visits API. The stored idempotent response is a `VisitDto`, so a replay is
//! exactly what the first request returned.

use catalog::PriceVariant;
use chrono::{DateTime, NaiveDate, Utc};
use sales::dto::{PaymentCreate, PaymentDto};
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Body of `POST /visits`. No amount is read from the client: unknown fields such as `unit_price`
/// or `total` are ignored, one-off prices come from the catalog and the customer's gender.
#[derive(Debug, Deserialize, ToSchema)]
pub struct VisitCreate {
    pub customer_id: Uuid,
    /// When the service happened (giờ làm), not when it is typed. Defaults to now. At most
    /// 5 minutes ahead of the server and on today's business day; with `visit.backdate` (owner)
    /// up to 366 days back, and the visit, its sale and payments then belong to that day.
    pub check_in_at: Option<DateTime<Utc>>,
    /// Discount on the one-off lines (D10: `sale.discount` up to the spa's cap,
    /// `sale.discount_unlimited` beyond; reason required).
    pub discount: Option<sales::dto::ServiceDiscountCreate>,
    /// Session note incl. reactions (health data): needs `customer.health_notes.write`.
    pub note: Option<String>,
    /// 1–20 service lines.
    pub lines: Vec<VisitLineCreate>,
    /// For the one-off lines: 0–2 payments, one per method, summing exactly to their total.
    #[serde(default)]
    pub payments: Vec<PaymentCreate>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VisitLineSourceDto {
    /// Consumes sessions of a customer package.
    Package,
    /// Paid now at the listed price.
    OneOff,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VisitLineCreate {
    pub service_id: Uuid,
    pub source: VisitLineSourceDto,
    /// `package` lines: the package item to use; omitted = the customer's usable package for this
    /// service that expires soonest, then the oldest (AC4.3).
    pub customer_package_item_id: Option<Uuid>,
    /// Technician who performed the line (required, must be working here).
    pub staff_id: Option<Uuid>,
    /// Sessions / times, 1–10 (default 1).
    pub qty: Option<u32>,
    /// `one_off` lines for a customer without a recorded gender: which gender price applies.
    pub price_variant: Option<PriceVariant>,
    /// `package` lines: use an expired package anyway (needs `package.use_expired_override`, audited).
    pub expiry_override_reason: Option<String>,
}

/// Absent = unchanged, `null` = cleared.
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Body of `PATCH /visits/{id}` (with `If-Match`): corrections that do not touch money or
/// sessions ("Sửa lượt", direct edit, spec US6b).
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VisitUpdate {
    /// Absent = unchanged, null or empty = cleared. Needs `customer.health_notes.write`.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    pub note: Option<Option<String>>,
    /// New service time (giờ làm): on the visit's own business day, not in the future.
    pub check_in_at: Option<DateTime<Utc>>,
    /// Technician corrections, by line id.
    #[serde(default)]
    pub lines: Vec<VisitLineUpdate>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VisitLineUpdate {
    pub id: Uuid,
    pub staff_id: Uuid,
}

/// Body of `POST /visits/{id}/correct` (with `If-Match` + `Idempotency-Key`): the visit as it
/// should have been recorded, plus the reason. The old visit is voided and this one recorded in
/// one transaction (spec US6b), with the rules and permissions of `POST /visits`. No amount is read
/// from the client.
#[derive(Debug, Deserialize, ToSchema)]
pub struct VisitCorrection {
    /// Why (1–490 characters); the old visit keeps it as "Sửa lượt: …".
    pub reason: Option<String>,
    /// May differ from the old visit's customer (recorded for the wrong person).
    pub customer_id: Uuid,
    /// Service time; absent = the old visit's. Must stay on the old visit's business day.
    pub check_in_at: Option<DateTime<Utc>>,
    pub discount: Option<sales::dto::ServiceDiscountCreate>,
    /// Absent = keep the old visit's note (copied on the server, never sent to the browser);
    /// null or empty = no note; text = a new note (needs `customer.health_notes.write`).
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    pub note: Option<Option<String>>,
    /// 1–20 service lines, as in `POST /visits`.
    pub lines: Vec<VisitLineCreate>,
    #[serde(default)]
    pub payments: Vec<PaymentCreate>,
}

impl VisitCorrection {
    /// The reason, the note change (absent = keep) and the new visit as `POST /visits` reads it.
    pub fn into_parts(self) -> (Option<String>, Option<Option<String>>, VisitCreate) {
        (
            self.reason,
            self.note,
            VisitCreate {
                customer_id: self.customer_id,
                check_in_at: self.check_in_at,
                discount: self.discount,
                note: None,
                lines: self.lines,
                payments: self.payments,
            },
        )
    }
}

/// Response of `POST /visits/{id}/correct`.
#[derive(Debug, Serialize, ToSchema)]
pub struct VisitCorrected {
    /// The old visit, now `voided`, with `corrected_by_visit_id` set.
    pub voided: VisitDto,
    /// The corrected visit (`corrects_visit_id` = the old one).
    pub visit: VisitDto,
    /// New total − old total: > 0 collect more ("Thu thêm"), < 0 give back ("Hoàn"), 0 = same.
    pub difference: i64,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct VisitListQuery {
    /// Business day `YYYY-MM-DD`; default today.
    pub date: Option<NaiveDate>,
    /// Only the newest N visits of the day (1–500); default all. The front desk asks for one more
    /// than it shows to know whether "Xem tất cả" is needed, keeping a busy day's first paint light.
    pub limit: Option<u32>,
}

/// Upper bound of `VisitListQuery::limit`.
pub const VISIT_LIST_MAX_LIMIT: u32 = 500;

#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VisitStatusDto {
    Completed,
    Voided,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VisitLineDto {
    pub id: Uuid,
    pub service_id: Uuid,
    /// Service name at the visit (snapshot).
    pub service_name: String,
    /// Current catalog category of the service (e.g. "Gội đầu dưỡng sinh").
    pub category_name: String,
    pub source: VisitLineSourceDto,
    pub customer_package_id: Option<Uuid>,
    pub customer_package_item_id: Option<Uuid>,
    pub package_name: Option<String>,
    /// `package` lines: sessions left on the item right after this visit, of `sessions_total`.
    pub sessions_left_after: Option<i32>,
    pub sessions_total: Option<i32>,
    pub staff_id: Uuid,
    pub staff_name: String,
    pub qty: i32,
    /// `one_off` lines: which price was charged.
    pub price_variant: Option<PriceVariant>,
    /// One-off: listed price; package: value of one paid session.
    pub unit_price: i64,
    /// One-off: this line's share of the visit discount (0 for package lines).
    pub discount_amount: i64,
    /// One-off: qty × unit price − discount share (paid now); package: value of the sessions
    /// taken (bonus = 0).
    pub line_value: i64,
    pub expiry_override_reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VisitDto {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub customer_gender: Option<crm::http::GenderDto>,
    /// Business day of the service (`check_in_at`), which may be before `created_at` when the
    /// owner entered it late.
    pub business_date: NaiveDate,
    /// When the service happened (giờ làm).
    pub check_in_at: DateTime<Utc>,
    pub status: VisitStatusDto,
    /// Set when the visit was voided (US6): it stays in history with the reason.
    pub void_reason: Option<String>,
    pub voided_at: Option<DateTime<Utc>>,
    /// Set on a visit recorded by "Sửa lượt": the voided visit it replaces.
    pub corrects_visit_id: Option<Uuid>,
    /// Set on a voided visit that was corrected: the visit that replaced it. The newest version
    /// of a visit is the one without this field.
    pub corrected_by_visit_id: Option<Uuid>,
    /// Present only on a single visit and only for `customer.health_notes.read`; null = no note.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub note: Option<Option<String>>,
    /// Sale of the one-off lines (none when the visit only used package sessions).
    pub sale_id: Option<Uuid>,
    /// Sum of the one-off lines before discount.
    pub subtotal: i64,
    pub discount_amount: i64,
    pub discount_reason: Option<String>,
    /// Amount collected in this visit (one-off lines after discount).
    pub total: i64,
    pub payments: Vec<PaymentDto>,
    pub lines: Vec<VisitLineDto>,
    /// Send back as `If-Match` when correcting.
    pub version: i64,
    pub created_at: DateTime<Utc>,
}
