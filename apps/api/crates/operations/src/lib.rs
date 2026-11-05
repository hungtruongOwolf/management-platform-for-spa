//! Operations: visits recorded at the front desk (spec R1-1 US4). A visit consumes package
//! sessions (`packages`) and/or sells one-off services (`sales`), all inside one `TenantTx`.

pub mod appointments;
pub mod domain;
pub mod dto;
pub mod http;
mod repo;
pub mod service;
