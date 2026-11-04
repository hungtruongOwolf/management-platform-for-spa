//! Strongly typed identifiers. A `TenantId` can never be passed where a `UserId` is expected,
//! and every repository function takes a `TenantId`, so forgetting tenant scope does not compile.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, sqlx::Type)]
        #[serde(transparent)]
        #[sqlx(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// New time-ordered (UUIDv7) identifier.
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(
    /// A spa (tenant). All business data belongs to exactly one tenant.
    TenantId
);
id_type!(
    /// A physical location of a tenant.
    BranchId
);
id_type!(
    /// A login identity (global; joins tenants through memberships).
    UserId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ids_are_v7_and_time_ordered() {
        let a = TenantId::new();
        let b = TenantId::new();
        assert_eq!(a.as_uuid().get_version_num(), 7);
        assert!(a < b);
    }

    #[test]
    fn serializes_as_plain_uuid_string() {
        let id = UserId::from_uuid(Uuid::nil());
        assert_eq!(
            serde_json::to_string(&id).ok(),
            Some("\"00000000-0000-0000-0000-000000000000\"".to_owned())
        );
    }
}
