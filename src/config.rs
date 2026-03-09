use std::env;

use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Config {
    pub stellar_network: String,
    pub stellar_platform_secret: String,
    pub horizon_url: String,
    pub soroban_rpc_url: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingEnvVar(&'static str),

    #[error("environment variable {name} cannot be empty")]
    EmptyEnvVar { name: &'static str },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        Ok(Self {
            stellar_network: read_required_env("STELLAR_NETWORK")?,
            stellar_platform_secret: read_required_env("STELLAR_PLATFORM_SECRET")?,
            horizon_url: read_required_env("HORIZON_URL")?,
            soroban_rpc_url: read_required_env("SOROBAN_RPC_URL")?,
        })
    }
}

fn read_required_env(name: &'static str) -> Result<String, ConfigError> {
    let value = env::var(name).map_err(|_| ConfigError::MissingEnvVar(name))?;

    if value.trim().is_empty() {
        return Err(ConfigError::EmptyEnvVar { name });
    }

    Ok(value)
}
