//! Sales: sales, sale lines and append-only payments (spec R1-1 US3; one-off service lines in S7).

pub mod domain;
pub mod dto;
pub mod http;
mod repo;
pub mod service;

pub use domain::PaymentMethod;
