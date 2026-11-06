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
}

/// Body of `POST /sales/{id}/void` and `POST /visits/{id}/void`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VoidCreate {
    /// Why the record is voided (required, 1–500 characters); shown in history and audited.
    pub reason: Option<String>,
}
