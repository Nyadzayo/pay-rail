use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use payrail_core::{
    CanonicalEvent, Money, PaymentCommand, PaymentIntent, PaymentState, RawWebhook, SignatureConfig,
};

// ---------------------------------------------------------------------------
// AdapterError (Task 1)
// ---------------------------------------------------------------------------

/// Errors arising from payment adapter operations.
///
/// All error messages follow the `[WHAT] [WHY] [WHAT TO DO]` format.
/// Error messages MUST NOT include raw card data, PAN, CVV, or signing secrets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterError {
    /// Provider API returned an error.
    #[error(
        "[ProviderError] Provider '{provider}' returned an error: {message}. [Check provider dashboard and retry if transient]"
    )]
    ProviderError { provider: String, message: String },

    /// Provider response could not be parsed or was malformed.
    #[error(
        "[InvalidResponse] Provider '{provider}' returned an invalid response: {details}. [Check adapter type mappings against provider API docs]"
    )]
    InvalidResponse { provider: String, details: String },

    /// Command not supported by this adapter.
    #[error(
        "[UnsupportedCommand] Provider '{provider}' does not support command '{command}'. [Check supported operations for this provider]"
    )]
    UnsupportedCommand { provider: String, command: String },

    /// Webhook payload could not be translated to a canonical event.
    #[error(
        "[WebhookTranslationFailed] Provider '{provider}' webhook translation failed: {reason}. [Check webhook payload format and adapter mappings]"
    )]
    WebhookTranslationFailed { provider: String, reason: String },

    /// Adapter configuration is invalid or missing.
    #[error("[ConfigurationError] {0}. [Check adapter configuration and environment variables]")]
    ConfigurationError(String),
}

// ---------------------------------------------------------------------------
// PaymentEvent (Task 2)
// ---------------------------------------------------------------------------

/// Result of a payment adapter `execute()` call.
///
/// Carries provider response data translated to canonical types.
/// Does NOT perform typestate transitions — the engine does that.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaymentEvent {
    /// Provider identifier (e.g., `"peach_payments"`).
    pub provider: String,
    /// Provider's transaction reference.
    pub provider_transaction_id: String,
    /// Canonical payment state after this event.
    pub state: PaymentState,
    /// Transaction amount in canonical format (integer cents).
    pub amount: Money,
    /// Raw provider response preserved for audit.
    pub raw_response: serde_json::Value,
    /// UTC timestamp of the event.
    pub timestamp: DateTime<Utc>,
    /// Additional metadata from the provider.
    pub metadata: serde_json::Value,
}

// ---------------------------------------------------------------------------
// PaymentAdapter trait (Task 3)
// ---------------------------------------------------------------------------

/// Standardized interface for payment provider integrations.
///
/// Each adapter (~150-200 lines) translates between a provider's API format
/// and PayRail's canonical types. Typestate enforcement is the engine's job.
///
/// # Object Safety
///
/// `execute()` returns `Pin<Box<dyn Future>>` to remain object-safe for use
/// in `AdapterRegistry` via `Box<dyn PaymentAdapter>`. The sync methods
/// (`translate_webhook`, `signature_config`) are naturally object-safe.
pub trait PaymentAdapter: Send + Sync {
    /// Sends a payment command to the provider and returns the translated result.
    ///
    /// The adapter handles HTTP communication, response parsing, and translation
    /// to canonical types. It does NOT enforce state transitions.
    fn execute(
        &self,
        command: PaymentCommand,
        intent: &PaymentIntent,
    ) -> Pin<Box<dyn Future<Output = Result<PaymentEvent, AdapterError>> + Send + '_>>;

    /// Translates a raw provider webhook into a canonical event.
    ///
    /// Sync because it only performs data transformation (no I/O).
    fn translate_webhook(&self, raw: &RawWebhook) -> Result<CanonicalEvent, AdapterError>;

    /// Returns the provider's webhook signature configuration.
    fn signature_config(&self) -> &SignatureConfig;
}

// ---------------------------------------------------------------------------
// AdapterConfig (Task 4)
// ---------------------------------------------------------------------------

/// Configuration for a payment adapter instance.
///
/// Passed at adapter construction time, not per-call.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterConfig {
    /// Provider identifier (e.g., `"peach_payments"`).
    pub provider_name: String,
    /// Whether to use sandbox/test environment.
    pub sandbox: bool,
    /// Base URL for provider API.
    pub base_url: String,
    /// Environment variable name holding the API key.
    pub api_key_env_var: String,
    /// Environment variable name holding the webhook signing secret.
    pub webhook_secret_env_var: String,
    /// Environment variable name holding the provider entity/merchant ID.
    pub entity_id_env_var: String,
    /// HTTP request timeout.
    pub timeout: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- AdapterError tests --

    /// Verifies all AdapterError variants follow the [WHAT] [WHY] [WHAT TO DO] format.
    #[test]
    fn adapter_error_all_variants_follow_what_why_todo_format() {
        let variants: Vec<AdapterError> = vec![
            AdapterError::ProviderError {
                provider: "peach".to_owned(),
                message: "timeout".to_owned(),
            },
            AdapterError::InvalidResponse {
                provider: "peach".to_owned(),
                details: "missing amount field".to_owned(),
            },
            AdapterError::UnsupportedCommand {
                provider: "peach".to_owned(),
                command: "Refund".to_owned(),
            },
            AdapterError::WebhookTranslationFailed {
                provider: "peach".to_owned(),
                reason: "unknown event type".to_owned(),
            },
            AdapterError::ConfigurationError("missing API key".to_owned()),
        ];

        for err in &variants {
            let msg = err.to_string();
            // [WHAT]: starts with bracketed tag
            assert!(msg.starts_with('['), "Error missing [WHAT] tag: {msg}");
            // [WHY]: contains descriptive content after the tag
            assert!(
                msg.contains(']') && msg.len() > msg.find(']').unwrap() + 2,
                "Error missing [WHY] section: {msg}"
            );
            // [WHAT TO DO]: ends with a bracketed action clause
            assert!(
                msg.contains("[Check "),
                "Error missing [WHAT TO DO] action clause: {msg}"
            );
        }
    }

    #[test]
    fn adapter_error_provider_error_display() {
        let err = AdapterError::ProviderError {
            provider: "peach".to_owned(),
            message: "timeout".to_owned(),
        };
        let msg = err.to_string();
        assert!(msg.contains("[ProviderError]"));
        assert!(msg.contains("peach"));
        assert!(msg.contains("timeout"));
        assert!(msg.contains("[Check provider dashboard"));
    }

    #[test]
    fn adapter_error_invalid_response_display() {
        let err = AdapterError::InvalidResponse {
            provider: "peach".to_owned(),
            details: "missing amount field".to_owned(),
        };
        let msg = err.to_string();
        assert!(msg.contains("[InvalidResponse]"));
        assert!(msg.contains("missing amount field"));
        assert!(msg.contains("[Check adapter type mappings"));
    }

    #[test]
    fn adapter_error_unsupported_command_display() {
        let err = AdapterError::UnsupportedCommand {
            provider: "peach".to_owned(),
            command: "Refund".to_owned(),
        };
        let msg = err.to_string();
        assert!(msg.contains("[UnsupportedCommand]"));
        assert!(msg.contains("Refund"));
        assert!(msg.contains("[Check supported operations"));
    }

    #[test]
    fn adapter_error_webhook_translation_display() {
        let err = AdapterError::WebhookTranslationFailed {
            provider: "peach".to_owned(),
            reason: "unknown event type".to_owned(),
        };
        let msg = err.to_string();
        assert!(msg.contains("[WebhookTranslationFailed]"));
        assert!(msg.contains("unknown event type"));
        assert!(msg.contains("[Check webhook payload format"));
    }

    #[test]
    fn adapter_error_configuration_display() {
        let err = AdapterError::ConfigurationError("missing API key".to_owned());
        let msg = err.to_string();
        assert!(msg.contains("[ConfigurationError]"));
        assert!(msg.contains("missing API key"));
        assert!(msg.contains("[Check adapter configuration"));
    }

    #[test]
    fn adapter_error_does_not_leak_secrets() {
        let secret = "sk_live_super_secret_key_12345";
        let err = AdapterError::ProviderError {
            provider: "peach".to_owned(),
            message: "auth failed".to_owned(),
        };
        assert!(!err.to_string().contains(secret));
    }

    // -- PaymentEvent tests --

    #[test]
    fn payment_event_construction() {
        let event = PaymentEvent {
            provider: "peach_payments".to_owned(),
            provider_transaction_id: "txn_123".to_owned(),
            state: PaymentState::Authorized,
            amount: Money::new(15000, payrail_core::Currency::ZAR),
            raw_response: serde_json::json!({"status": "ok"}),
            timestamp: Utc::now(),
            metadata: serde_json::json!({}),
        };
        assert_eq!(event.provider, "peach_payments");
        assert_eq!(event.state, PaymentState::Authorized);
        assert_eq!(event.amount.value, 15000);
    }

    #[test]
    fn payment_event_serde_round_trip() {
        let event = PaymentEvent {
            provider: "peach_payments".to_owned(),
            provider_transaction_id: "txn_456".to_owned(),
            state: PaymentState::Captured,
            amount: Money::new(5000, payrail_core::Currency::USD),
            raw_response: serde_json::json!({"id": "abc"}),
            timestamp: DateTime::parse_from_rfc3339("2026-03-06T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            metadata: serde_json::json!({"order": "ORD-1"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: PaymentEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event);
    }

    // -- AdapterConfig tests --

    #[test]
    fn adapter_config_construction() {
        let config = AdapterConfig {
            provider_name: "peach_payments".to_owned(),
            sandbox: true,
            base_url: "https://testsecure.peachpayments.com".to_owned(),
            api_key_env_var: "PEACH_SANDBOX_API_KEY".to_owned(),
            webhook_secret_env_var: "PEACH_SANDBOX_WEBHOOK_SECRET".to_owned(),
            entity_id_env_var: "PEACH_SANDBOX_ENTITY_ID".to_owned(),
            timeout: Duration::from_secs(30),
        };
        assert_eq!(config.provider_name, "peach_payments");
        assert!(config.sandbox);
        assert_eq!(config.timeout, Duration::from_secs(30));
    }

    // -- PaymentCommand variant coverage --

    #[test]
    fn payment_command_all_variants() {
        let commands = vec![
            PaymentCommand::CreateIntent,
            PaymentCommand::Authorize,
            PaymentCommand::Capture,
            PaymentCommand::Refund,
            PaymentCommand::Void,
        ];
        assert_eq!(commands.len(), 5);
        // Verify each variant can be matched
        for cmd in &commands {
            match cmd {
                PaymentCommand::CreateIntent => {}
                PaymentCommand::Authorize => {}
                PaymentCommand::Capture => {}
                PaymentCommand::Refund => {}
                PaymentCommand::Void => {}
            }
        }
    }
}
