use std::fmt;

use serde::{Deserialize, Serialize};

/// Domain-prefixed error codes in SCREAMING_SNAKE_CASE.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    /// Invalid payment state transition attempted.
    #[serde(rename = "PAY_INVALID_TRANSITION")]
    PayInvalidTransition,
    /// Payment amount is invalid (e.g., negative for a charge).
    #[serde(rename = "PAY_INVALID_AMOUNT")]
    PayInvalidAmount,
    /// Unsupported or invalid currency code.
    #[serde(rename = "PAY_INVALID_CURRENCY")]
    PayInvalidCurrency,
    /// Payment exceeded its timeout window.
    #[serde(rename = "PAY_TIMEOUT")]
    PayTimeout,
    /// Duplicate event detected (idempotency).
    #[serde(rename = "PAY_DUPLICATE_EVENT")]
    PayDuplicateEvent,

    /// Provider returned an error response.
    #[serde(rename = "ADAPTER_PROVIDER_ERROR")]
    AdapterProviderError,
    /// Provider did not respond within the timeout window.
    #[serde(rename = "ADAPTER_TIMEOUT")]
    AdapterTimeout,
    /// Provider returned an unparseable response.
    #[serde(rename = "ADAPTER_INVALID_RESPONSE")]
    AdapterInvalidResponse,
    /// Webhook signature verification failed.
    #[serde(rename = "ADAPTER_WEBHOOK_SIGNATURE_INVALID")]
    AdapterWebhookSignatureInvalid,

    /// Knowledge pack confidence below threshold.
    #[serde(rename = "KNOWLEDGE_LOW_CONFIDENCE")]
    KnowledgeLowConfidence,
    /// Requested knowledge pack not found.
    #[serde(rename = "KNOWLEDGE_PACK_NOT_FOUND")]
    KnowledgePackNotFound,

    /// MCP tool execution error.
    #[serde(rename = "MCP_TOOL_ERROR")]
    McpToolError,
    /// Invalid input to an MCP tool.
    #[serde(rename = "MCP_INVALID_INPUT")]
    McpInvalidInput,
}

impl ErrorCode {
    /// Returns the SCREAMING_SNAKE_CASE string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::PayInvalidTransition => "PAY_INVALID_TRANSITION",
            ErrorCode::PayInvalidAmount => "PAY_INVALID_AMOUNT",
            ErrorCode::PayInvalidCurrency => "PAY_INVALID_CURRENCY",
            ErrorCode::PayTimeout => "PAY_TIMEOUT",
            ErrorCode::PayDuplicateEvent => "PAY_DUPLICATE_EVENT",
            ErrorCode::AdapterProviderError => "ADAPTER_PROVIDER_ERROR",
            ErrorCode::AdapterTimeout => "ADAPTER_TIMEOUT",
            ErrorCode::AdapterInvalidResponse => "ADAPTER_INVALID_RESPONSE",
            ErrorCode::AdapterWebhookSignatureInvalid => "ADAPTER_WEBHOOK_SIGNATURE_INVALID",
            ErrorCode::KnowledgeLowConfidence => "KNOWLEDGE_LOW_CONFIDENCE",
            ErrorCode::KnowledgePackNotFound => "KNOWLEDGE_PACK_NOT_FOUND",
            ErrorCode::McpToolError => "MCP_TOOL_ERROR",
            ErrorCode::McpInvalidInput => "MCP_INVALID_INPUT",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured error with domain context.
///
/// Display format: `[WHAT happened] [WHY it happened] [WHAT TO DO about it]`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayRailError {
    /// Machine-readable error code.
    pub code: ErrorCode,
    /// Human-readable description of what went wrong.
    pub message: String,
    /// Error domain for routing (e.g., `"payment"`, `"adapter"`).
    pub domain: String,
    /// Structured context for debugging.
    pub context: serde_json::Value,
}

impl PayRailError {
    /// Creates a new structured error with domain context.
    pub fn new(
        code: ErrorCode,
        message: impl Into<String>,
        domain: impl Into<String>,
        context: serde_json::Value,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            domain: domain.into(),
            context,
        }
    }
}

impl fmt::Display for PayRailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Format: [WHAT] [WHY] [WHAT TO DO]
        write!(
            f,
            "[{}] [{}] [Check {} domain]",
            self.code, self.message, self.domain
        )
    }
}

impl std::error::Error for PayRailError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn error_code_display() {
        assert_eq!(
            ErrorCode::PayInvalidTransition.to_string(),
            "PAY_INVALID_TRANSITION"
        );
        assert_eq!(ErrorCode::AdapterTimeout.to_string(), "ADAPTER_TIMEOUT");
        assert_eq!(ErrorCode::McpToolError.to_string(), "MCP_TOOL_ERROR");
    }

    #[test]
    fn error_code_serde_round_trip() {
        let code = ErrorCode::PayInvalidTransition;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, r#""PAY_INVALID_TRANSITION""#);
        let parsed: ErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, code);
    }

    #[test]
    fn error_construction() {
        let err = PayRailError::new(
            ErrorCode::PayInvalidTransition,
            "Cannot refund a payment in Created state",
            "state_machine",
            json!({
                "current_state": "Created",
                "attempted": "Refund",
                "valid_transitions": ["Authorize", "Fail"]
            }),
        );
        assert_eq!(err.code, ErrorCode::PayInvalidTransition);
        assert_eq!(err.domain, "state_machine");
    }

    #[test]
    fn error_display_format() {
        let err = PayRailError::new(
            ErrorCode::PayInvalidTransition,
            "Cannot refund a payment in Created state",
            "state_machine",
            json!({}),
        );
        let display = err.to_string();
        // Verify [WHAT] [WHY] [WHAT TO DO] format
        assert!(display.contains("[PAY_INVALID_TRANSITION]"));
        assert!(display.contains("[Cannot refund a payment in Created state]"));
        assert!(display.contains("[Check state_machine domain]"));
    }

    #[test]
    fn error_json_serialization() {
        let err = PayRailError::new(
            ErrorCode::PayInvalidTransition,
            "Cannot refund a payment in Created state",
            "state_machine",
            json!({
                "current_state": "Created",
                "attempted": "Refund",
                "valid_transitions": ["Authorize", "Fail"]
            }),
        );
        let json_val = serde_json::to_value(&err).unwrap();
        assert_eq!(json_val["code"], "PAY_INVALID_TRANSITION");
        assert_eq!(json_val["domain"], "state_machine");
        assert_eq!(json_val["context"]["current_state"], "Created");
    }

    #[test]
    fn error_is_std_error() {
        let err = PayRailError::new(
            ErrorCode::AdapterProviderError,
            "Provider returned 500",
            "adapter",
            json!({}),
        );
        // Verify it implements std::error::Error
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn error_code_exhaustive_serde() {
        let all_codes = vec![
            (ErrorCode::PayInvalidTransition, "PAY_INVALID_TRANSITION"),
            (ErrorCode::PayInvalidAmount, "PAY_INVALID_AMOUNT"),
            (ErrorCode::PayInvalidCurrency, "PAY_INVALID_CURRENCY"),
            (ErrorCode::PayTimeout, "PAY_TIMEOUT"),
            (ErrorCode::PayDuplicateEvent, "PAY_DUPLICATE_EVENT"),
            (ErrorCode::AdapterProviderError, "ADAPTER_PROVIDER_ERROR"),
            (ErrorCode::AdapterTimeout, "ADAPTER_TIMEOUT"),
            (
                ErrorCode::AdapterInvalidResponse,
                "ADAPTER_INVALID_RESPONSE",
            ),
            (
                ErrorCode::AdapterWebhookSignatureInvalid,
                "ADAPTER_WEBHOOK_SIGNATURE_INVALID",
            ),
            (
                ErrorCode::KnowledgeLowConfidence,
                "KNOWLEDGE_LOW_CONFIDENCE",
            ),
            (ErrorCode::KnowledgePackNotFound, "KNOWLEDGE_PACK_NOT_FOUND"),
            (ErrorCode::McpToolError, "MCP_TOOL_ERROR"),
            (ErrorCode::McpInvalidInput, "MCP_INVALID_INPUT"),
        ];
        for (variant, expected_str) in all_codes {
            let json = serde_json::to_string(&variant).unwrap();
            let expected_json = format!("\"{}\"", expected_str);
            assert_eq!(json, expected_json, "serialize mismatch for {:?}", variant);
            let parsed: ErrorCode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, variant, "deserialize mismatch for {}", json);
        }
    }

    #[test]
    fn payrail_error_deserialize_round_trip() {
        let err = PayRailError::new(
            ErrorCode::AdapterTimeout,
            "Provider did not respond within 30s",
            "adapter",
            json!({"provider": "peach_payments", "timeout_ms": 30000}),
        );
        let json = serde_json::to_string(&err).unwrap();
        let parsed: PayRailError = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.code, err.code);
        assert_eq!(parsed.message, err.message);
        assert_eq!(parsed.domain, err.domain);
        assert_eq!(parsed.context, err.context);
    }

    // E2-UNIT-003: all 13 ErrorCode variants serialize and deserialize correctly
    #[test]
    fn error_code_all_13_variants_serde() {
        let all_variants = vec![
            ErrorCode::PayInvalidTransition,
            ErrorCode::PayInvalidAmount,
            ErrorCode::PayInvalidCurrency,
            ErrorCode::PayTimeout,
            ErrorCode::PayDuplicateEvent,
            ErrorCode::AdapterProviderError,
            ErrorCode::AdapterTimeout,
            ErrorCode::AdapterInvalidResponse,
            ErrorCode::AdapterWebhookSignatureInvalid,
            ErrorCode::KnowledgeLowConfidence,
            ErrorCode::KnowledgePackNotFound,
            ErrorCode::McpToolError,
            ErrorCode::McpInvalidInput,
        ];
        assert_eq!(
            all_variants.len(),
            13,
            "expected exactly 13 ErrorCode variants"
        );
        for variant in &all_variants {
            let json = serde_json::to_string(variant)
                .unwrap_or_else(|e| panic!("failed to serialize {variant:?}: {e}"));
            let parsed: ErrorCode = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("failed to deserialize {json}: {e}"));
            assert_eq!(&parsed, variant, "round-trip mismatch for {json}");
        }
    }
}
