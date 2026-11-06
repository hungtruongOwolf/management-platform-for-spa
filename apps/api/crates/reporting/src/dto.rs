//! Response and query shapes of the management views (the API contract, see backend.md §3b).
//! Money is integer đồng. Fields behind `report.revenue.read` are `Option` and left out of the
//! JSON entirely when the caller lacks it (entitled data only, never hidden on the client).

use chrono::{DateTime, NaiveDate, Utc};
use crm::http::{FollowUpChannelDto, FollowUpOutcomeDto, GenderDto};
use packages::http::{CustomerPackageDto, PackageStatusDto};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

// ------------------------------------------------------------------------------- money shapes

/// One amount split by how the money moved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct MethodSplit {
    pub cash: i64,
    pub bank_transfer: i64,
    /// cash + bank_transfer.
    pub total: i64,
}

impl MethodSplit {
    pub fn add(&mut self, method: &str, amount: i64) {
        match method {
            "cash" => self.cash += amount,
            _ => self.bank_transfer += amount,
        }
        self.total += amount;
    }

    pub fn minus(self, other: Self) -> Self {
        Self {
            cash: self.cash - other.cash,
            bank_transfer: self.bank_transfer - other.bank_transfer,
            total: self.total - other.total,
        }
    }

    pub fn plus(self, other: Self) -> Self {
        Self {
            cash: self.cash + other.cash,
            bank_transfer: self.bank_transfer + other.bank_transfer,
            total: self.total + other.total,
        }
    }
}

/// One amount split like the paper revenue book: package sales vs one-off services.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct KindSplit {
    pub package: i64,
    pub one_off: i64,
}

impl KindSplit {
    pub fn add(&mut self, kind: SaleKind, amount: i64) {
        match kind {
            SaleKind::Package => self.package += amount,
            SaleKind::OneOff => self.one_off += amount,
        }
    }

    pub fn minus(self, other: Self) -> Self {
        Self {
            package: self.package - other.package,
            one_off: self.one_off - other.one_off,
        }
    }

    pub fn plus(self, other: Self) -> Self {
        Self {
            package: self.package + other.package,
            one_off: self.one_off + other.one_off,
        }
    }
}

/// Money of a day or a period, cash basis, from the append-only `payment` and `refund` rows.
/// A void never deletes the original payment: it adds a reversal on the day of the void, so a
/// sale voided the next day still counts on its own day and shows as a void on the next.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct Revenue {
    /// Money received (`payment.kind = 'payment'`), discounts already taken off.
    pub collected: MethodSplit,
    pub collected_by_kind: KindSplit,
    /// Reversals of voided sales, booked on the day of the void.
    pub voids: MethodSplit,
    pub voids_by_kind: KindSplit,
    /// Money paid back when a package was cancelled (always a package).
    pub refunds: MethodSplit,
    /// collected − voids − refunds.
    pub net: MethodSplit,
    pub net_by_kind: KindSplit,
}

impl Revenue {
    /// Fills the derived `net` fields from the three books.
    pub fn settle(mut self) -> Self {
        self.net = self.collected.minus(self.voids).minus(self.refunds);
        self.net_by_kind = self
            .collected_by_kind
            .minus(self.voids_by_kind)
            .minus(KindSplit {
                package: self.refunds.total,
                one_off: 0,
            });
        self
    }

    pub fn plus(self, o: Self) -> Self {
        Self {
            collected: self.collected.plus(o.collected),
            collected_by_kind: self.collected_by_kind.plus(o.collected_by_kind),
            voids: self.voids.plus(o.voids),
            voids_by_kind: self.voids_by_kind.plus(o.voids_by_kind),
            refunds: self.refunds.plus(o.refunds),
            net: self.net.plus(o.net),
            net_by_kind: self.net_by_kind.plus(o.net_by_kind),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SaleKind {
    /// A package sale (`POST /sales`).
    Package,
    /// The one-off services of a visit.
    OneOff,
}

impl SaleKind {
    pub fn parse(s: &str) -> Self {
        if s == "package" {
            Self::Package
        } else {
            Self::OneOff
        }
    }
}

// --------------------------------------------------------------------------- daily dashboard

/// Operational counts (no money): visible with `report.read`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct DayCounts {
    /// Distinct customers with at least one completed visit.
    pub customers: i64,
    /// Completed visits (voided ones are in `voided_visits`).
    pub visits: i64,
    /// Visits cancelled ("Hủy lượt"); visits replaced by "Sửa lượt" are not cancellations.
    pub voided_visits: i64,
    /// Visits replaced by "Sửa lượt" ("đã sửa"); their newest version counts in `visits`.
    pub corrected_visits: i64,
    /// Package sessions taken by completed visits.
    pub sessions_used: i64,
    /// One-off services performed in completed visits.
    pub one_off_services: i64,
    /// Packages sold in sales that are not voided.
    pub packages_sold: i64,
}

impl DayCounts {
    /// Sum of additive counts; `customers` is not additive and is set by the caller.
    pub fn plus(self, o: Self) -> Self {
        Self {
            customers: self.customers + o.customers,
            visits: self.visits + o.visits,
            voided_visits: self.voided_visits + o.voided_visits,
            corrected_visits: self.corrected_visits + o.corrected_visits,
            sessions_used: self.sessions_used + o.sessions_used,
            one_off_services: self.one_off_services + o.one_off_services,
            packages_sold: self.packages_sold + o.packages_sold,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TechnicianDay {
    pub staff_id: Uuid,
    pub name: String,
    /// Completed visits in which this technician did at least one line.
    pub visits: i64,
    pub sessions_used: i64,
    pub one_off_services: i64,
}

/// The headline numbers of another day, for "so với hôm qua / cùng thứ tuần trước".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct DayHeadline {
    pub date: NaiveDate,
    pub customers: i64,
    pub visits: i64,
    pub sessions_used: i64,
    pub packages_sold: i64,
    /// `revenue.net.total` of that day; only with `report.revenue.read`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_revenue: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct DayComparison {
    pub yesterday: DayHeadline,
    pub same_weekday_last_week: DayHeadline,
}

/// `GET /reports/daily`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct DailyReport {
    pub date: NaiveDate,
    pub counts: DayCounts,
    /// Only with `report.revenue.read`; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revenue: Option<Revenue>,
    /// Technicians who worked that day, most visits first.
    pub technicians: Vec<TechnicianDay>,
    pub compare: DayComparison,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DailyQuery {
    /// Business day `YYYY-MM-DD` (spa time zone); default today.
    pub date: Option<NaiveDate>,
}

// ------------------------------------------------------------------------------ range report

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Granularity {
    #[default]
    Day,
    /// ISO weeks, Monday first.
    Week,
    /// Calendar months.
    Month,
    /// Calendar years (multi-year views).
    Year,
}

impl Granularity {
    pub const fn as_sql(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }
}

/// Ready-made periods, computed on the server from the spa's own "today" (its time zone), so
/// every device agrees on what "this month" is. Each has a default granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RangePreset {
    /// Today and the 6 days before (by day).
    #[serde(rename = "last_7_days")]
    Last7Days,
    /// Today and the 29 days before (by day).
    #[serde(rename = "last_30_days")]
    Last30Days,
    /// 1st of this month → today (by day).
    ThisMonth,
    /// The whole previous calendar month (by day).
    LastMonth,
    /// 1 January → today (by month).
    ThisYear,
    /// 1st of the month 11 months ago → today: 12 calendar months (by month).
    #[serde(rename = "last_12_months")]
    Last12Months,
    /// 1 January last year → today: this and last calendar year (by month; `year` compares the two).
    #[serde(rename = "last_2_years")]
    Last2Years,
    /// 1 January two years ago → today: 3 calendar years (by month). Needs `report.long_range`.
    #[serde(rename = "last_3_years")]
    Last3Years,
    /// 1 January four years ago → today: 5 calendar years (by year). Needs `report.long_range`.
    #[serde(rename = "last_5_years")]
    Last5Years,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct RangeBucket {
    /// First day of the bucket (clipped to `from`).
    pub start: NaiveDate,
    /// Last day of the bucket (clipped to `to`).
    pub end: NaiveDate,
    /// `customers` = distinct customers inside this bucket.
    pub counts: DayCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revenue: Option<Revenue>,
    /// `revenue.net.total`, flat for line charts; only with `report.revenue.read`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_revenue: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TopService {
    pub service_id: Uuid,
    pub name: String,
    /// Current catalog category of the service (e.g. "Gội đầu dưỡng sinh").
    pub category_name: String,
    pub sessions_used: i64,
    pub one_off_services: i64,
    /// List price of the one-off services; only with `report.revenue.read`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_off_revenue: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TopPackage {
    pub plan_id: Option<Uuid>,
    pub name: String,
    /// Packages sold in sales that are not voided.
    pub sold: i64,
    /// Sum of their prices after discount; only with `report.revenue.read`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revenue: Option<i64>,
}

/// `GET /reports/range`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RangeReport {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub granularity: Granularity,
    /// The preset the range came from, if any.
    pub preset: Option<RangePreset>,
    /// Whole range; `customers` = distinct customers over the range.
    pub totals: DayCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revenue: Option<Revenue>,
    /// Every bucket of the range in order, empty ones included (continuous charts).
    pub series: Vec<RangeBucket>,
    /// Top 10 by sessions used + one-off services.
    pub top_services: Vec<TopService>,
    /// Top 10 by packages sold.
    pub top_packages: Vec<TopPackage>,
    /// Every technician who worked in the range, most visits first (same counting as the daily
    /// dashboard: a visit counts once per technician however many lines they did).
    pub technicians: Vec<TechnicianDay>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RangeQuery {
    /// First business day; default `to` − 6 days. Not with `preset`.
    pub from: Option<NaiveDate>,
    /// Last business day (inclusive); default today. Not with `preset`. Span: at most 366 days
    /// for `day` / `week`, at most 732 days (2 years) for `month` / `year`; 1830 days (5 years)
    /// with `report.long_range` (owner).
    pub to: Option<NaiveDate>,
    /// Bucket size of `series`; default `day`, or the preset's default.
    pub granularity: Option<Granularity>,
    /// A ready-made period instead of `from` / `to`.
    pub preset: Option<RangePreset>,
}

// ------------------------------------------------------------------------- revenue book (ledger)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LedgerEntry {
    /// A sale and the money received for it that day.
    Sale,
    /// The reversal of a voided sale, on the day of the void (negative amounts).
    Void,
    /// Money paid back for a cancelled package (negative amounts).
    Refund,
}

/// One line of the "sổ theo dõi doanh thu hằng ngày". Outgoing money is negative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LedgerRow {
    /// The sale id (`sale`, `void`) or the refund id (`refund`).
    pub id: Uuid,
    pub entry: LedgerEntry,
    pub business_date: NaiveDate,
    pub at: DateTime<Utc>,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub customer_gender: Option<GenderDto>,
    pub kind: SaleKind,
    /// "Dịch vụ 1 lần" column: the one-off services, comma-separated.
    pub one_off: Option<String>,
    /// "Liệu trình" column: the packages, comma-separated.
    pub package: Option<String>,
    pub bank_transfer: i64,
    pub cash: i64,
    /// "Chưa thanh toán": total − received (always 0 in R1, which collects in full).
    pub unpaid: i64,
    /// Discount reason, void reason or refund reason.
    pub note: Option<String>,
    /// The sale was voided later (its `void` row may be on another day).
    pub voided: bool,
    /// The visit the sale was made in (one-off lines, a package sold in the cart, an addition).
    pub visit_id: Option<Uuid>,
    /// Upsell attribution ("người giới thiệu / chốt"): the closer of a package sale, the
    /// referrers of the one-off lines of a service sale; null = "khách tự chọn".
    pub upsell_by: Option<String>,
}

/// Daily subtotal of the book, for every day of the range (not only the current page).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct LedgerDay {
    pub date: NaiveDate,
    pub bank_transfer: i64,
    pub cash: i64,
    pub unpaid: i64,
    /// bank_transfer + cash: equals `revenue.net.total` of `GET /reports/daily`.
    pub total: i64,
    pub entries: i64,
}

/// `GET /reports/ledger`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LedgerPage {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Book order: day, then time.
    pub rows: Vec<LedgerRow>,
    /// Days of the range that have entries, in order.
    pub days: Vec<LedgerDay>,
    /// Pass as `cursor` for the next rows; `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LedgerQuery {
    /// Default today.
    pub from: Option<NaiveDate>,
    /// Default `from`; at most 92 days after `from`.
    pub to: Option<NaiveDate>,
    pub cursor: Option<String>,
    /// 1–100, default 20.
    pub limit: Option<u32>,
}

// -------------------------------------------------------------------------------- customers

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryKind {
    Visit,
    /// A package sale.
    Purchase,
    VisitVoided,
    PurchaseVoided,
    /// Cancelled by the owner (US7), with or without a refund. Packages cancelled by a voided
    /// sale show as `purchase_voided` instead.
    PackageCancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LineSource {
    Package,
    OneOff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HistoryVisitLine {
    pub service_name: String,
    /// Current catalog category of the service.
    pub category_name: String,
    pub source: LineSource,
    /// The package the session came from (`source = package`).
    pub customer_package_id: Option<Uuid>,
    pub package_name: Option<String>,
    pub qty: i32,
    /// Sessions left in that package item right after this visit.
    pub sessions_left_after: Option<i32>,
    pub staff_id: Uuid,
    pub staff_name: String,
    /// Price of a one-off line (0 for a package session).
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HistoryVisit {
    pub id: Uuid,
    pub voided: bool,
    pub check_in_at: DateTime<Utc>,
    pub lines: Vec<HistoryVisitLine>,
    /// Money received for the one-off lines (before any void).
    pub paid: MethodSplit,
    pub void_reason: Option<String>,
    pub voided_at: Option<DateTime<Utc>>,
    /// "Sửa lượt": the voided visit this one replaces.
    pub corrects_visit_id: Option<Uuid>,
    /// "Sửa lượt": the visit that replaced this (voided) one.
    pub corrected_by_visit_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HistoryPurchaseLine {
    pub description: String,
    pub qty: i32,
    pub line_total: i64,
    pub customer_package_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HistoryPurchase {
    pub sale_id: Uuid,
    pub voided: bool,
    pub lines: Vec<HistoryPurchaseLine>,
    pub subtotal: i64,
    pub discount_amount: i64,
    pub discount_reason: Option<String>,
    pub total: i64,
    /// Money received (before any void).
    pub paid: MethodSplit,
    pub closer_name: Option<String>,
    pub void_reason: Option<String>,
    pub voided_at: Option<DateTime<Utc>>,
    /// The visit this package was sold in, if any.
    pub visit_id: Option<Uuid>,
    /// Bought for someone else (US13): who the packages belong to.
    pub beneficiary_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HistoryVoid {
    /// The voided visit or sale.
    pub id: Uuid,
    pub reason: String,
    /// Money given back by reversal (positive amounts).
    pub reversed: MethodSplit,
    /// Package sessions put back (visit voids).
    pub sessions_restored: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HistoryRefund {
    pub amount: i64,
    pub method: String,
    pub business_date: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HistoryCancellation {
    pub customer_package_id: Uuid,
    pub package_name: String,
    pub reason: String,
    pub refund: Option<HistoryRefund>,
}

/// One entry of a customer's timeline; exactly one of the detail fields is set, per `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HistoryEvent {
    pub kind: HistoryKind,
    pub at: DateTime<Utc>,
    pub business_date: NaiveDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit: Option<HistoryVisit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchase: Option<HistoryPurchase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub void: Option<HistoryVoid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation: Option<HistoryCancellation>,
}

/// `GET /customers/{id}/history`: newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HistoryPage {
    pub customer_id: Uuid,
    pub events: Vec<HistoryEvent>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HistoryQuery {
    /// Service, category, package or staff name (accent-insensitive, 2–100 characters).
    pub q: Option<String>,
    /// Only events of this kind.
    pub kind: Option<HistoryKind>,
    /// First business day of the event (inclusive).
    pub from: Option<NaiveDate>,
    /// Last business day of the event (inclusive).
    pub to: Option<NaiveDate>,
    pub cursor: Option<String>,
    /// 1–100, default 20.
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct PackageCounts {
    pub active: i64,
    pub expired: i64,
    pub exhausted: i64,
    pub cancelled: i64,
}

/// `GET /customers/{id}/summary`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct CustomerStats {
    pub customer_id: Uuid,
    pub first_visit_on: Option<NaiveDate>,
    pub last_visit_on: Option<NaiveDate>,
    /// Days from the last completed visit to today.
    pub days_since_last_visit: Option<i64>,
    /// Completed visits.
    pub visit_count: i64,
    pub voided_visit_count: i64,
    pub sessions_used: i64,
    /// Money received from this customer (before voids and refunds).
    pub paid: i64,
    pub voided: i64,
    pub refunded: i64,
    /// paid − voided − refunded.
    pub net_spent: i64,
    pub packages: PackageCounts,
    /// Remaining sessions across active packages.
    pub sessions_remaining: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PackageUse {
    pub visit_id: Uuid,
    pub visit_line_id: Uuid,
    pub business_date: NaiveDate,
    pub check_in_at: DateTime<Utc>,
    pub item_id: Uuid,
    pub service_name: String,
    /// Current catalog category of the service.
    pub category_name: String,
    pub qty: i32,
    pub sessions_left_after: Option<i32>,
    pub staff_id: Uuid,
    pub staff_name: String,
    /// The visit was voided and its sessions were put back.
    pub voided: bool,
}

/// `GET /customer-packages/{id}/usage`: one page of the paper "sổ theo dõi liệu trình".
#[derive(Debug, Serialize, ToSchema)]
pub struct PackageUsage {
    pub package: CustomerPackageDto,
    /// Oldest first, like the paper page (STT 1, 2, 3…).
    pub uses: Vec<PackageUse>,
}

// -------------------------------------------------------------------------------- overview

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageStatusFilter {
    Active,
    Expired,
    Exhausted,
    Cancelled,
}

impl PackageStatusFilter {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PackageOverviewRow {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub customer_phone: String,
    pub customer_gender: Option<GenderDto>,
    pub name: String,
    pub status: PackageStatusDto,
    pub sessions_total: i64,
    pub sessions_remaining: i64,
    pub starts_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    /// Days from today to `expires_on` (negative once expired).
    pub days_to_expiry: Option<i64>,
    /// Last completed visit that used this package.
    pub last_used_on: Option<NaiveDate>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct PackageOverviewCounts {
    pub active: i64,
    /// Active packages expiring within `expiring_within_days` (default 30).
    pub expiring_soon: i64,
    pub expired: i64,
    pub exhausted: i64,
    pub cancelled: i64,
}

/// `GET /packages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PackageOverviewPage {
    pub items: Vec<PackageOverviewRow>,
    /// Over the whole spa (or the `q` match), ignoring `status` and the cursor.
    pub counts: PackageOverviewCounts,
    /// The window `counts.expiring_soon` used.
    pub expiring_within_days: u32,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PackagesQuery {
    /// Derived status; default all.
    pub status: Option<PackageStatusFilter>,
    /// 1–365. Set: only active packages expiring within that many days, soonest first.
    /// Also the window of `counts.expiring_soon` (default 30).
    pub expiring_within_days: Option<u32>,
    /// Customer name or package name (accent-insensitive, ≥ 2 characters), or phone digits (≥ 3).
    pub q: Option<String>,
    pub cursor: Option<String>,
    /// 1–100, default 20.
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InactiveScope {
    /// Only customers with sessions left on an active package.
    WithSessions,
    /// Every customer who ever came or paid: one-off and package customers alike (default since
    /// EM round 3, 2026-11-05).
    #[default]
    All,
}

/// What kind of customer a re-engagement row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CustomerKind {
    /// Has (or had) a package that was not voided: active, used up, expired or cancelled.
    Package,
    /// Only ever came or paid per session ("khách lẻ").
    OneOff,
}

/// `GET /customers/inactive?kind=`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CustomerKindFilter {
    #[default]
    All,
    Package,
    OneOff,
}

/// `GET /customers/inactive?follow_up=`, judged on the follow-ups of the current absence
/// (recorded on or after `last_seen_on`; a visit since then starts a new absence).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FollowUpFilter {
    #[default]
    All,
    /// Needs a call today: never contacted in this absence, or `due_on` ≤ today (a planned date
    /// that has come, or 90 days after a "not interested").
    Due,
    /// No follow-up recorded in this absence.
    NotContacted,
    /// Contacted, next follow-up planned after today.
    Scheduled,
}

/// `GET /customers/inactive?sort=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InactiveSort {
    /// Who to call first: never contacted in this absence, then `next_follow_up_on` soonest,
    /// then contacted with no date planned; ties by longest absence.
    Due,
    /// Longest absence first.
    Absent,
}

/// The latest follow-up of the current absence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LastFollowUp {
    pub on: NaiveDate,
    pub channel: FollowUpChannelDto,
    pub outcome: FollowUpOutcomeDto,
    /// Name of the user who recorded it.
    pub by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct InactiveCustomer {
    pub customer_id: Uuid,
    pub full_name: String,
    pub phone: String,
    pub gender: Option<GenderDto>,
    pub kind: CustomerKind,
    pub last_visit_on: Option<NaiveDate>,
    /// Latest of: last completed visit, last sale not voided, last package start.
    pub last_seen_on: NaiveDate,
    pub days_absent: i64,
    pub active_packages: i64,
    pub sessions_remaining: i64,
    /// Latest follow-up since `last_seen_on`; `null` = not contacted in this absence.
    pub last_follow_up: Option<LastFollowUp>,
    /// The date that follow-up planned for the next try (`null` = none planned).
    pub next_follow_up_on: Option<NaiveDate>,
    /// When this contacted customer is due again: `next_follow_up_on`, or for `not_interested`
    /// without a date, contact day + 90 (shown once more then). `null` = not contacted in this
    /// absence (already due) or nothing due.
    pub due_on: Option<NaiveDate>,
}

/// `GET /customers/inactive`: longest absence first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct InactivePage {
    pub days: u32,
    pub items: Vec<InactiveCustomer>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InactiveQuery {
    /// Not seen for at least this many days (1–365, default 7).
    pub days: Option<u32>,
    /// Default `all`.
    pub scope: Option<InactiveScope>,
    /// Default `all`.
    pub kind: Option<CustomerKindFilter>,
    /// Default `all`.
    pub follow_up: Option<FollowUpFilter>,
    /// Default `due` when `follow_up` is `due` or `scheduled`, else `absent`.
    pub sort: Option<InactiveSort>,
    /// Customer name (accent-insensitive, ≥ 2 characters) or phone digits (≥ 3).
    pub q: Option<String>,
    pub cursor: Option<String>,
    /// 1–100, default 20.
    pub limit: Option<u32>,
}
