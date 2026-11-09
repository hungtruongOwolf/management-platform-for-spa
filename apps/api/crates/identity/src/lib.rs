//! Identity: accounts, opaque sessions, roles-as-data and the `AuthUser` extractor (ADR-0004).

pub mod account;
pub mod http;
pub mod lockout;
pub mod mfa;
pub mod password;
pub mod provisioning;
mod repo;
pub mod roles;
pub mod service;
pub mod token;

pub use http::{AuthUser, CurrentSession, PendingMfaSession};
pub use repo::{LiveSession, MemberAccess};
pub use roles::SystemRole;
