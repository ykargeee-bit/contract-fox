//! Webhook delivery for the `donation.confirmed` event.
//!
//! Sends an HTTP POST to the configured `WEBHOOK_URL` when a donation is
//! confirmed on-chain.  Delivery is retried up to 3 times on failure with a
//! short delay between attempts.

use std::time::Duration;

use reqwest::Client;
use serde::Serialize;
use thiserror::Error;

/// Payload sent in the webhook POST body.
#[derive(Debug, Serialize)]
pub struct DonationConfirmedPayload {
    pub event: &'static str,
    pub tx_hash: String,
    pub campaign_id: String,
    pub donor_address: String,
    pub amount: u64,
    pub timestamp: String,
}

impl DonationConfirmedPayload {
    pub fn new(
        tx_hash: impl Into<String>,
        campaign_id: impl Into<String>,
        donor_address: impl Into<String>,
        amount: u64,
        timestamp: impl Into<String>,
    ) -> Self {
        Self {
            event: "donation.confirmed",
            tx_hash: tx_hash.into(),
            campaign_id: campaign_id.into(),
            donor_address: donor_address.into(),
            amount,
            timestamp: timestamp.into(),
        }
    }
}

/// Errors that can occur during webhook delivery.
#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("webhook delivery failed after {attempts} attempt(s): {last_error}")]
    DeliveryFailed {
        attempts: u32,
        last_error: String,
    },
}

const MAX_ATTEMPTS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Deliver a `donation.confirmed` webhook to `url`, retrying up to 3 times.
///
/// Succeeds as soon as the server responds with a 2xx status.
pub async fn deliver(
    client: &Client,
    url: &str,
    payload: &DonationConfirmedPayload,
) -> Result<(), WebhookError> {
    let mut last_error = String::new();

    for attempt in 1..=MAX_ATTEMPTS {
        match client.post(url).json(payload).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => {
                last_error = format!("HTTP {}", resp.status());
            }
            Err(e) => {
                last_error = e.to_string();
            }
        }

        if attempt < MAX_ATTEMPTS {
            tokio::time::sleep(RETRY_DELAY).await;
        }
    }

    Err(WebhookError::DeliveryFailed {
        attempts: MAX_ATTEMPTS,
        last_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_serializes_correctly() {
        let p = DonationConfirmedPayload::new(
            "txhash123",
            "campaign-1",
            "GABC123",
            5000,
            "2026-06-21T17:44:34Z",
        );
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["event"], "donation.confirmed");
        assert_eq!(json["tx_hash"], "txhash123");
        assert_eq!(json["campaign_id"], "campaign-1");
        assert_eq!(json["donor_address"], "GABC123");
        assert_eq!(json["amount"], 5000);
        assert_eq!(json["timestamp"], "2026-06-21T17:44:34Z");
    }

    #[test]
    fn webhook_error_message_includes_attempts_and_reason() {
        let err = WebhookError::DeliveryFailed {
            attempts: 3,
            last_error: "connection refused".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains('3'));
        assert!(msg.contains("connection refused"));
    }
}
