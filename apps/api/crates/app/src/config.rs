//! Runtime configuration, read from the environment once at startup.
//! Invalid configuration stops the process before it accepts any traffic.

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
        let raw_addr = lookup("APP_BIND_ADDR").ok_or(ConfigError::Missing("APP_BIND_ADDR"))?;
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

        Ok(Self {
            bind_addr,
            environment,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn parses_valid_config_and_defaults_to_development() {
        let config = Config::from_lookup(lookup(&[("APP_BIND_ADDR", "127.0.0.1:8080")]));
        let config = config.map_err(|e| e.to_string());
        assert_eq!(
            config.as_ref().map(|c| c.environment),
            Ok(Environment::Development)
        );
        assert_eq!(config.map(|c| c.bind_addr.port()), Ok(8080),);
    }

    #[test]
    fn missing_bind_addr_fails_fast() {
        let err = Config::from_lookup(lookup(&[])).err();
        assert_eq!(err, Some(ConfigError::Missing("APP_BIND_ADDR")));
    }

    #[test]
    fn invalid_values_are_rejected() {
        let bad_addr = Config::from_lookup(lookup(&[("APP_BIND_ADDR", "not-an-addr")])).err();
        assert!(matches!(
            bad_addr,
            Some(ConfigError::Invalid {
                name: "APP_BIND_ADDR",
                ..
            })
        ));

        let bad_env = Config::from_lookup(lookup(&[
            ("APP_BIND_ADDR", "127.0.0.1:8080"),
            ("APP_ENV", "prod"),
        ]))
        .err();
        assert!(matches!(
            bad_env,
            Some(ConfigError::Invalid {
                name: "APP_ENV",
                ..
            })
        ));
    }
}
