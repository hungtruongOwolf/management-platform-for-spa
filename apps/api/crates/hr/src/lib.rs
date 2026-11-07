//! HR: daily attendance, daily reviews, monthly evaluations, bonuses/penalties and monthly
//! payroll with a closing lock (memory/wiki/product/specs/staff-management.md). Rules come from
//! the spa's sheet (memory/wiki/product/compensation-rules.md); the open contradictions are
//! configurable per spa in [`rules::PayrollRules`]. Staff profiles and pay terms live in `org`.

pub mod adjustment;
pub mod attendance;
pub mod calc;
pub mod day;
pub mod evaluation;
pub mod http;
pub mod me;
pub mod month;
pub mod payroll;
pub mod period;
mod repo;
pub mod review;
pub mod rules;
pub mod scope;
