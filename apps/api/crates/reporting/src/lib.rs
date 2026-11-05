//! Reporting: read-only management views over the books (spec management-views).
//!
//! Nothing here writes. Every figure is computed in SQL from `visit`, `visit_line`, `sale`,
//! `sale_line`, `payment` and `refund`, scoped to the session's tenant (explicit filter + RLS), by
//! business day (`business_date`, which the writers computed in the spa's time zone).
//!
//! - [`day`]: the daily dashboard, the date-range report and the revenue book (ledger);
//! - [`customer`]: one customer's timeline, summary and per-package usage;
//! - [`overview`]: packages overview and the re-engagement list.

mod cursor;
pub mod customer;
pub mod day;
pub mod dto;
pub mod http;
pub mod overview;
