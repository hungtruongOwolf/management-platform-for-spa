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
    /// 0–2 payments, one per method, summing exactly to everything due: the one-off lines after
    /// discount plus every package sold in `package_sales`. The server splits them over the
    /// sales: cash pays the one-off sale first, then the packages in order; the rest is transfer.
    #[serde(default)]
    pub payments: Vec<PaymentCreate>,
    /// 0–3 packages sold in this checkout (spec §10.4.1, needs `package.sell`). Each becomes a
    /// sale of its own linked to the visit (`sale.visit_id`); a line with `package_ref` uses
    /// today's session from it ("Dùng gói này cho buổi hôm nay").
    #[serde(default)]
    pub package_sales: Vec<VisitPackageSaleCreate>,
    /// 0–5 companions known only by a note ("chồng của chị Lan"), 1–100 characters each: shown
    /// on the visit, never served or charged.
    #[serde(default)]
    pub companions: Vec<String>,
}

/// A package sold inside a checkout (`POST /visits`, `POST /appointments/{id}/complete`,
/// `POST /visits/{id}/additions`, `POST /visits/{id}/convert-to-package`). Same rules as
/// `POST /sales` for one package; the price comes from the catalog.
#[derive(Debug, Deserialize, ToSchema)]
pub struct VisitPackageSaleCreate {
    /// Client-chosen name (1–20 characters, unique in the request) that lines point at with
    /// `package_ref`; omitted when no line uses this package today.
    #[serde(rename = "ref")]
    pub package_ref: Option<String>,
    pub package_plan_id: Uuid,
    /// "Người giới thiệu / chốt" (upsell credit): any staff member working here; null = "khách tự
    /// chọn". The desk defaults to the technician serving the customer.
    pub closer_staff_id: Option<Uuid>,
    /// Omitted = the plan's gift; empty = none.
    pub gift_note: Option<String>,
    /// Discount on this package alone (D10; Q-U9: the manager cap applies per sale).
    pub discount: Option<sales::dto::DiscountCreate>,
    /// Who the package belongs to (US13); omitted = the visit's customer (the payer). A package
    /// for someone else cannot be used in this visit.
    pub beneficiary_customer_id: Option<Uuid>,
    /// "Ghi chú gói" of this package (F4), beside the visit's session note: ≤ 2000 characters,
    /// health data (encrypted at rest), needs `customer.health_notes.write`.
    pub note: Option<String>,
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
    /// `package` lines: take today's session from the package sold in this request under that
    /// `ref` (instead of `customer_package_item_id`).
    pub package_ref: Option<String>,
    /// `one_off` lines: who recommended this service ("người giới thiệu", upsell credit); null =
    /// "khách tự chọn". The technician (`staff_id`) keeps the credit for performing it.
    pub referrer_staff_id: Option<Uuid>,
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
    /// Absent = unchanged; otherwise the full list of companion notes (0–5).
    pub companions: Option<Vec<String>>,
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
    /// Companions (notes only), as in `POST /visits`.
    #[serde(default)]
    pub companions: Vec<String>,
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
                package_sales: Vec::new(),
                companions: self.companions,
            },
        )
    }
}

/// Body of `POST /visits/{id}/additions` (Idempotency-Key): services or packages sold after the
/// visit was saved ("Thêm dịch vụ vào lượt", US12). Adds lines and sales, never changes the
/// existing ones, needs no reason. Same rules as `POST /visits`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct VisitAdditions {
    /// 0–20 new lines (one-off or package); at least one line or one package sale.
    #[serde(default)]
    pub lines: Vec<VisitLineCreate>,
    /// 0–3 packages sold now, linked to the visit.
    #[serde(default)]
    pub package_sales: Vec<VisitPackageSaleCreate>,
    /// Discount on the new one-off lines.
    pub discount: Option<sales::dto::ServiceDiscountCreate>,
    /// Summing exactly to the new one-off lines plus the new packages.
    #[serde(default)]
    pub payments: Vec<PaymentCreate>,
}

impl VisitAdditions {
    /// The addition as `POST /visits` reads a checkout (the customer is the visit's).
    pub fn into_visit(self) -> VisitCreate {
        VisitCreate {
            customer_id: Uuid::nil(),
            check_in_at: None,
            discount: self.discount,
            note: None,
            lines: self.lines,
            payments: self.payments,
            package_sales: self.package_sales,
            companions: Vec::new(),
        }
    }
}

/// Body of `POST /visits/{id}/convert-to-package` (If-Match + Idempotency-Key): "Tính buổi hôm
/// nay vào gói" (US11, Q-U1 case by case). The one-off lines of the visit whose service the new
/// package covers become today's session of that package and their charge is reversed; every
/// other line is recorded again unchanged. Without this choice the desk simply sells the package
/// with `POST /visits/{id}/additions` and today's one-off stays.
#[derive(Debug, Deserialize, ToSchema)]
pub struct VisitConversion {
    /// The package (its `ref` is ignored); it must belong to the visit's customer.
    pub package_sale: VisitPackageSaleCreate,
    /// Only the top-up ("Khách đưa thêm"), in the method(s) the customer uses now: summing exactly
    /// to the package plus the one-off lines kept, minus what was already paid for the visit
    /// (empty when that covers it). The money already paid is recorded again by the server in its
    /// own method, so the day's cash / transfer split does not move.
    #[serde(default)]
    pub payments: Vec<PaymentCreate>,
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
    /// `one_off` lines: the sale that charged it (the visit's first sale or an addition).
    pub sale_id: Option<Uuid>,
    /// `one_off` lines: who recommended it (upsell credit); null = "khách tự chọn".
    pub referrer_staff_id: Option<Uuid>,
    pub referrer_name: Option<String>,
}

/// A package sold in a visit (`sale.visit_id`), as the visit shows it.
#[derive(Debug, Serialize, ToSchema)]
pub struct VisitPackageSaleDto {
    pub sale_id: Uuid,
    pub voided: bool,
    pub business_date: NaiveDate,
    /// Package names, in sale order.
    pub description: String,
    pub customer_package_ids: Vec<Uuid>,
    pub total: i64,
    /// "Người giới thiệu / chốt"; null = "khách tự chọn".
    pub closer_staff_id: Option<Uuid>,
    pub closer_name: Option<String>,
    /// Who the package belongs to when not the visit's customer (US13).
    pub beneficiary_customer_id: Option<Uuid>,
    pub beneficiary_name: Option<String>,
    pub payments: Vec<PaymentDto>,
    /// The sale's optimistic-lock version: the `If-Match` of `PATCH /sales/{id}` ("Sửa ghi chú gói").
    pub version: i64,
    /// "Ghi chú gói" (F4): present exactly when the visit's session note is (a visit or the day
    /// list read by `customer.health_notes.read`); null = no note. Never in an idempotent replay.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub note: Option<Option<String>>,
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
    /// "Đã sửa" (ADR-0022 D9): present only for `record.history.read`; `true` when the visit's
    /// time or a technician was changed, or "Sửa lượt" replaced it or made it. Equals the
    /// visit history's `summary.edits >= 1`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited: Option<bool>,
    /// Present (on a visit and in the day list) only for `customer.health_notes.read`; null = no
    /// note. Never in an idempotent replay.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub note: Option<Option<String>>,
    /// First sale of the one-off lines (none when the visit only used package sessions); later
    /// additions have sales of their own (`lines[].sale_id`).
    pub sale_id: Option<Uuid>,
    /// Sum of the one-off lines before discount.
    pub subtotal: i64,
    pub discount_amount: i64,
    /// Reasons of the discounts on the visit's one-off bills (its sale and each addition), in the
    /// order they were made, joined by "; ".
    pub discount_reason: Option<String>,
    /// Amount collected for the one-off lines (after discount), additions included. Packages sold
    /// in the visit are in `package_sales`.
    pub total: i64,
    /// Payments of the one-off sales (the first one and every addition).
    pub payments: Vec<PaymentDto>,
    /// Packages sold in this visit, each a sale of its own. A visit recorded by "Sửa lượt" also
    /// shows the packages still standing that the visits it replaced sold to the same customer:
    /// they stay with it.
    pub package_sales: Vec<VisitPackageSaleDto>,
    /// Companions known only by a note (never served or charged).
    pub companions: Vec<String>,
    pub lines: Vec<VisitLineDto>,
    /// Send back as `If-Match` when correcting.
    pub version: i64,
    pub created_at: DateTime<Utc>,
}
