// Contract Fox Worker - Background service for contract management
mod logging;
mod redaction;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub rpc_url: String,
    pub poll_interval: u64,
    pub network: String,
}

impl WorkerConfig {
    pub fn log_safe(&self) -> serde_json::Value {
        redaction::log_safe_config(&serde_json::json!({
            "rpc_url": self.rpc_url,
            "poll_interval": self.poll_interval,
            "network": self.network
        }))
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://soroban-testnet.stellar.org/".to_string(),
            poll_interval: 30,
            network: "testnet".to_string(),
        }
    }
}

pub async fn run_worker(config: WorkerConfig) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting contract fox worker");
    debug!(
        "Worker configuration: {}",
        serde_json::to_string_pretty(&config.log_safe())
            .unwrap_or_else(|_| "Failed to serialize config".to_string())
    );

    loop {
        debug!("Polling for contract updates");

        match tokio::time::timeout(
            tokio::time::Duration::from_secs(config.poll_interval),
            check_contracts(&config),
        )
        .await
        {
            Ok(result) => {
                if let Err(e) = result {
                    error!("Error checking contracts: {}", e);
                }
            }
            Err(_) => {
                debug!("Polling timeout reached, continuing to next iteration");
            }
        }
    }
}

async fn check_contracts(config: &WorkerConfig) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Checking contracts on network: {}", config.network);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    logging::init_logging()?;

    let config = WorkerConfig::default();
    info!("Worker initialized with default configuration");

    run_worker(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WorkerConfig::default();
        assert_eq!(config.network, "testnet");
        assert_eq!(config.poll_interval, 30);
    }
}
