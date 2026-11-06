//! Org: the spa's staff — pickers for closer and technician (R1-1), profiles and pay terms
//! (staff management). Staff are not user accounts: a technician who never logs in is still
//! staff; a profile may be linked to a login for self-service.

pub mod http;
pub mod profile;
pub mod provisioning;
mod repo;
pub mod service;

pub use service::{ActiveStaff, StaffStatus};
