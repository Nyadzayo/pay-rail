use payrail_core::{Currency, EventType, Money, PaymentState};

use super::types::{PeachEventType, PeachResultCode};
use crate::AdapterError;

// ---------------------------------------------------------------------------
// State mapping (Task 2.1, 2.2, 2.3)
// ---------------------------------------------------------------------------

/// Maps a Peach event type + result code to canonical (state_before, state_after).
///
/// Result codes take precedence: 800.xxx/900.xxx always mean failure,
/// 000.100.112 always means 3DS redirect, regardless of event type.
pub fn peach_event_to_canonical_state(
    event_type: &str,
    result_code: &str,
) -> Result<(PaymentState, PaymentState), AdapterError> {
    let code = PeachResultCode::new(result_code);
    let event = PeachEventType::parse(event_type);

    // 3DS redirect code overrides event type
    if code.is_3ds_redirect() {
        return Ok((PaymentState::Created, PaymentState::Pending3ds));
    }

    // Result code overrides for failures — state_before depends on event type context
    if code.is_rejected() || code.is_timeout_or_error() {
        let state_before = match event {
            PeachEventType::CaptureSucceeded | PeachEventType::CaptureFailed => {
                PaymentState::Authorized
            }
            PeachEventType::VoidSucceeded => PaymentState::Authorized,
            PeachEventType::RefundSucceeded | PeachEventType::RefundFailed => {
                PaymentState::Captured
            }
            _ => PaymentState::Created,
        };
        return Ok((state_before, PaymentState::Failed));
    }

    match event {
        PeachEventType::ChargeSucceeded => {
            if code.is_3ds_success() {
                // 3DS verification completed -> authorized
                Ok((PaymentState::Pending3ds, PaymentState::Authorized))
            } else {
                // Direct authorization
                Ok((PaymentState::Created, PaymentState::Authorized))
            }
        }
        PeachEventType::ChargeFailed => {
            if code.is_3ds_failure() {
                // 3DS authentication/processing failure
                Ok((PaymentState::Pending3ds, PaymentState::Failed))
            } else {
                Ok((PaymentState::Created, PaymentState::Failed))
            }
        }
        PeachEventType::ChargePending => Ok((PaymentState::Created, PaymentState::Pending3ds)),
        PeachEventType::CaptureSucceeded => Ok((PaymentState::Authorized, PaymentState::Captured)),
        PeachEventType::CaptureFailed => Ok((PaymentState::Authorized, PaymentState::Failed)),
        PeachEventType::VoidSucceeded => Ok((PaymentState::Authorized, PaymentState::Voided)),
        PeachEventType::RefundSucceeded => Ok((PaymentState::Captured, PaymentState::Refunded)),
        PeachEventType::RefundFailed => {
            // Refund failed — payment remains captured
            Ok((PaymentState::Captured, PaymentState::Captured))
        }
        PeachEventType::ThreeDsRedirect => Ok((PaymentState::Created, PaymentState::Pending3ds)),
        PeachEventType::Unknown(ref t) => Err(AdapterError::WebhookTranslationFailed {
            provider: "peach_payments".to_owned(),
            reason: format!("unknown Peach event type: {t}"),
        }),
    }
}

// ---------------------------------------------------------------------------
// Event type mapping (Task 2.1)
// ---------------------------------------------------------------------------

/// Maps a Peach event type to a canonical EventType (domain.entity.action).
pub fn peach_event_to_event_type(event_type: &str) -> Result<EventType, AdapterError> {
    let canonical = match PeachEventType::parse(event_type) {
        PeachEventType::ChargeSucceeded => "payment.charge.authorized",
        PeachEventType::ChargeFailed => "payment.charge.failed",
        PeachEventType::ChargePending => "payment.charge.pending",
        PeachEventType::CaptureSucceeded => "payment.capture.succeeded",
        PeachEventType::CaptureFailed => "payment.capture.failed",
        PeachEventType::VoidSucceeded => "payment.void.succeeded",
        PeachEventType::RefundSucceeded => "payment.refund.succeeded",
        PeachEventType::RefundFailed => "payment.refund.failed",
        PeachEventType::ThreeDsRedirect => "payment.charge.three_ds_redirect",
        PeachEventType::Unknown(ref t) => {
            return Err(AdapterError::WebhookTranslationFailed {
                provider: "peach_payments".to_owned(),
                reason: format!("unknown Peach event type: {t}"),
            });
        }
    };

    EventType::new(canonical).map_err(|e| AdapterError::WebhookTranslationFailed {
        provider: "peach_payments".to_owned(),
        reason: format!("invalid canonical event type: {e}"),
    })
}

// ---------------------------------------------------------------------------
// Amount parsing (Task 2.4)
// ---------------------------------------------------------------------------

/// Parses a Peach amount string + currency code to canonical Money (integer cents).
///
/// Never uses floating point. Parses the decimal string directly.
pub fn peach_amount_to_money(amount: &str, currency: &str) -> Result<Money, AdapterError> {
    let cur = match currency {
        "ZAR" => Currency::ZAR,
        "USD" => Currency::USD,
        "EUR" => Currency::EUR,
        "GBP" => Currency::GBP,
        other => {
            return Err(AdapterError::WebhookTranslationFailed {
                provider: "peach_payments".to_owned(),
                reason: format!("unsupported currency: {other}"),
            });
        }
    };

    let cents = parse_amount_to_cents(amount, cur.minor_unit_digits()).map_err(|e| {
        AdapterError::WebhookTranslationFailed {
            provider: "peach_payments".to_owned(),
            reason: format!("invalid amount '{amount}': {e}"),
        }
    })?;

    Ok(Money::new(cents, cur))
}

/// Converts canonical Money (integer cents) to Peach decimal string.
///
/// Peach API amounts are always non-negative — refunds use `paymentType=RF`,
/// not negative amounts. Uses `unsigned_abs()` to prevent malformed output
/// from negative `i64` values (e.g., `-10050` would produce `"-100.-50"`
/// without this guard).
///
/// Example: Money { value: 10050, currency: ZAR } → "100.50"
pub fn money_to_peach_amount(money: &Money) -> String {
    let value = money.value.unsigned_abs();
    let digits = money.currency.minor_unit_digits() as u32;
    let divisor = 10u64.pow(digits);
    let whole = value / divisor;
    let frac = value % divisor;
    format!("{whole}.{frac:0>width$}", width = digits as usize)
}

/// Parses a decimal amount string to integer cents without floating point.
///
/// Rejects negative amounts — webhook amounts must always be positive.
/// The event type (charge vs refund) determines the financial direction.
fn parse_amount_to_cents(amount: &str, decimal_places: u8) -> Result<i64, String> {
    if amount.starts_with('-') {
        return Err("negative amounts are not allowed in webhook payloads".to_owned());
    }

    let parts: Vec<&str> = amount.split('.').collect();

    match parts.len() {
        1 => {
            // Whole number: "100" -> 10000
            let whole: i64 = parts[0]
                .parse()
                .map_err(|e| format!("invalid number: {e}"))?;
            let multiplier = 10i64.pow(decimal_places as u32);
            Ok(whole * multiplier)
        }
        2 => {
            let whole: i64 = parts[0]
                .parse()
                .map_err(|e| format!("invalid whole part: {e}"))?;
            let frac_str = parts[1];

            if frac_str.len() > decimal_places as usize {
                return Err(format!(
                    "too many decimal places: expected at most {decimal_places}, got {}",
                    frac_str.len()
                ));
            }

            // Pad right with zeros if needed: "5" -> "50" for 2 decimal places
            let padded = format!("{frac_str:0<width$}", width = decimal_places as usize);
            let frac: i64 = padded
                .parse()
                .map_err(|e| format!("invalid fractional part: {e}"))?;

            let multiplier = 10i64.pow(decimal_places as u32);
            Ok(whole * multiplier + frac)
        }
        _ => Err("multiple decimal points in amount".to_owned()),
    }
}

// ---------------------------------------------------------------------------
// Tests (Task 7.1)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- State mapping tests --

    #[test]
    fn charge_succeeded_maps_to_authorized() {
        let (before, after) =
            peach_event_to_canonical_state("charge.succeeded", "000.000.000").unwrap();
        assert_eq!(before, PaymentState::Created);
        assert_eq!(after, PaymentState::Authorized);
    }

    #[test]
    fn charge_failed_maps_to_failed() {
        let (before, after) =
            peach_event_to_canonical_state("charge.failed", "800.100.100").unwrap();
        assert_eq!(before, PaymentState::Created);
        assert_eq!(after, PaymentState::Failed);
    }

    #[test]
    fn charge_pending_maps_to_pending3ds() {
        let (before, after) =
            peach_event_to_canonical_state("charge.pending", "000.100.112").unwrap();
        assert_eq!(before, PaymentState::Created);
        assert_eq!(after, PaymentState::Pending3ds);
    }

    #[test]
    fn refund_succeeded_maps_to_refunded() {
        let (before, after) =
            peach_event_to_canonical_state("refund.succeeded", "000.000.000").unwrap();
        assert_eq!(before, PaymentState::Captured);
        assert_eq!(after, PaymentState::Refunded);
    }

    #[test]
    fn refund_failed_remains_captured() {
        let (_before, after) =
            peach_event_to_canonical_state("refund.failed", "800.100.100").unwrap();
        // Result code 800.xxx overrides to Failed
        assert_eq!(after, PaymentState::Failed);

        // With a non-failure result code, refund.failed keeps Captured
        let (before2, after2) =
            peach_event_to_canonical_state("refund.failed", "000.000.000").unwrap();
        assert_eq!(before2, PaymentState::Captured);
        assert_eq!(after2, PaymentState::Captured);
    }

    #[test]
    fn result_code_3ds_redirect_maps_correctly() {
        let (before, after) =
            peach_event_to_canonical_state("charge.pending", "000.100.112").unwrap();
        assert_eq!(before, PaymentState::Created);
        assert_eq!(after, PaymentState::Pending3ds);
    }

    #[test]
    fn result_code_3ds_success_maps_correctly() {
        let (before, after) =
            peach_event_to_canonical_state("charge.succeeded", "000.100.110").unwrap();
        assert_eq!(before, PaymentState::Pending3ds);
        assert_eq!(after, PaymentState::Authorized);
    }

    #[test]
    fn result_code_success_maps_correctly() {
        let (before, after) =
            peach_event_to_canonical_state("charge.succeeded", "000.000.000").unwrap();
        assert_eq!(before, PaymentState::Created);
        assert_eq!(after, PaymentState::Authorized);
    }

    #[test]
    fn result_code_rejected_maps_to_failed() {
        let (_, after) = peach_event_to_canonical_state("charge.succeeded", "800.110.100").unwrap();
        assert_eq!(after, PaymentState::Failed);
    }

    #[test]
    fn result_code_timeout_maps_to_failed() {
        let (_, after) = peach_event_to_canonical_state("charge.succeeded", "900.100.100").unwrap();
        assert_eq!(after, PaymentState::Failed);
    }

    #[test]
    fn unknown_event_type_returns_error() {
        let result = peach_event_to_canonical_state("unknown.event", "000.000.000");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AdapterError::WebhookTranslationFailed { .. }));
    }

    #[test]
    fn three_ds_redirect_event_maps_to_pending3ds() {
        let (before, after) =
            peach_event_to_canonical_state("3ds.redirect", "000.100.112").unwrap();
        assert_eq!(before, PaymentState::Created);
        assert_eq!(after, PaymentState::Pending3ds);
    }

    // -- Event type mapping tests --

    #[test]
    fn event_type_charge_succeeded() {
        let et = peach_event_to_event_type("charge.succeeded").unwrap();
        assert_eq!(et.as_str(), "payment.charge.authorized");
    }

    #[test]
    fn event_type_charge_failed() {
        let et = peach_event_to_event_type("charge.failed").unwrap();
        assert_eq!(et.as_str(), "payment.charge.failed");
    }

    #[test]
    fn event_type_3ds_redirect() {
        let et = peach_event_to_event_type("3ds.redirect").unwrap();
        assert_eq!(et.as_str(), "payment.charge.three_ds_redirect");
    }

    #[test]
    fn event_type_charge_pending() {
        let et = peach_event_to_event_type("charge.pending").unwrap();
        assert_eq!(et.as_str(), "payment.charge.pending");
    }

    #[test]
    fn event_type_refund_succeeded() {
        let et = peach_event_to_event_type("refund.succeeded").unwrap();
        assert_eq!(et.as_str(), "payment.refund.succeeded");
    }

    #[test]
    fn event_type_refund_failed() {
        let et = peach_event_to_event_type("refund.failed").unwrap();
        assert_eq!(et.as_str(), "payment.refund.failed");
    }

    #[test]
    fn event_type_unknown_returns_error() {
        let result = peach_event_to_event_type("unknown.event");
        assert!(result.is_err());
    }

    // -- Amount parsing tests --

    #[test]
    fn amount_parsing_decimal() {
        let money = peach_amount_to_money("100.50", "ZAR").unwrap();
        assert_eq!(money.value, 10050);
        assert_eq!(money.currency, Currency::ZAR);
    }

    #[test]
    fn amount_parsing_whole() {
        let money = peach_amount_to_money("100", "ZAR").unwrap();
        assert_eq!(money.value, 10000);
        assert_eq!(money.currency, Currency::ZAR);
    }

    #[test]
    fn amount_parsing_zero() {
        let money = peach_amount_to_money("0.00", "ZAR").unwrap();
        assert_eq!(money.value, 0);
        assert_eq!(money.currency, Currency::ZAR);
    }

    #[test]
    fn amount_parsing_single_decimal() {
        // "0.5" ZAR -> 50 cents
        let money = peach_amount_to_money("0.5", "ZAR").unwrap();
        assert_eq!(money.value, 50);
    }

    #[test]
    fn amount_parsing_one_cent() {
        let money = peach_amount_to_money("0.01", "ZAR").unwrap();
        assert_eq!(money.value, 1);
    }

    #[test]
    fn amount_parsing_negative_rejected() {
        let result = peach_amount_to_money("-100.50", "ZAR");
        assert!(result.is_err());
    }

    #[test]
    fn amount_parsing_unsupported_currency() {
        let result = peach_amount_to_money("100.00", "NGN");
        assert!(result.is_err());
    }

    #[test]
    fn amount_parsing_too_many_decimals() {
        let result = peach_amount_to_money("100.123", "ZAR");
        assert!(result.is_err());
    }

    #[test]
    fn amount_parsing_invalid_number() {
        let result = peach_amount_to_money("abc", "ZAR");
        assert!(result.is_err());
    }

    // -- money_to_peach_amount tests --

    #[test]
    fn money_to_peach_whole_amount() {
        let money = Money::new(10000, Currency::ZAR);
        assert_eq!(money_to_peach_amount(&money), "100.00");
    }

    #[test]
    fn money_to_peach_fractional_amount() {
        let money = Money::new(10050, Currency::ZAR);
        assert_eq!(money_to_peach_amount(&money), "100.50");
    }

    #[test]
    fn money_to_peach_zero() {
        let money = Money::new(0, Currency::ZAR);
        assert_eq!(money_to_peach_amount(&money), "0.00");
    }

    #[test]
    fn money_to_peach_one_cent() {
        let money = Money::new(1, Currency::ZAR);
        assert_eq!(money_to_peach_amount(&money), "0.01");
    }

    #[test]
    fn money_to_peach_large_amount() {
        let money = Money::new(999999, Currency::ZAR);
        assert_eq!(money_to_peach_amount(&money), "9999.99");
    }

    #[test]
    fn money_to_peach_negative_produces_positive_string() {
        // Peach API amounts are always positive; refunds use paymentType=RF
        let money = Money::new(-10050, Currency::ZAR);
        assert_eq!(money_to_peach_amount(&money), "100.50");
    }

    #[test]
    fn money_to_peach_round_trip() {
        let test_values = [0i64, 1, 50, 100, 10050, 999999, 1234567];
        for &cents in &test_values {
            let money = Money::new(cents, Currency::ZAR);
            let peach_str = money_to_peach_amount(&money);
            let back = peach_amount_to_money(&peach_str, "ZAR").unwrap();
            assert_eq!(back.value, cents, "round-trip failed for {cents} cents");
        }
    }
}
