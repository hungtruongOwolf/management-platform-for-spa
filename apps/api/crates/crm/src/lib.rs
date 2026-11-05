//! CRM: customer profiles, accent-insensitive search, encrypted health notes (spec R1-1 US2).

pub mod cipher;
pub mod follow_up;
pub mod http;
mod repo;
pub mod service;
pub mod text;

pub use cipher::FieldCipher;
