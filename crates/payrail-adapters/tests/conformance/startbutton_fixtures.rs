use std::collections::HashMap;
use std::time::Duration;

use payrail_adapters::{AdapterConfig, StartbuttonAdapter};
use payrail_core::{PaymentId, PaymentState, RawWebhook};

use super::ConformanceTestable;

// ---------------------------------------------------------------------------
// Startbutton conformance fixtures
// ---------------------------------------------------------------------------

/// Creates a test Startbutton adapter for conformance testing.
pub fn startbutton_test_adapter() -> StartbuttonAdapter {
    StartbuttonAdapter::new(AdapterConfig {
        provider_name: "startbutton".to_owned(),
        sandbox: true,
        base_url: "https://sandbox.startbutton.co/v1".to_owned(),
        api_key_env_var: "STARTBUTTON_SANDBOX_API_KEY".to_owned(),
        webhook_secret_env_var: "STARTBUTTON_SANDBOX_WEBHOOK_SECRET".to_owned(),
        entity_id_env_var: "STARTBUTTON_SANDBOX_ENTITY_ID".to_owned(),
        timeout: Duration::from_secs(30),
    })
}

impl ConformanceTestable for StartbuttonAdapter {
    fn provider_name(&self) -> &str {
        "Startbutton"
    }

    fn make_webhook_for_transition(
        &self,
        from: PaymentState,
        to: PaymentState,
    ) -> Option<RawWebhook> {
        let (event_type, status) = match (from, to) {
            // Created transitions
            (PaymentState::Created, PaymentState::Authorized) => {
                ("payment.authorized", "AUTHORIZED")
            }
            (PaymentState::Created, PaymentState::Pending3ds) => {
                // VERIFY: 3DS initiation event mapping (confidence: 0.78, source: community_report)
                ("payment.authorized", "AWAITING_3DS")
            }
            (PaymentState::Created, PaymentState::Failed) => ("payment.failed", "FAILED"),

            // Pending3ds transitions
            (PaymentState::Pending3ds, PaymentState::Authorized) => {
                ("payment.3ds_completed", "AUTHORIZED")
            }
            (PaymentState::Pending3ds, PaymentState::Failed) => ("payment.3ds_completed", "FAILED"),

            // Authorized transitions
            (PaymentState::Authorized, PaymentState::Captured) => ("payment.captured", "CAPTURED"),
            (PaymentState::Authorized, PaymentState::Voided) => ("payment.voided", "VOIDED"),
            // Startbutton doesn't have a distinct event type for capture failures
            // (unlike Peach's capture.failed). payment.failed is ambiguous — skip.
            (PaymentState::Authorized, PaymentState::Failed) => return None,

            // Captured transitions
            (PaymentState::Captured, PaymentState::Refunded) => ("payment.refunded", "REFUNDED"),
            (PaymentState::Captured, PaymentState::Failed) => ("payment.reversed", "REVERSED"),

            // Timeout transitions are engine-side
            (_, PaymentState::TimedOut) => return None,
            _ => return None,
        };

        Some(make_startbutton_webhook(event_type, status))
    }

    fn make_self_transition_webhook(&self, state: PaymentState) -> Option<RawWebhook> {
        let (event_type, status) = match state {
            PaymentState::Authorized => ("payment.authorized", "AUTHORIZED"),
            PaymentState::Captured => ("payment.captured", "CAPTURED"),
            PaymentState::Refunded => ("payment.refunded", "REFUNDED"),
            PaymentState::Voided => ("payment.voided", "VOIDED"),
            PaymentState::Failed => ("payment.failed", "FAILED"),
            PaymentState::Pending3ds => ("payment.3ds_completed", "AWAITING_3DS"),
            _ => return None,
        };

        Some(make_startbutton_webhook(event_type, status))
    }
}

/// Constructs a Startbutton webhook RawWebhook with the given event type and status.
fn make_startbutton_webhook(event_type: &str, status: &str) -> RawWebhook {
    let pay_id = PaymentId::new();
    let json = serde_json::json!({
        "event": event_type,
        "data": {
            "id": "sb_txn_conformance_001",
            "status": status,
            "amount": 10000,
            "currency": "ZAR",
            "merchant_reference": pay_id.as_str()
        }
    });

    RawWebhook {
        headers: HashMap::new(),
        body: serde_json::to_vec(&json).unwrap(),
    }
}
