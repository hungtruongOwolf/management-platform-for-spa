//! Runtime configuration, read from the environment once at startup.
//! Invalid configuration stops the process before it accepts any traffic.

use secrecy::SecretString;
use std::net::SocketAddr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Development,
    Staging,
    Production,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub environment: Environment,
    /// Runtime connection as `app_rw` (row-level security applies). Never the schema owner.
    pub app_database_url: SecretString,
    /// The only browser origin allowed to send state-changing requests (CSRF defence).
    pub public_origin: String,
    /// AES-256 key (base64) for field encryption of health notes. Never stored in the database.
    pub data_key: SecretString,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{0} is required")]
    Missing(&'static str),
    #[error("{name} is invalid: {value:?}")]
    Invalid { name: &'static str, value: String },
}

/// Lets the local prod-like smoke test (`infra/prodtest`, a TLS-less stand-in Postgres) run with
/// `APP_ENV=staging`. Honoured only in staging, never in production.
pub const INSECURE_DB_TLS_FLAG: &str = "APP_DB_INSECURE_TLS_FOR_LOCAL_TEST";

/// Whether a libpq URL makes the client verify the server certificate (security review L6):
/// `verify-full` (CA + host name) or `verify-ca` (CA only; acceptable when the CA is private to
/// the provider, as Supabase's is). `require`/`prefer` encrypt but accept any certificate.
pub fn verifies_server_certificate(url: &str) -> bool {
    let Some((_, query)) = url.split_once('?') else {
        return false;
    };
    let modes: Vec<&str> = query
        .split('&')
        .filter_map(|pair| pair.strip_prefix("sslmode="))
        .collect();
    // A repeated parameter is ambiguous across drivers: refuse rather than guess which wins.
    matches!(modes.as_slice(), [mode] if *mode == "verify-full" || *mode == "verify-ca")
}

impl Config {
    /// Reads configuration from process environment variables.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Reads configuration through `lookup`, so tests can supply values without touching the process env.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let required = |name: &'static str| {
            lookup(name)
                .filter(|v| !v.trim().is_empty())
                .ok_or(ConfigError::Missing(name))
        };

        let raw_addr = required("APP_BIND_ADDR")?;
        let bind_addr = raw_addr.parse().map_err(|_| ConfigError::Invalid {
            name: "APP_BIND_ADDR",
            value: raw_addr.clone(),
        })?;

        let raw_env = lookup("APP_ENV").unwrap_or_else(|| "development".to_owned());
        let environment = match raw_env.as_str() {
            "development" => Environment::Development,
            "staging" => Environment::Staging,
            "production" => Environment::Production,
            _ => {
                return Err(ConfigError::Invalid {
                    name: "APP_ENV",
                    value: raw_env,
                });
            }
        };

        let app_database_url = required("APP_DATABASE_URL")?;
        if !app_database_url.starts_with("postgres://")
            && !app_database_url.starts_with("postgresql://")
        {
            return Err(ConfigError::Invalid {
                name: "APP_DATABASE_URL",
                value: "[redacted]".to_owned(),
            });
        }

        if environment != Environment::Development {
            let exempt = environment == Environment::Staging
                && lookup(INSECURE_DB_TLS_FLAG).as_deref() == Some("true");
            if !exempt && !verifies_server_certificate(&app_database_url) {
                return Err(ConfigError::Invalid {
                    name: "APP_DATABASE_URL",
                    value:
                        "[redacted] (outside development, sslmode must be verify-full or verify-ca)"
                            .to_owned(),
                });
            }
        }

        let public_origin = required("APP_PUBLIC_ORIGIN")?;
        let origin_ok = (public_origin.starts_with("https://")
            || (environment == Environment::Development && public_origin.starts_with("http://")))
            && !public_origin.ends_with('/');
        if !origin_ok {
            return Err(ConfigError::Invalid {
                name: "APP_PUBLIC_ORIGIN",
                value: public_origin,
            });
        }

        let data_key = required("APP_DATA_KEY")?;
        if kernel::crypto::FieldCipher::from_base64(&data_key).is_err() {
            return Err(ConfigError::Invalid {
                name: "APP_DATA_KEY",
                value: "[redacted]".to_owned(),
            });
        }

        Ok(Self {
            bind_addr,
            environment,
            app_database_url: SecretString::from(app_database_url),
            public_origin,
            data_key: SecretString::from(data_key),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const DB: &str = "postgres://app_rw:pw@127.0.0.1:5432/vpyd";
    const TEST_KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn valid() -> Vec<(&'static str, &'static str)> {
        vec![
            ("APP_BIND_ADDR", "127.0.0.1:8080"),
            ("APP_DATABASE_URL", DB),
            ("APP_PUBLIC_ORIGIN", "http://localhost:5173"),
            ("APP_DATA_KEY", TEST_KEY),
        ]
    }

    #[test]
    fn parses_valid_config_and_defaults_to_development() {
        let config = Config::from_lookup(lookup(&valid())).map_err(|e| e.to_string());
        assert_eq!(
            config.as_ref().map(|c| c.environment),
            Ok(Environment::Development)
        );
        assert_eq!(config.map(|c| c.bind_addr.port()), Ok(8080));
    }

    #[test]
    fn missing_required_values_fail_fast() {
        assert_eq!(
            Config::from_lookup(lookup(&[])).err(),
            Some(ConfigError::Missing("APP_BIND_ADDR"))
        );
        let without_db: Vec<_> = valid()
            .into_iter()
            .filter(|(k, _)| *k != "APP_DATABASE_URL")
            .collect();
        assert_eq!(
            Config::from_lookup(lookup(&without_db)).err(),
            Some(ConfigError::Missing("APP_DATABASE_URL"))
        );
    }

    #[test]
    fn invalid_values_are_rejected() {
        let mut bad_addr = valid();
        bad_addr[0] = ("APP_BIND_ADDR", "not-an-addr");
        assert!(matches!(
            Config::from_lookup(lookup(&bad_addr)).err(),
            Some(ConfigError::Invalid {
                name: "APP_BIND_ADDR",
                ..
            })
        ));

        let mut bad_env = valid();
        bad_env.push(("APP_ENV", "prod"));
        assert!(matches!(
            Config::from_lookup(lookup(&bad_env)).err(),
            Some(ConfigError::Invalid {
                name: "APP_ENV",
                ..
            })
        ));
    }

    #[test]
    fn data_key_must_be_a_32_byte_base64_key() {
        let mut bad = valid();
        bad.retain(|(k, _)| *k != "APP_DATA_KEY");
        assert_eq!(
            Config::from_lookup(lookup(&bad)).err(),
            Some(ConfigError::Missing("APP_DATA_KEY"))
        );
        bad.push(("APP_DATA_KEY", "c2hvcnQ="));
        assert!(matches!(
            Config::from_lookup(lookup(&bad)).err(),
            Some(ConfigError::Invalid {
                name: "APP_DATA_KEY",
                ..
            })
        ));
    }

    const TLS_DB: &str = "postgres://app_rw:pw@db.example:5432/vpyd?sslmode=verify-full&sslrootcert=/run/secrets/ca.crt";

    fn deployed(env: &'static str) -> Vec<(&'static str, &'static str)> {
        let mut pairs = valid();
        pairs.retain(|(k, _)| !matches!(*k, "APP_DATABASE_URL" | "APP_PUBLIC_ORIGIN"));
        pairs.extend([
            ("APP_ENV", env),
            ("APP_DATABASE_URL", TLS_DB),
            ("APP_PUBLIC_ORIGIN", "https://staging.example"),
        ]);
        pairs
    }

    fn with_db(
        mut pairs: Vec<(&'static str, &'static str)>,
        url: &'static str,
    ) -> Vec<(&'static str, &'static str)> {
        pairs.retain(|(k, _)| *k != "APP_DATABASE_URL");
        pairs.push(("APP_DATABASE_URL", url));
        pairs
    }

    fn rejects_db(pairs: Vec<(&'static str, &'static str)>) -> bool {
        matches!(
            Config::from_lookup(lookup(&pairs)).err(),
            Some(ConfigError::Invalid {
                name: "APP_DATABASE_URL",
                ..
            })
        )
    }

    #[test]
    fn verified_tls_is_recognised_in_the_url() {
        assert!(verifies_server_certificate(TLS_DB));
        assert!(verifies_server_certificate(
            "postgres://h/db?sslmode=verify-ca"
        ));
        assert!(!verifies_server_certificate("postgres://h/db"));
        assert!(!verifies_server_certificate(
            "postgres://h/db?sslmode=require"
        ));
        assert!(!verifies_server_certificate(
            "postgres://h/db?sslmode=prefer"
        ));
        assert!(!verifies_server_certificate(
            "postgres://h/db?xsslmode=verify-full"
        ));
        assert!(!verifies_server_certificate(
            "postgres://h/db?sslmode=verify-full&sslmode=disable"
        ));
    }

    #[test]
    fn deployed_environments_require_verified_database_tls() {
        for env in ["staging", "production"] {
            assert!(Config::from_lookup(lookup(&deployed(env))).is_ok(), "{env}");
            assert!(rejects_db(with_db(deployed(env), DB)), "{env}: no sslmode");
            assert!(
                rejects_db(with_db(deployed(env), "postgres://h/db?sslmode=require")),
                "{env}: unverified TLS"
            );
        }
        // Development (local Postgres, CI, E2E) keeps working without TLS.
        assert!(Config::from_lookup(lookup(&valid())).is_ok());
    }

    #[test]
    fn the_local_smoke_test_exemption_never_applies_to_production() {
        let mut staging = with_db(deployed("staging"), DB);
        staging.push((INSECURE_DB_TLS_FLAG, "true"));
        assert!(Config::from_lookup(lookup(&staging)).is_ok());
        let mut prod = with_db(deployed("production"), DB);
        prod.push((INSECURE_DB_TLS_FLAG, "true"));
        assert!(rejects_db(prod));
    }

    #[test]
    fn production_requires_https_origin() {
        let mut prod = with_db(valid(), TLS_DB);
        prod.push(("APP_ENV", "production"));
        assert!(matches!(
            Config::from_lookup(lookup(&prod)).err(),
            Some(ConfigError::Invalid {
                name: "APP_PUBLIC_ORIGIN",
                ..
            })
        ));
    }

    #[test]
    fn debug_output_never_contains_the_database_url() {
        let config = Config::from_lookup(lookup(&valid()));
        let debug = config.map(|c| format!("{c:?}")).unwrap_or_default();
        assert!(!debug.contains("app_rw:pw"));
        assert!(debug.to_lowercase().contains("redacted"), "{debug}");
    }
}
