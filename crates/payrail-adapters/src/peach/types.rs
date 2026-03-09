use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Peach Payments webhook payload types (Task 1)
// ---------------------------------------------------------------------------

/// Deserialized Peach Payments webhook JSON body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeachWebhookPayload {
    /// Peach event ID (e.g., "evt_abc123").
    pub id: String,
    /// Peach event type string (e.g., "charge.succeeded").
    #[serde(rename = "type")]
    pub event_type: String,
    /// Nested payload with transaction details.
    pub payload: PeachPayloadData,
}

/// Transaction details nested inside the webhook payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeachPayloadData {
    /// Peach transaction ID.
    pub id: String,
    /// Result code and description.
    pub result: PeachResult,
    /// Amount as decimal string (e.g., "100.50").
    pub amount: String,
    /// ISO currency code (e.g., "ZAR").
    pub currency: String,
    /// Our PaymentId sent when creating the payment.
    pub merchant_transaction_id: String,
}

/// Peach result code and human-readable description.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeachResult {
    /// Result code (e.g., "000.000.000").
    pub code: String,
    /// Human-readable description.
    pub description: String,
}

// ---------------------------------------------------------------------------
// PeachResultCode newtype (Task 1)
// ---------------------------------------------------------------------------

/// Newtype wrapping a Peach result code string (e.g., "000.000.000").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeachResultCode(String);

impl PeachResultCode {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_success(&self) -> bool {
        self.0 == "000.000.000"
    }

    pub fn is_3ds_redirect(&self) -> bool {
        self.0 == "000.100.112"
    }

    pub fn is_3ds_success(&self) -> bool {
        self.0 == "000.100.110"
    }

    pub fn is_rejected(&self) -> bool {
        self.0.starts_with("800.")
    }

    pub fn is_timeout_or_error(&self) -> bool {
        self.0.starts_with("900.")
    }

    pub fn is_3ds_failure(&self) -> bool {
        self.0.starts_with("100.")
    }
}

// ---------------------------------------------------------------------------
// PeachEventType enum (Task 1)
// ---------------------------------------------------------------------------

/// Peach event type strings mapped to enum variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeachEventType {
    ChargeSucceeded,
    ChargeFailed,
    ChargePending,
    CaptureSucceeded,
    CaptureFailed,
    VoidSucceeded,
    RefundSucceeded,
    RefundFailed,
    ThreeDsRedirect,
    Unknown(String),
}

impl PeachEventType {
    pub fn parse(s: &str) -> Self {
        match s {
            "charge.succeeded" => Self::ChargeSucceeded,
            "charge.failed" => Self::ChargeFailed,
            "charge.pending" => Self::ChargePending,
            "capture.succeeded" => Self::CaptureSucceeded,
            "capture.failed" => Self::CaptureFailed,
            "void.succeeded" => Self::VoidSucceeded,
            "refund.succeeded" => Self::RefundSucceeded,
            "refund.failed" => Self::RefundFailed,
            "3ds.redirect" => Self::ThreeDsRedirect,
            other => Self::Unknown(other.to_owned()),
        }
    }
}
