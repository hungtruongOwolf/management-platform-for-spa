//! Reporting: read-only management views over the books (spec management-views).
//!
//! Nothing here writes. Every figure is computed in SQL from `visit`, `visit_line`, `sale`,
//! `sale_line`, `payment` and `refund`, scoped to the session's tenant (explicit filter + RLS), by
//! business day (`business_date`, which the writers computed in the spa's time zone).
//!
//! - [`day`]: the daily dashboard, the date-range report and the revenue book (ledger);
//! - [`customer`]: one customer's timeline, summary and per-package usage;
//! - [`overview`]: packages overview and the re-engagement list;
//! - [`activity`]: the owner's "Nhật ký thao tác" over the spa's audit trail (ADR-0021);
//! - [`history`]: "ai ghi, ai sửa" on each record, from the same trail (ADR-0022).

pub mod activity;
pub mod activity_dto;
mod cursor;
pub mod customer;
pub mod day;
pub mod dto;
pub mod history;
pub mod history_dto;
pub mod http;
pub mod notes;
pub mod notes_dto;
pub mod overview;
