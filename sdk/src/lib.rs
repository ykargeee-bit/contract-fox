use thiserror::Error;

/// Network endpoints used by the SDK transaction builder.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub horizon_url: String,
    pub soroban_rpc_url: String,
}

/// Errors returned by the SDK.
#[derive(Debug, Error)]
pub enum StellarAidError {
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Soroban RPC error (code {code}): {message}")]
    SorobanError { code: i64, message: String },
    #[error("Transaction failed: {0}")]
    TransactionFailed(String),
}

/// Contract Fox SDK - Common utilities for smart contracts
pub mod types;
pub mod utils;

pub use types::*;
pub use utils::*;

pub mod bindings;
pub mod transaction_builder;
