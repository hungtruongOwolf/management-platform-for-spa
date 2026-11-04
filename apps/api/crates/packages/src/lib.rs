//! Packages: the treatment packages a customer owns and their session counters (spec R1-1 US3, US4, US7, US8).
//! Other contexts use [`service::create`] and [`service::consume_session`] inside their own `TenantTx`.

pub mod domain;
pub mod http;
mod repo;
pub mod service;

pub use domain::PackageStatus;
pub use service::{Consume, Consumed, CustomerPackage, NewItem, NewPackage, Origin};
