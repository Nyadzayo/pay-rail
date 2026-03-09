use payrail_core::{Currency, EventType, Money, PaymentState};

use super::types::{StartbuttonEventType, StartbuttonStatus};
use crate::AdapterError;

// ---------------------------------------------------------------------------
// State mapping
// ---------------------------------------------------------------------------

/// Maps a Startbutton event type + status to canonical (state_before, state_after).
///
/// Unlike Peach which uses result codes for precedence, Startbutton uses explicit
/// status strings that directly map to canonical states. The event type provides
/// context for inferring state_before.
pub fn startbutton_event_to_canonical_state(
    event_type: &str,
    status: &str,
) -> Result<(PaymentState, PaymentState), AdapterError> {
    let event = StartbuttonEventType::parse(event_type);
    let sb_status = StartbuttonStatus::parse(status);

    // Status takes precedence for determining state_after
    let state_after = match sb_status {
        StartbuttonStatus::Authorized => PaymentState::Authorized,
        StartbuttonStatus::Captured => PaymentState::Captured,
        StartbuttonStatus::Refunded => PaymentState::Refunded,
        StartbuttonStatus::Voided => PaymentState::Voided,
        StartbuttonStatus::Failed | StartbuttonStatus::Declined | StartbuttonStatus::Reversed => {
            PaymentState::Failed
        }
        StartbuttonStatus::Pending => PaymentState::Created,
        StartbuttonStatus::Expired => PaymentState::TimedOut,
        // VERIFY: AWAITING_3DS mapping (confidence: 0.78, source: community_report, check: test against sandbox)
        StartbuttonStatus::Awaiting3ds => PaymentState::Pending3ds,
        StartbuttonStatus::Unknown(ref s) => {
            return Err(AdapterError::WebhookTranslationFailed {
                provider: "startbutton".to_owned(),
                reason: format!("unknown Startbutton status: {s}"),
            });
        }
    };

    // Event type determines state_before
    let state_before = match event {
        StartbuttonEventType::PaymentAuthorized => {
            // VERIFY: 3DS completion leading to authorization (confidence: 0.72, source: community_report, check: sandbox)
            // If status is AUTHORIZED from a 3ds_completed event context, state_before would be Pending3ds
            // But for a direct payment.authorized event, state_before is Created
            PaymentState::Created
        }
        StartbuttonEventType::PaymentCaptured => PaymentState::Authorized,
        StartbuttonEventType::PaymentRefunded => PaymentState::Captured,
        StartbuttonEventType::PaymentVoided => PaymentState::Authorized,
        StartbuttonEventType::PaymentFailed => {
            // Failure can happen from multiple states; infer from status context
            match sb_status {
                StartbuttonStatus::Declined => PaymentState::Created,
                _ => PaymentState::Created, // Default: failure during authorization
            }
        }
        StartbuttonEventType::Payment3dsCompleted => {
            // 3DS completion: can result in AUTHORIZED (success) or FAILED
            PaymentState::Pending3ds
        }
        // VERIFY: expired event state_before (confidence: 0.75, source: inferred, check: sandbox)
        StartbuttonEventType::PaymentExpired => PaymentState::Authorized,
        StartbuttonEventType::PaymentReversed => {
            // Reversal typically happens after capture (undocumented)
            PaymentState::Captured
        }
        StartbuttonEventType::Unknown(ref t) => {
            return Err(AdapterError::WebhookTranslationFailed {
                provider: "startbutton".to_owned(),
                reason: format!("unknown Startbutton event type: {t}"),
            });
        }
    };

    Ok((state_before, state_after))
}

// ---------------------------------------------------------------------------
// Event type mapping
// ---------------------------------------------------------------------------

/// Maps a Startbutton event type to a canonical EventType (domain.entity.action).
pub fn startbutton_event_to_event_type(event_type: &str) -> Result<EventType, AdapterError> {
    let canonical = match StartbuttonEventType::parse(event_type) {
        StartbuttonEventType::PaymentAuthorized => "payment.charge.authorized",
        StartbuttonEventType::PaymentCaptured => "payment.capture.succeeded",
        StartbuttonEventType::PaymentRefunded => "payment.refund.succeeded",
        StartbuttonEventType::PaymentVoided => "payment.void.succeeded",
        StartbuttonEventType::PaymentFailed => "payment.charge.failed",
        StartbuttonEventType::Payment3dsCompleted => "payment.charge.three_ds_completed",
        StartbuttonEventType::PaymentExpired => "payment.charge.expired",
        StartbuttonEventType::PaymentReversed => "payment.charge.reversed",
        StartbuttonEventType::Unknown(ref t) => {
            return Err(AdapterError::WebhookTranslationFailed {
                provider: "startbutton".to_owned(),
                reason: format!("unknown Startbutton event type: {t}"),
            });
        }
    };

    EventType::new(canonical).map_err(|e| AdapterError::WebhookTranslationFailed {
        provider: "startbutton".to_owned(),
        reason: format!("invalid canonical event type: {e}"),
    })
}

// ---------------------------------------------------------------------------
// Amount parsing
// ---------------------------------------------------------------------------

/// Converts a Startbutton integer amount + currency to canonical Money.
///
/// Startbutton amounts are already in integer cents — no decimal parsing needed.
pub fn startbutton_amount_to_money(amount: i64, currency: &str) -> Result<Money, AdapterError> {
    if amount < 0 {
        return Err(AdapterError::WebhookTranslationFailed {
            provider: "startbutton".to_owned(),
            reason: "negative amounts are not allowed in webhook payloads".to_owned(),
        });
    }

    let cur = match currency {
        "ZAR" => Currency::ZAR,
        "USD" => Currency::USD,
        "EUR" => Currency::EUR,
        "GBP" => Currency::GBP,
        other => {
            return Err(AdapterError::WebhookTranslationFailed {
                provider: "startbutton".to_owned(),
                reason: format!("unsupported currency: {other}"),
            });
        }
    };

    Ok(Money::new(amount, cur))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- State mapping tests --

    #[test]
    fn authorized_maps_correctly() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.authorized", "AUTHORIZED").unwrap();
        assert_eq!(before, PaymentState::Created);
        assert_eq!(after, PaymentState::Authorized);
    }

    #[test]
    fn captured_maps_correctly() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.captured", "CAPTURED").unwrap();
        assert_eq!(before, PaymentState::Authorized);
        assert_eq!(after, PaymentState::Captured);
    }

    #[test]
    fn refunded_maps_correctly() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.refunded", "REFUNDED").unwrap();
        assert_eq!(before, PaymentState::Captured);
        assert_eq!(after, PaymentState::Refunded);
    }

    #[test]
    fn voided_maps_correctly() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.voided", "VOIDED").unwrap();
        assert_eq!(before, PaymentState::Authorized);
        assert_eq!(after, PaymentState::Voided);
    }

    #[test]
    fn failed_maps_correctly() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.failed", "FAILED").unwrap();
        assert_eq!(before, PaymentState::Created);
        assert_eq!(after, PaymentState::Failed);
    }

    #[test]
    fn declined_maps_to_failed() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.failed", "DECLINED").unwrap();
        assert_eq!(before, PaymentState::Created);
        assert_eq!(after, PaymentState::Failed);
    }

    #[test]
    fn reversed_maps_to_failed() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.reversed", "REVERSED").unwrap();
        assert_eq!(before, PaymentState::Captured);
        assert_eq!(after, PaymentState::Failed);
    }

    #[test]
    fn expired_maps_correctly() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.expired", "EXPIRED").unwrap();
        assert_eq!(before, PaymentState::Authorized);
        assert_eq!(after, PaymentState::TimedOut);
    }

    #[test]
    fn awaiting_3ds_maps_to_pending3ds() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.3ds_completed", "AWAITING_3DS").unwrap();
        assert_eq!(before, PaymentState::Pending3ds);
        assert_eq!(after, PaymentState::Pending3ds);
    }

    #[test]
    fn three_ds_completed_authorized_maps_correctly() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.3ds_completed", "AUTHORIZED").unwrap();
        assert_eq!(before, PaymentState::Pending3ds);
        assert_eq!(after, PaymentState::Authorized);
    }

    #[test]
    fn three_ds_completed_failed_maps_correctly() {
        let (before, after) =
            startbutton_event_to_canonical_state("payment.3ds_completed", "FAILED").unwrap();
        assert_eq!(before, PaymentState::Pending3ds);
        assert_eq!(after, PaymentState::Failed);
    }

    #[test]
    fn unknown_event_type_returns_error() {
        let result = startbutton_event_to_canonical_state("unknown.event", "AUTHORIZED");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AdapterError::WebhookTranslationFailed { .. }
        ));
    }

    #[test]
    fn unknown_status_returns_error() {
        let result = startbutton_event_to_canonical_state("payment.authorized", "MYSTERY");
        assert!(result.is_err());
    }

    // -- Event type mapping tests --

    #[test]
    fn event_type_authorized() {
        let et = startbutton_event_to_event_type("payment.authorized").unwrap();
        assert_eq!(et.as_str(), "payment.charge.authorized");
    }

    #[test]
    fn event_type_captured() {
        let et = startbutton_event_to_event_type("payment.captured").unwrap();
        assert_eq!(et.as_str(), "payment.capture.succeeded");
    }

    #[test]
    fn event_type_refunded() {
        let et = startbutton_event_to_event_type("payment.refunded").unwrap();
        assert_eq!(et.as_str(), "payment.refund.succeeded");
    }

    #[test]
    fn event_type_voided() {
        let et = startbutton_event_to_event_type("payment.voided").unwrap();
        assert_eq!(et.as_str(), "payment.void.succeeded");
    }

    #[test]
    fn event_type_failed() {
        let et = startbutton_event_to_event_type("payment.failed").unwrap();
        assert_eq!(et.as_str(), "payment.charge.failed");
    }

    #[test]
    fn event_type_3ds_completed() {
        let et = startbutton_event_to_event_type("payment.3ds_completed").unwrap();
        assert_eq!(et.as_str(), "payment.charge.three_ds_completed");
    }

    #[test]
    fn event_type_expired() {
        let et = startbutton_event_to_event_type("payment.expired").unwrap();
        assert_eq!(et.as_str(), "payment.charge.expired");
    }

    #[test]
    fn event_type_reversed() {
        let et = startbutton_event_to_event_type("payment.reversed").unwrap();
        assert_eq!(et.as_str(), "payment.charge.reversed");
    }

    #[test]
    fn event_type_unknown_returns_error() {
        let result = startbutton_event_to_event_type("unknown.event");
        assert!(result.is_err());
    }

    // -- Amount tests --

    #[test]
    fn amount_integer_cents() {
        let money = startbutton_amount_to_money(10050, "ZAR").unwrap();
        assert_eq!(money.value, 10050);
        assert_eq!(money.currency, Currency::ZAR);
    }

    #[test]
    fn amount_zero() {
        let money = startbutton_amount_to_money(0, "ZAR").unwrap();
        assert_eq!(money.value, 0);
    }

    #[test]
    fn amount_negative_rejected() {
        let result = startbutton_amount_to_money(-100, "ZAR");
        assert!(result.is_err());
    }

    #[test]
    fn amount_unsupported_currency() {
        let result = startbutton_amount_to_money(100, "NGN");
        assert!(result.is_err());
    }
}
