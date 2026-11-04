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
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{0} is required")]
    Missing(&'static str),
    #[error("{name} is invalid: {value:?}")]
    Invalid { name: &'static str, value: String },
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

        Ok(Self {
            bind_addr,
            environment,
            app_database_url: SecretString::from(app_database_url),
            public_origin,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const DB: &str = "postgres://app_rw:pw@127.0.0.1:5432/vpyd";

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
    fn production_requires_https_origin() {
        let mut prod = valid();
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
