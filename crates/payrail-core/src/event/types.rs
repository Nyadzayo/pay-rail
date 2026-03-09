use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::{EventId, PaymentId};
use crate::payment::types::{Money, PaymentState};

/// Custom serde module ensuring ISO 8601 UTC with fixed millisecond precision.
/// Guarantees output like `2026-03-05T14:30:00.000Z` (always 3 decimal places).
mod timestamp_millis {
    use chrono::{DateTime, NaiveDateTime, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3fZ";

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = date.format(FORMAT).to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        // Accept any valid RFC 3339 / ISO 8601 UTC timestamp
        if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
            return Ok(dt.with_timezone(&Utc));
        }
        NaiveDateTime::parse_from_str(&s, FORMAT)
            .map(|ndt| ndt.and_utc())
            .map_err(|e| serde::de::Error::custom(e.to_string()))
    }
}

/// Validated event type enforcing `domain.entity.action` format.
///
/// Examples: `payment.charge.captured`, `payment.refund.completed`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EventType(String);

impl EventType {
    /// Regex pattern: one or more lowercase segments separated by dots,
    /// with exactly three segments: domain.entity.action
    /// Validates against `^[a-z]+\.[a-z]+\.[a-z_]+$`:
    /// - Segments 1-2 (domain, entity): lowercase letters only
    /// - Segment 3 (action): lowercase letters and underscores
    fn is_valid(s: &str) -> bool {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return false;
        }
        let alpha_only = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase());
        let alpha_underscore =
            |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c == '_');

        alpha_only(parts[0]) && alpha_only(parts[1]) && alpha_underscore(parts[2])
    }

    /// Creates a new event type, validating the `domain.entity.action` format.
    pub fn new(s: impl Into<String>) -> Result<Self, EventTypeError> {
        let s = s.into();
        if Self::is_valid(&s) {
            Ok(Self(s))
        } else {
            Err(EventTypeError::InvalidFormat(s))
        }
    }

    /// Returns the event type as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for EventType {
    type Err = EventTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for EventType {
    type Error = EventTypeError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

impl From<EventType> for String {
    fn from(et: EventType) -> Self {
        et.0
    }
}

/// Errors from parsing or creating an [`EventType`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum EventTypeError {
    /// The input does not match the required `domain.entity.action` format.
    #[error(
        "[EVENT_TYPE_INVALID] '{0}' does not match domain.entity.action format [Use lowercase a-z and underscores, exactly 3 dot-separated segments]"
    )]
    InvalidFormat(String),
}

/// Canonical event — the normalized representation of any provider event.
///
/// All fields are required. Use `serde_json::Value::Null` or empty object for
/// `raw_provider_payload` / `metadata` when no data is available.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalEvent {
    /// Unique event identifier.
    pub event_id: EventId,
    /// Validated event type in `domain.entity.action` format.
    pub event_type: EventType,
    /// The payment this event relates to.
    pub payment_id: PaymentId,
    /// Provider that originated the event.
    pub provider: String,
    /// When the event occurred (UTC, millisecond precision).
    #[serde(with = "timestamp_millis")]
    pub timestamp: DateTime<Utc>,
    /// Payment state before this event.
    pub state_before: PaymentState,
    /// Payment state after this event.
    pub state_after: PaymentState,
    /// Transaction amount.
    pub amount: Money,
    /// Deterministic key for deduplication.
    pub idempotency_key: String,
    /// Original provider payload for audit.
    pub raw_provider_payload: serde_json::Value,
    /// Application-defined metadata.
    pub metadata: serde_json::Value,
}

impl fmt::Display for CanonicalEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Event({}, {}, {} -> {})",
            self.event_id, self.event_type, self.state_before, self.state_after
        )
    }
}

/// Raw webhook payload before normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawWebhook {
    /// HTTP headers from the webhook request.
    pub headers: std::collections::HashMap<String, String>,
    /// Raw request body bytes.
    pub body: Vec<u8>,
}

impl fmt::Display for RawWebhook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawWebhook({} bytes)", self.body.len())
    }
}

/// Wraps a [`CanonicalEvent`] with routing metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// The canonical event.
    pub event: CanonicalEvent,
    /// When PayRail received this event.
    #[serde(with = "timestamp_millis")]
    pub received_at: DateTime<Utc>,
    /// Origin of the event (e.g., `"webhook"`, `"api"`).
    pub source: String,
    /// Optional correlation ID for request tracing.
    pub correlation_id: Option<String>,
}

impl fmt::Display for EventEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "EventEnvelope({}, src={})",
            self.event.event_id, self.source
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::types::Currency;
    use serde_json::json;

    // -- EventType tests --

    #[test]
    fn event_type_valid_formats() {
        assert!(EventType::new("payment.charge.captured").is_ok());
        assert!(EventType::new("payment.refund.completed").is_ok());
        assert!(EventType::new("payment.charge.three_ds_required").is_ok());
    }

    #[test]
    fn event_type_invalid_formats() {
        // Too few segments
        assert!(EventType::new("payment.charge").is_err());
        // Too many segments
        assert!(EventType::new("payment.charge.captured.extra").is_err());
        // Uppercase
        assert!(EventType::new("Payment.Charge.Captured").is_err());
        // Empty segment
        assert!(EventType::new("payment..captured").is_err());
        // Numbers
        assert!(EventType::new("payment.charge.3ds").is_err());
        // Empty string
        assert!(EventType::new("").is_err());
        // Underscores in domain (first segment) — not allowed per spec
        assert!(EventType::new("some_domain.charge.captured").is_err());
        // Underscores in entity (second segment) — not allowed per spec
        assert!(EventType::new("payment.some_entity.captured").is_err());
    }

    #[test]
    fn event_type_display() {
        let et = EventType::new("payment.charge.captured").unwrap();
        assert_eq!(et.to_string(), "payment.charge.captured");
    }

    #[test]
    fn event_type_serde_round_trip() {
        let et = EventType::new("payment.charge.captured").unwrap();
        let json = serde_json::to_string(&et).unwrap();
        assert_eq!(json, r#""payment.charge.captured""#);
        let parsed: EventType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, et);
    }

    #[test]
    fn event_type_serde_invalid() {
        let result: Result<EventType, _> = serde_json::from_str(r#""INVALID""#);
        assert!(result.is_err());
    }

    // -- CanonicalEvent tests --

    fn sample_event() -> CanonicalEvent {
        use chrono::TimeZone;
        CanonicalEvent {
            event_id: crate::id::EventId::new(),
            event_type: EventType::new("payment.charge.captured").unwrap(),
            payment_id: crate::id::PaymentId::new(),
            provider: "peach_payments".to_owned(),
            timestamp: Utc.with_ymd_and_hms(2026, 3, 5, 12, 0, 0).unwrap(),
            state_before: PaymentState::Authorized,
            state_after: PaymentState::Captured,
            amount: Money::new(15000, Currency::ZAR),
            idempotency_key: "peach:merchant123:webhook:evt_abc".to_owned(),
            raw_provider_payload: json!({}),
            metadata: json!({}),
        }
    }

    #[test]
    fn canonical_event_construction() {
        let event = sample_event();
        assert_eq!(event.provider, "peach_payments");
        assert_eq!(event.amount.value, 15000);
        assert_eq!(event.amount.currency, Currency::ZAR);
    }

    #[test]
    fn canonical_event_serde_round_trip() {
        let event = sample_event();
        let json = serde_json::to_string(&event).unwrap();
        let parsed: CanonicalEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider, event.provider);
        assert_eq!(parsed.amount, event.amount);
        assert_eq!(parsed.event_type, event.event_type);
        assert_eq!(parsed.state_before, event.state_before);
        assert_eq!(parsed.state_after, event.state_after);
    }

    #[test]
    fn canonical_event_json_structure() {
        let event = sample_event();
        let val = serde_json::to_value(&event).unwrap();
        // Verify all required fields are present
        assert!(val.get("event_id").is_some());
        assert!(val.get("event_type").is_some());
        assert!(val.get("payment_id").is_some());
        assert!(val.get("provider").is_some());
        assert!(val.get("timestamp").is_some());
        assert!(val.get("state_before").is_some());
        assert!(val.get("state_after").is_some());
        assert!(val.get("amount").is_some());
        assert!(val.get("idempotency_key").is_some());
        assert!(val.get("raw_provider_payload").is_some());
        assert!(val.get("metadata").is_some());
    }

    #[test]
    fn canonical_event_timestamp_millis_precision() {
        let event = sample_event();
        let val = serde_json::to_value(&event).unwrap();
        let ts = val["timestamp"].as_str().unwrap();
        // Must end with .NNNZ (exactly 3 decimal digits before Z)
        assert!(ts.ends_with('Z'), "Timestamp must end with Z: {ts}");
        let dot_pos = ts.rfind('.').expect("Timestamp must have decimal point");
        let frac_len = ts.len() - 1 - dot_pos - 1; // exclude dot and Z
        assert_eq!(frac_len, 3, "Timestamp must have exactly 3 ms digits: {ts}");
    }

    // -- EventEnvelope tests --

    #[test]
    fn event_envelope_serde_round_trip() {
        use chrono::TimeZone;
        let envelope = EventEnvelope {
            event: sample_event(),
            received_at: Utc.with_ymd_and_hms(2026, 3, 5, 12, 1, 0).unwrap(),
            source: "webhook".to_owned(),
            correlation_id: Some("corr_123".to_owned()),
        };
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: EventEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.source, "webhook");
        assert_eq!(parsed.correlation_id, Some("corr_123".to_owned()));
    }

    // -- RawWebhook tests --

    #[test]
    fn raw_webhook_construction() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("content-type".to_owned(), "application/json".to_owned());
        let raw = RawWebhook {
            headers,
            body: b"{}".to_vec(),
        };
        assert_eq!(raw.headers.get("content-type").unwrap(), "application/json");
        assert_eq!(raw.body, b"{}");
    }

    // -- Spec compliance & extended tests --

    #[test]
    fn canonical_event_json_field_names_match_spec() {
        let event = sample_event();
        let val = serde_json::to_value(&event).unwrap();
        let obj = val.as_object().unwrap();
        let expected_fields = vec![
            "event_id",
            "event_type",
            "payment_id",
            "provider",
            "timestamp",
            "state_before",
            "state_after",
            "amount",
            "idempotency_key",
            "raw_provider_payload",
            "metadata",
        ];
        assert_eq!(obj.len(), expected_fields.len(), "Field count mismatch");
        for field in &expected_fields {
            assert!(obj.contains_key(*field), "Missing field: {}", field);
        }
    }

    #[test]
    fn canonical_event_serde_full_equality() {
        // sample_event() already uses a fixed millis-precision timestamp,
        // so round-trip through the millis serializer preserves equality.
        let event = sample_event();
        let json = serde_json::to_string(&event).unwrap();
        let parsed: CanonicalEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event, "Full round-trip should preserve equality");
    }

    #[test]
    fn event_type_rejects_underscores_in_domain_entity() {
        // Underscores only allowed in third segment (action)
        assert!(EventType::new("some_domain.charge.captured").is_err());
        assert!(EventType::new("payment.some_entity.captured").is_err());
        assert!(EventType::new("pay_ment.charge.captured").is_err());
        assert!(EventType::new("payment.ch_arge.captured").is_err());
        // But underscores in action segment ARE valid
        assert!(EventType::new("payment.charge.three_ds_required").is_ok());
        assert!(EventType::new("payment.charge.partially_captured").is_ok());
    }

    #[test]
    fn timestamp_deserializes_varying_precision() {
        // 0 fractional digits (no decimal)
        let json_0 = r#"{"event_id":"evt_01HXYZ01HXYZ01HXYZ01HXYZ01","event_type":"payment.charge.captured","payment_id":"pay_01HXYZ01HXYZ01HXYZ01HXYZ01","provider":"test","timestamp":"2026-03-05T14:30:00Z","state_before":"Authorized","state_after":"Captured","amount":{"value":100,"currency":"ZAR"},"idempotency_key":"k1","raw_provider_payload":{},"metadata":{}}"#;
        let evt0: CanonicalEvent = serde_json::from_str(json_0).unwrap();
        assert_eq!(evt0.timestamp.timestamp(), 1772721000);

        // 6 fractional digits (microseconds)
        let json_6 = json_0.replace("2026-03-05T14:30:00Z", "2026-03-05T14:30:00.123456Z");
        let evt6: CanonicalEvent = serde_json::from_str(&json_6).unwrap();
        assert_eq!(evt6.timestamp.timestamp(), 1772721000);

        // 9 fractional digits (nanoseconds)
        let json_9 = json_0.replace("2026-03-05T14:30:00Z", "2026-03-05T14:30:00.123456789Z");
        let evt9: CanonicalEvent = serde_json::from_str(&json_9).unwrap();
        assert_eq!(evt9.timestamp.timestamp(), 1772721000);
    }

    #[test]
    fn timestamp_zero_nanosecond_produces_millis() {
        use chrono::TimeZone;
        let event = CanonicalEvent {
            timestamp: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            ..sample_event()
        };
        let val = serde_json::to_value(&event).unwrap();
        let ts = val["timestamp"].as_str().unwrap();
        assert!(ts.ends_with(".000Z"), "Expected .000Z, got: {}", ts);
    }

    // -- E1 Display Format Tests --

    #[test]
    fn canonical_event_display() {
        // E1-UNIT-006
        let event = sample_event();
        let display = event.to_string();
        assert!(display.starts_with("Event(evt_"));
        assert!(display.contains("payment.charge.captured"));
        assert!(display.contains("Authorized -> Captured"));
    }

    #[test]
    fn raw_webhook_display() {
        // E1-UNIT-007
        let webhook = RawWebhook {
            headers: std::collections::HashMap::new(),
            body: vec![1, 2, 3, 4, 5],
        };
        assert_eq!(webhook.to_string(), "RawWebhook(5 bytes)");
    }

    #[test]
    fn event_envelope_display() {
        // E1-UNIT-008
        let envelope = EventEnvelope {
            event: sample_event(),
            received_at: Utc::now(),
            source: "webhook".to_owned(),
            correlation_id: Some("corr-123".to_owned()),
        };
        let display = envelope.to_string();
        assert!(display.starts_with("EventEnvelope(evt_"));
        assert!(display.contains("src=webhook"));
    }
}
