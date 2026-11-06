//! Appointments (spec R1-3 `appointments.md`): booked → arrived → done (= a visit), or no-show /
//! cancelled. Walk-ins skip this and use `POST /visits` directly.

pub mod domain;
pub mod dto;
pub mod http;
pub(crate) mod repo;
pub mod service;
