use std::fmt;

use serde::{Deserialize, Serialize};

/// ISO 4217 currency codes supported by PayRail.
///
/// Extensible — add new variants as providers are onboarded.
///
/// # Example
///
/// ```
/// use payrail_core::Currency;
///
/// assert_eq!(Currency::ZAR.to_string(), "ZAR");
/// assert_eq!(Currency::ZAR.minor_unit_digits(), 2);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Currency {
    /// South African Rand.
    #[serde(rename = "ZAR")]
    ZAR,
    /// United States Dollar.
    #[serde(rename = "USD")]
    USD,
    /// Euro.
    #[serde(rename = "EUR")]
    EUR,
    /// British Pound Sterling.
    #[serde(rename = "GBP")]
    GBP,
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Currency::ZAR => write!(f, "ZAR"),
            Currency::USD => write!(f, "USD"),
            Currency::EUR => write!(f, "EUR"),
            Currency::GBP => write!(f, "GBP"),
        }
    }
}

impl Currency {
    /// Number of decimal places for this currency's minor unit.
    pub fn minor_unit_digits(&self) -> u8 {
        match self {
            Currency::ZAR | Currency::USD | Currency::EUR | Currency::GBP => 2,
        }
    }
}

/// Money value in the smallest currency unit (e.g., cents).
///
/// Both `value` and `currency` are required — no implicit currency.
/// Value is `i64` to support negative amounts (refunds) while preventing float contamination.
///
/// # Example
///
/// ```
/// use payrail_core::prelude::*;
///
/// let amount = Money::new(15000, Currency::ZAR);
/// assert_eq!(amount.to_string(), "ZAR 150.00");
///
/// let refund = Money::new(-5000, Currency::ZAR);
/// assert_eq!(refund.to_string(), "ZAR -50.00");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    /// Amount in smallest currency unit (e.g., cents). Never floating point.
    pub value: i64,
    /// ISO 4217 currency code.
    pub currency: Currency,
}

impl Money {
    /// Creates a new money value in the smallest currency unit (e.g., cents).
    pub fn new(value: i64, currency: Currency) -> Self {
        Self { value, currency }
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = self.currency.minor_unit_digits() as u32;
        let divisor = 10_i64.pow(digits);
        let abs = self.value.unsigned_abs();
        let whole = abs / divisor as u64;
        let frac = abs % divisor as u64;
        let sign = if self.value < 0 { "-" } else { "" };
        write!(
            f,
            "{} {sign}{whole}.{frac:0>width$}",
            self.currency,
            width = digits as usize
        )
    }
}

// Compile-time guard: prevent float contamination.
// These trait impls are intentionally absent:
//   impl From<f64> for Money  — FORBIDDEN
//   impl From<f32> for Money  — FORBIDDEN

/// Commands that can be issued against a payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentCommand {
    /// Create a new payment intent.
    CreateIntent,
    /// Authorize the payment with the provider.
    Authorize,
    /// Capture authorized funds.
    Capture,
    /// Refund a captured payment.
    Refund,
    /// Void an authorized payment.
    Void,
}

impl fmt::Display for PaymentIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PaymentIntent({}, {})", self.id, self.amount)
    }
}

impl fmt::Display for PaymentCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PaymentCommand::CreateIntent => write!(f, "CreateIntent"),
            PaymentCommand::Authorize => write!(f, "Authorize"),
            PaymentCommand::Capture => write!(f, "Capture"),
            PaymentCommand::Refund => write!(f, "Refund"),
            PaymentCommand::Void => write!(f, "Void"),
        }
    }
}

/// A payment intent — the initial request to process a payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentIntent {
    /// Unique payment identifier.
    pub id: crate::id::PaymentId,
    /// Amount to charge in integer cents.
    pub amount: Money,
    /// Payment provider identifier (e.g., `"peach_payments"`).
    pub provider: String,
    /// Application-defined metadata (order IDs, references, etc.).
    pub metadata: serde_json::Value,
}

/// The logical state a payment can be in.
///
/// Used in `CanonicalEvent` for `state_before`/`state_after` fields.
/// The typestate machine (Story 1.3) provides compile-time enforcement;
/// this enum is for runtime representation in events and serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentState {
    /// Payment created, awaiting authorization.
    #[serde(rename = "Created")]
    Created,
    /// Awaiting 3D Secure verification.
    #[serde(rename = "Pending3DS")]
    Pending3ds,
    /// Payment authorized by provider.
    #[serde(rename = "Authorized")]
    Authorized,
    /// Funds captured (collected).
    #[serde(rename = "Captured")]
    Captured,
    /// Payment refunded (terminal).
    #[serde(rename = "Refunded")]
    Refunded,
    /// Payment voided (terminal).
    #[serde(rename = "Voided")]
    Voided,
    /// Payment failed (terminal).
    #[serde(rename = "Failed")]
    Failed,
    /// Payment timed out.
    #[serde(rename = "TimedOut")]
    TimedOut,
    /// Payment settled after reconciliation (terminal).
    #[serde(rename = "Settled")]
    Settled,
}

impl fmt::Display for PaymentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PaymentState::Created => write!(f, "Created"),
            PaymentState::Pending3ds => write!(f, "Pending3DS"),
            PaymentState::Authorized => write!(f, "Authorized"),
            PaymentState::Captured => write!(f, "Captured"),
            PaymentState::Refunded => write!(f, "Refunded"),
            PaymentState::Voided => write!(f, "Voided"),
            PaymentState::Failed => write!(f, "Failed"),
            PaymentState::TimedOut => write!(f, "TimedOut"),
            PaymentState::Settled => write!(f, "Settled"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Currency tests --

    #[test]
    fn currency_display() {
        assert_eq!(Currency::ZAR.to_string(), "ZAR");
        assert_eq!(Currency::USD.to_string(), "USD");
        assert_eq!(Currency::EUR.to_string(), "EUR");
        assert_eq!(Currency::GBP.to_string(), "GBP");
    }

    #[test]
    fn currency_serde_round_trip() {
        let json = serde_json::to_string(&Currency::ZAR).unwrap();
        assert_eq!(json, r#""ZAR""#);
        let parsed: Currency = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Currency::ZAR);
    }

    #[test]
    fn currency_invalid_deserialize() {
        let result: Result<Currency, _> = serde_json::from_str(r#""INVALID""#);
        assert!(result.is_err());
    }

    // -- Money tests --

    #[test]
    fn money_construction() {
        let m = Money::new(15000, Currency::ZAR);
        assert_eq!(m.value, 15000);
        assert_eq!(m.currency, Currency::ZAR);
    }

    #[test]
    fn money_display_formatting() {
        assert_eq!(Money::new(15000, Currency::ZAR).to_string(), "ZAR 150.00");
        assert_eq!(Money::new(99, Currency::USD).to_string(), "USD 0.99");
        assert_eq!(Money::new(0, Currency::EUR).to_string(), "EUR 0.00");
        assert_eq!(Money::new(100, Currency::GBP).to_string(), "GBP 1.00");
    }

    #[test]
    fn money_display_negative() {
        assert_eq!(Money::new(-5000, Currency::ZAR).to_string(), "ZAR -50.00");
        assert_eq!(Money::new(-99, Currency::ZAR).to_string(), "ZAR -0.99");
        assert_eq!(Money::new(-1, Currency::USD).to_string(), "USD -0.01");
    }

    #[test]
    fn money_serde_round_trip() {
        let m = Money::new(15000, Currency::ZAR);
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"value":15000,"currency":"ZAR"}"#);
        let parsed: Money = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, m);
    }

    #[test]
    fn money_equality() {
        let a = Money::new(100, Currency::USD);
        let b = Money::new(100, Currency::USD);
        let c = Money::new(100, Currency::EUR);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // -- PaymentCommand tests --

    #[test]
    fn payment_command_serde() {
        let json = serde_json::to_string(&PaymentCommand::CreateIntent).unwrap();
        assert_eq!(json, r#""create_intent""#);
        let parsed: PaymentCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PaymentCommand::CreateIntent);
    }

    #[test]
    fn payment_command_display() {
        assert_eq!(PaymentCommand::Capture.to_string(), "Capture");
    }

    // -- PaymentState tests --

    #[test]
    fn payment_state_serde_round_trip() {
        let json = serde_json::to_string(&PaymentState::Pending3ds).unwrap();
        assert_eq!(json, r#""Pending3DS""#);
        let parsed: PaymentState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PaymentState::Pending3ds);

        // Verify PascalCase matches architecture spec
        assert_eq!(
            serde_json::to_string(&PaymentState::Authorized).unwrap(),
            r#""Authorized""#
        );
        assert_eq!(
            serde_json::to_string(&PaymentState::TimedOut).unwrap(),
            r#""TimedOut""#
        );
    }

    #[test]
    fn payment_state_display() {
        assert_eq!(PaymentState::Created.to_string(), "Created");
        assert_eq!(PaymentState::Pending3ds.to_string(), "Pending3DS");
        assert_eq!(PaymentState::TimedOut.to_string(), "TimedOut");
    }

    #[test]
    fn money_i64_max_boundary() {
        let m = Money::new(i64::MAX, Currency::USD);
        assert_eq!(m.value, i64::MAX);
        let display = m.to_string();
        // i64::MAX = 9223372036854775807 => 92233720368547758.07
        assert_eq!(display, "USD 92233720368547758.07");
    }

    #[test]
    fn money_i64_min_boundary() {
        let m = Money::new(i64::MIN, Currency::USD);
        assert_eq!(m.value, i64::MIN);
        let display = m.to_string();
        // i64::MIN = -9223372036854775808 => -92233720368547758.08
        assert_eq!(display, "USD -92233720368547758.08");
    }

    #[test]
    fn money_display_single_digit_cents() {
        assert_eq!(Money::new(1, Currency::ZAR).to_string(), "ZAR 0.01");
        assert_eq!(Money::new(9, Currency::EUR).to_string(), "EUR 0.09");
        assert_eq!(Money::new(-1, Currency::GBP).to_string(), "GBP -0.01");
        assert_eq!(Money::new(-9, Currency::USD).to_string(), "USD -0.09");
    }

    #[test]
    fn payment_state_exhaustive_serde() {
        let cases = vec![
            (PaymentState::Created, r#""Created""#),
            (PaymentState::Pending3ds, r#""Pending3DS""#),
            (PaymentState::Authorized, r#""Authorized""#),
            (PaymentState::Captured, r#""Captured""#),
            (PaymentState::Refunded, r#""Refunded""#),
            (PaymentState::Voided, r#""Voided""#),
            (PaymentState::Failed, r#""Failed""#),
            (PaymentState::TimedOut, r#""TimedOut""#),
        ];
        for (variant, expected_json) in cases {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected_json, "serialize failed for {:?}", variant);
            let parsed: PaymentState = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, variant, "deserialize failed for {}", json);
        }
    }

    #[test]
    fn payment_state_display_matches_serde() {
        let all_states = vec![
            PaymentState::Created,
            PaymentState::Pending3ds,
            PaymentState::Authorized,
            PaymentState::Captured,
            PaymentState::Refunded,
            PaymentState::Voided,
            PaymentState::Failed,
            PaymentState::TimedOut,
        ];
        for state in all_states {
            let display = state.to_string();
            let serde_str = serde_json::to_string(&state).unwrap();
            // serde_str is like "\"Created\"", strip quotes
            let serde_bare = &serde_str[1..serde_str.len() - 1];
            assert_eq!(
                display, serde_bare,
                "Display/serde mismatch for {:?}",
                state
            );
        }
    }

    #[test]
    fn currency_all_variants_display_and_digits() {
        let cases = vec![
            (Currency::ZAR, "ZAR", 2),
            (Currency::USD, "USD", 2),
            (Currency::EUR, "EUR", 2),
            (Currency::GBP, "GBP", 2),
        ];
        for (currency, expected_display, expected_digits) in cases {
            assert_eq!(currency.to_string(), expected_display);
            assert_eq!(currency.minor_unit_digits(), expected_digits);
        }
    }

    #[test]
    fn payment_intent_serde_round_trip() {
        let intent = PaymentIntent {
            id: crate::id::PaymentId::new(),
            amount: Money::new(25000, Currency::ZAR),
            provider: "peach_payments".to_owned(),
            metadata: serde_json::json!({"order_id": "ORD-123"}),
        };
        let json = serde_json::to_string(&intent).unwrap();
        let parsed: PaymentIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, intent.id);
        assert_eq!(parsed.amount, intent.amount);
        assert_eq!(parsed.provider, intent.provider);
        assert_eq!(parsed.metadata, intent.metadata);
    }

    // -- Compile-time guard (verified by absence, not by test) --
    // No From<f64> or From<f32> for Money exists.
    // Attempting `Money::from(1.0_f64)` would fail to compile.

    // -- E1 Edge Case Tests --

    #[test]
    fn payment_intent_display() {
        // E1-UNIT-005: PaymentIntent Display format
        let intent = PaymentIntent {
            id: crate::id::PaymentId::new(),
            amount: Money::new(15000, Currency::ZAR),
            provider: "test".to_owned(),
            metadata: serde_json::json!({}),
        };
        let display = intent.to_string();
        assert!(display.starts_with("PaymentIntent(pay_"));
        assert!(display.contains("ZAR 150.00"));
    }

    #[test]
    fn payment_state_is_copy() {
        // E1-UNIT-009: PaymentState implements Copy
        let state = PaymentState::Created;
        let copy = state; // Copy, not move
        assert_eq!(state, copy); // original still usable
    }
}
