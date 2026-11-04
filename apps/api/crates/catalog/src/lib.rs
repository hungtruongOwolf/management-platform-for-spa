//! Catalog: service categories, services with price variants, package plans (spec R1-1 US1).

pub mod commands;
pub mod domain;
pub mod http;
mod repo;
pub mod seed;
pub mod service;

pub use domain::{Gender, PriceLookupError, PriceVariant, ServicePrices};
