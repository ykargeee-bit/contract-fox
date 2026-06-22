use thiserror::Error;

/// All states a donation can occupy during its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DonationStatus {
    /// Created locally; not yet broadcast to the network.
    Pending,
    /// Transaction submitted to the Stellar network.
    Submitted,
    /// Transaction seen on-chain; waiting for sufficient ledger closings.
    Confirming,
    /// Sufficient confirmations received; donation is finalised.
    Confirmed,
    /// Transaction failed or was rejected by the network.
    Failed,
    /// Funds returned to the donor after a failure.
    Refunded,
}

/// Events that drive the donation state machine forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DonationEvent {
    /// Donor authorises and broadcasts the transaction.
    Submit,
    /// Network acknowledges the transaction; confirmation begins.
    BeginConfirmation,
    /// Enough ledger closings have passed to consider the donation confirmed.
    Confirm,
    /// The transaction was rejected or timed out.
    Fail,
    /// The funds have been returned to the donor.
    Refund,
}

/// Returned when a state transition is not permitted.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransitionError {
    #[error("invalid transition: cannot apply {event:?} while in state {from:?}")]
    InvalidTransition {
        from: DonationStatus,
        event: DonationEvent,
    },
}

impl DonationStatus {
    /// Attempt to advance the state machine by applying `event`.
    ///
    /// Returns the new [`DonationStatus`] on success, or a
    /// [`TransitionError`] if the transition is not permitted from the
    /// current state.
    ///
    /// ## Valid transitions
    ///
    /// ```text
    /// Pending     --[Submit]-------------> Submitted
    /// Submitted   --[BeginConfirmation]--> Confirming
    /// Submitted   --[Fail]--------------> Failed
    /// Confirming  --[Confirm]-----------> Confirmed
    /// Confirming  --[Fail]--------------> Failed
    /// Failed      --[Refund]------------> Refunded
    /// ```
    pub fn transition(self, event: DonationEvent) -> Result<DonationStatus, TransitionError> {
        match (&self, &event) {
            (DonationStatus::Pending, DonationEvent::Submit) => Ok(DonationStatus::Submitted),
            (DonationStatus::Submitted, DonationEvent::BeginConfirmation) => {
                Ok(DonationStatus::Confirming)
            }
            (DonationStatus::Submitted, DonationEvent::Fail) => Ok(DonationStatus::Failed),
            (DonationStatus::Confirming, DonationEvent::Confirm) => Ok(DonationStatus::Confirmed),
            (DonationStatus::Confirming, DonationEvent::Fail) => Ok(DonationStatus::Failed),
            (DonationStatus::Failed, DonationEvent::Refund) => Ok(DonationStatus::Refunded),
            _ => Err(TransitionError::InvalidTransition { from: self, event }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Valid transitions ─────────────────────────────────────────────────

    #[test]
    fn pending_submit_yields_submitted() {
        let next = DonationStatus::Pending
            .transition(DonationEvent::Submit)
            .unwrap();
        assert_eq!(next, DonationStatus::Submitted);
    }

    #[test]
    fn submitted_begin_confirmation_yields_confirming() {
        let next = DonationStatus::Submitted
            .transition(DonationEvent::BeginConfirmation)
            .unwrap();
        assert_eq!(next, DonationStatus::Confirming);
    }

    #[test]
    fn confirming_confirm_yields_confirmed() {
        let next = DonationStatus::Confirming
            .transition(DonationEvent::Confirm)
            .unwrap();
        assert_eq!(next, DonationStatus::Confirmed);
    }

    #[test]
    fn submitted_fail_yields_failed() {
        let next = DonationStatus::Submitted
            .transition(DonationEvent::Fail)
            .unwrap();
        assert_eq!(next, DonationStatus::Failed);
    }

    #[test]
    fn confirming_fail_yields_failed() {
        let next = DonationStatus::Confirming
            .transition(DonationEvent::Fail)
            .unwrap();
        assert_eq!(next, DonationStatus::Failed);
    }

    #[test]
    fn failed_refund_yields_refunded() {
        let next = DonationStatus::Failed
            .transition(DonationEvent::Refund)
            .unwrap();
        assert_eq!(next, DonationStatus::Refunded);
    }

    // ── Invalid transitions ───────────────────────────────────────────────

    #[test]
    fn confirmed_cannot_go_to_pending() {
        let err = DonationStatus::Confirmed
            .transition(DonationEvent::Submit)
            .unwrap_err();
        assert_eq!(
            err,
            TransitionError::InvalidTransition {
                from: DonationStatus::Confirmed,
                event: DonationEvent::Submit,
            }
        );
    }

    #[test]
    fn refunded_cannot_transition_further() {
        let err = DonationStatus::Refunded
            .transition(DonationEvent::Submit)
            .unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn pending_cannot_confirm_directly() {
        let err = DonationStatus::Pending
            .transition(DonationEvent::Confirm)
            .unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn confirmed_cannot_be_refunded() {
        let err = DonationStatus::Confirmed
            .transition(DonationEvent::Refund)
            .unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn pending_cannot_fail() {
        let err = DonationStatus::Pending
            .transition(DonationEvent::Fail)
            .unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn error_message_contains_state_and_event() {
        let err = DonationStatus::Confirmed
            .transition(DonationEvent::Submit)
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Confirmed"));
        assert!(msg.contains("Submit"));
    }

    // ── Full lifecycle happy path ─────────────────────────────────────────

    #[test]
    fn full_happy_path_pending_to_confirmed() {
        let status = DonationStatus::Pending
            .transition(DonationEvent::Submit)
            .unwrap()
            .transition(DonationEvent::BeginConfirmation)
            .unwrap()
            .transition(DonationEvent::Confirm)
            .unwrap();
        assert_eq!(status, DonationStatus::Confirmed);
    }

    #[test]
    fn full_failure_path_pending_to_refunded() {
        let status = DonationStatus::Pending
            .transition(DonationEvent::Submit)
            .unwrap()
            .transition(DonationEvent::Fail)
            .unwrap()
            .transition(DonationEvent::Refund)
            .unwrap();
        assert_eq!(status, DonationStatus::Refunded);
    }
}
