//! Lucky draw ("Vòng quay may mắn", spec product/specs/lucky-draw.md): the owner's draws, their
//! participants and prizes, and spins drawn on the server (ADR-0025).

pub mod domain;
pub mod http;
mod repo;
pub mod service;

pub use domain::{OsRandom, RandomSource, RandomUnavailable};
pub use repo::{DrawRow, ParticipantRow, PrizeRow, ResultRow};
