//! Org (minimal, R1-1): the spa's staff, for the closer and technician pickers.
//! Staff are not user accounts: a technician who never logs in is still staff.

pub mod http;
pub mod provisioning;
mod repo;
pub mod service;

pub use service::{ActiveStaff, StaffStatus};
