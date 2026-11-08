//! Wire types of the sales API. The stored idempotent response is a `SaleDto`, so a replay is
//! byte-for-byte what the first request returned.

use chrono::{DateTime, NaiveDate, Utc};
use packages::http::CustomerPackageDto;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Body of `POST /sales`. Prices are never read from the client: unknown fields such as
/// `unit_price` or `total` are ignored and every amount is computed from the catalog.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SaleCreate {
    /// Who pays.
    pub customer_id: Uuid,
    /// Who the packages belong to (US13: "mua cho người khác"); omitted = the payer. Must be a
    /// customer of this spa (`404` otherwise).
    pub beneficiary_customer_id: Option<Uuid>,
    /// "Người giới thiệu / chốt" (gets the upsell credit): any staff member currently working
    /// here (owner, manager, technician); null or omitted = "khách tự chọn" (nobody is credited).
    pub closer_staff_id: Option<Uuid>,
    /// 1–10 packages.
    pub lines: Vec<SaleLineCreate>,
    pub discount: Option<DiscountCreate>,
    /// 0–2 payments, one per method, summing exactly to the total (R1 collects in full).
    pub payments: Vec<PaymentCreate>,
    /// "Ghi chú gói" (F4): an optional note on this package sale, ≤ 2000 characters. Often holds
    /// health information, so it is encrypted at rest and needs `customer.health_notes.write`;
    /// empty or blank = none.
    pub note: Option<String>,
}

/// Body of `POST /sales/checkout` (EM 2026-11-07 item 4): a Quầy cart holding only packages,
/// saved at once. Each package is its own sale — its own plan, beneficiary, closer and discount,
/// exactly like `package_sales[]` of `POST /visits` — and one payment step pays them all.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SaleCheckoutCreate {
    /// Who pays (every sale's `customer_id`).
    pub customer_id: Uuid,
    /// 1–3 packages, in cart order.
    pub package_sales: Vec<CheckoutPackageCreate>,
    /// 0–2 payments, one per method, summing exactly to every package's total. The server splits
    /// them over the sales like a visit's checkout: cash pays the sales in order first, the rest is
    /// bank transfer.
    pub payments: Vec<PaymentCreate>,
}

/// One package of `POST /sales/checkout`: the same fields as a package sold in a visit.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CheckoutPackageCreate {
    pub package_plan_id: Uuid,
    /// "Người giới thiệu / chốt": any staff member working here; null = "khách tự chọn".
    pub closer_staff_id: Option<Uuid>,
    /// Omitted = the plan's gift; empty = none.
    pub gift_note: Option<String>,
    /// Discount on this package alone (D10; the manager cap applies per sale).
    pub discount: Option<DiscountCreate>,
    /// Who the package belongs to ("Mua cho người khác"); omitted = the payer.
    pub beneficiary_customer_id: Option<Uuid>,
    /// "Ghi chú gói" of this package alone (F4), as `note` of `POST /sales`.
    pub note: Option<String>,
}

/// `POST /sales/checkout` → 201: the sales of the checkout, in request order.
#[derive(Debug, Serialize, ToSchema)]
pub struct SaleCheckoutDto {
    pub sales: Vec<SaleDto>,
    /// What the customer paid in all (the sum of the sales' totals).
    pub total: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaleLineCreate {
    pub package_plan_id: Uuid,
    /// Gift note for this package; omitted = the plan's, empty = none.
    pub gift_note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DiscountCreate {
    /// đồng, ≤ subtotal.
    pub amount: i64,
    /// Required when `amount` > 0.
    pub reason: Option<String>,
}

/// Discount on the one-off lines of a visit (spec appointments.md §5): **either** `percent` or
/// `amount`, never both. The server computes the amount (`percent` → floor of subtotal × % / 100).
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceDiscountCreate {
    /// 1–100.
    pub percent: Option<u32>,
    /// đồng, 1 ≤ amount ≤ subtotal of the one-off lines.
    pub amount: Option<i64>,
    /// Required (1–500 characters).
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethodDto {
    Cash,
    BankTransfer,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PaymentCreate {
    pub method: PaymentMethodDto,
    /// đồng, > 0.
    pub amount: i64,
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SaleStatusDto {
    Open,
    Voided,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SaleLineTypeDto {
    Package,
    Service,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SaleLineDto {
    pub id: Uuid,
    pub line_type: SaleLineTypeDto,
    pub package_plan_id: Option<Uuid>,
    pub customer_package_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
    pub description: String,
    pub qty: i32,
    pub unit_price: i64,
    pub line_total: i64,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentKindDto {
    Payment,
    Reversal,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaymentDto {
    pub id: Uuid,
    pub kind: PaymentKindDto,
    /// `reversal` rows: the payment this one cancels (same method and amount).
    pub reversal_of: Option<Uuid>,
    pub method: PaymentMethodDto,
    pub amount: i64,
    pub reference: Option<String>,
    pub paid_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SaleDto {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub business_date: NaiveDate,
    pub subtotal: i64,
    pub discount_amount: i64,
    pub discount_reason: Option<String>,
    pub total: i64,
    pub amount_paid: i64,
    pub status: SaleStatusDto,
    /// Set when the sale was voided (US6); its payments then each have a `reversal`.
    pub void_reason: Option<String>,
    pub voided_at: Option<DateTime<Utc>>,
    pub closer_staff_id: Option<Uuid>,
    pub closer_name: Option<String>,
    /// The visit this sale was made in (its one-off lines, a package sold in the cart, an
    /// addition); null = a sale at the desk without a visit.
    pub visit_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub lines: Vec<SaleLineDto>,
    pub payments: Vec<PaymentDto>,
    /// Packages this sale created.
    pub customer_packages: Vec<CustomerPackageDto>,
    /// "Ghi chú gói" (F4). Present (`GET /sales?date=`, `GET /sales/{id}`, `PATCH /sales/{id}`)
    /// only for `customer.health_notes.read`; null = no note. Never in an idempotent replay: the
    /// responses of `POST /sales`, `POST /sales/checkout` and the void leave it out.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub note: Option<Option<String>>,
}

/// Body of `PATCH /sales/{id}` ("Sửa ghi chú gói", F4): the note is the only field a package
/// sale lets the desk change in place (money and packages change through a void).
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SaleUpdate {
    /// The new note (≤ 2000 characters); null or blank = no note. Needs
    /// `customer.health_notes.write`.
    pub note: Option<String>,
}

/// `GET /sales?date=`: the package sales of one business day made at the desk without a visit.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct SaleListQuery {
    /// Business day (`YYYY-MM-DD`). Default: today.
    pub date: Option<NaiveDate>,
}

/// One package sale of the day list at Quầy (F6, live test 2026-11-08): a cart of packages saved
/// without a visit, with who bought it and who uses it.
#[derive(Debug, Serialize, ToSchema)]
pub struct DaySaleDto {
    pub sale: SaleDto,
    /// The payer's name ("Khách vãng lai" for a nameless profile).
    pub customer_name: String,
    pub customer_gender: Option<crm::http::GenderDto>,
    /// Who uses the package when it was bought for someone else (US13); null = the payer.
    pub beneficiary_name: Option<String>,
}

/// Body of `POST /sales/{id}/void` and `POST /visits/{id}/void`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VoidCreate {
    /// Why the record is voided (required, 1–500 characters); shown in history and audited.
    pub reason: Option<String>,
}
