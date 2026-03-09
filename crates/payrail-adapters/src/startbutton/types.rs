use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Startbutton webhook payload types
// ---------------------------------------------------------------------------

/// Deserialized Startbutton webhook JSON body.
///
/// Startbutton uses a flat structure: `{"event": "...", "data": {...}}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartbuttonWebhookPayload {
    /// Webhook event name (e.g., "payment.authorized").
    pub event: String,
    /// Event payload data.
    pub data: StartbuttonPayloadData,
}

/// Transaction details nested inside the webhook data field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartbuttonPayloadData {
    /// Startbutton transaction ID.
    pub id: String,
    /// Provider status string (e.g., "AUTHORIZED", "CAPTURED").
    pub status: String,
    /// Amount in integer cents.
    pub amount: i64,
    /// ISO currency code (e.g., "ZAR").
    pub currency: String,
    /// Our PaymentId sent when creating the payment.
    pub merchant_reference: String,
    /// Error code if present (on failure events).
    #[serde(default)]
    pub error_code: Option<String>,
    /// Error message if present.
    #[serde(default)]
    pub error_message: Option<String>,
    // VERIFY: auth_result field on 3DS events (confidence: 0.72, source: community_report, check: test against sandbox)
    /// 3DS authentication result if present.
    #[serde(default)]
    pub auth_result: Option<String>,
    // VERIFY: reason field on reversal events (confidence: 0.95, source: sandbox_test, check: confirmed in sandbox)
    /// Reason for reversal if present.
    #[serde(default)]
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// StartbuttonStatus enum
// ---------------------------------------------------------------------------

/// Startbutton provider status codes mapped to enum variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartbuttonStatus {
    Authorized,
    Captured,
    Refunded,
    Voided,
    Failed,
    Pending,
    Expired,
    Awaiting3ds,
    /// Undocumented status discovered via sandbox testing.
    Reversed,
    Declined,
    Unknown(String),
}

impl StartbuttonStatus {
    pub fn parse(s: &str) -> Self {
        match s {
            "AUTHORIZED" => Self::Authorized,
            "CAPTURED" => Self::Captured,
            "REFUNDED" => Self::Refunded,
            "VOIDED" => Self::Voided,
            "FAILED" => Self::Failed,
            "PENDING" => Self::Pending,
            "EXPIRED" => Self::Expired,
            "AWAITING_3DS" => Self::Awaiting3ds,
            "REVERSED" => Self::Reversed,
            "DECLINED" => Self::Declined,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

// ---------------------------------------------------------------------------
// StartbuttonEventType enum
// ---------------------------------------------------------------------------

/// Startbutton webhook event types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartbuttonEventType {
    PaymentAuthorized,
    PaymentCaptured,
    PaymentRefunded,
    PaymentVoided,
    PaymentFailed,
    // VERIFY: 3ds_completed event name (confidence: 0.72, source: community_report, check: test against sandbox)
    Payment3dsCompleted,
    // VERIFY: expired event name (confidence: 0.75, source: inferred, check: test against sandbox)
    PaymentExpired,
    /// Undocumented event discovered via sandbox testing.
    PaymentReversed,
    Unknown(String),
}

impl StartbuttonEventType {
    pub fn parse(s: &str) -> Self {
        match s {
            "payment.authorized" => Self::PaymentAuthorized,
            "payment.captured" => Self::PaymentCaptured,
            "payment.refunded" => Self::PaymentRefunded,
            "payment.voided" => Self::PaymentVoided,
            "payment.failed" => Self::PaymentFailed,
            "payment.3ds_completed" => Self::Payment3dsCompleted,
            "payment.expired" => Self::PaymentExpired,
            "payment.reversed" => Self::PaymentReversed,
            other => Self::Unknown(other.to_owned()),
        }
    }
}
