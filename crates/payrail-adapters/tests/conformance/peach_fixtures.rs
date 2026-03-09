use std::collections::HashMap;
use std::time::Duration;

use payrail_adapters::{AdapterConfig, PeachPaymentsAdapter};
use payrail_core::{PaymentId, PaymentState, RawWebhook};

use super::ConformanceTestable;

// ---------------------------------------------------------------------------
// Task 6: Peach Payments conformance fixtures
// ---------------------------------------------------------------------------

/// Creates a test Peach adapter for conformance testing.
pub fn peach_test_adapter() -> PeachPaymentsAdapter {
    PeachPaymentsAdapter::new(AdapterConfig {
        provider_name: "peach_payments".to_owned(),
        sandbox: true,
        base_url: "https://testsecure.peachpayments.com".to_owned(),
        api_key_env_var: "PEACH_SANDBOX_API_KEY".to_owned(),
        webhook_secret_env_var: "PEACH_SANDBOX_WEBHOOK_SECRET".to_owned(),
        entity_id_env_var: "PEACH_SANDBOX_ENTITY_ID".to_owned(),
        timeout: Duration::from_secs(30),
    })
}

impl ConformanceTestable for PeachPaymentsAdapter {
    fn provider_name(&self) -> &str {
        "Peach Payments"
    }

    fn make_webhook_for_transition(
        &self,
        from: PaymentState,
        to: PaymentState,
    ) -> Option<RawWebhook> {
        let (event_type, result_code) = match (from, to) {
            // Created transitions
            (PaymentState::Created, PaymentState::Authorized) => {
                ("charge.succeeded", "000.000.000")
            }
            (PaymentState::Created, PaymentState::Pending3ds) => ("charge.pending", "000.100.112"),
            (PaymentState::Created, PaymentState::Failed) => ("charge.failed", "800.100.100"),

            // Pending3ds transitions
            (PaymentState::Pending3ds, PaymentState::Authorized) => {
                ("charge.succeeded", "000.100.110")
            }
            (PaymentState::Pending3ds, PaymentState::Failed) => {
                ("charge.failed", "100.390.112") // 3DS authentication failure
            }

            // Authorized transitions
            (PaymentState::Authorized, PaymentState::Captured) => {
                ("capture.succeeded", "000.000.000")
            }
            (PaymentState::Authorized, PaymentState::Voided) => ("void.succeeded", "000.000.000"),
            (PaymentState::Authorized, PaymentState::Failed) => ("capture.failed", "800.100.100"),

            // Captured transitions
            (PaymentState::Captured, PaymentState::Refunded) => ("refund.succeeded", "000.000.000"),

            // Timeout and unsupported transitions via webhooks
            (_, PaymentState::TimedOut) => return None, // Engine-side
            (PaymentState::Captured, PaymentState::Failed) => return None, // Peach: refund.failed stays Captured
            _ => return None,
        };

        Some(make_peach_webhook(event_type, result_code))
    }

    fn make_self_transition_webhook(&self, state: PaymentState) -> Option<RawWebhook> {
        // Self-transition: duplicate event for a payment already in this state.
        // The adapter maps the same webhook to the same target state.
        let (event_type, result_code) = match state {
            PaymentState::Authorized => ("charge.succeeded", "000.000.000"),
            PaymentState::Captured => ("capture.succeeded", "000.000.000"),
            PaymentState::Refunded => ("refund.succeeded", "000.000.000"),
            PaymentState::Voided => ("void.succeeded", "000.000.000"),
            PaymentState::Failed => ("charge.failed", "800.100.100"),
            _ => return None,
        };

        Some(make_peach_webhook(event_type, result_code))
    }
}

/// Constructs a Peach webhook RawWebhook with the given event type and result code.
fn make_peach_webhook(event_type: &str, result_code: &str) -> RawWebhook {
    let pay_id = PaymentId::new();
    let json = serde_json::json!({
        "id": "evt_conformance_001",
        "type": event_type,
        "payload": {
            "id": "txn_conformance_001",
            "result": {
                "code": result_code,
                "description": "Conformance test"
            },
            "amount": "100.00",
            "currency": "ZAR",
            "merchantTransactionId": pay_id.as_str()
        }
    });

    RawWebhook {
        headers: HashMap::new(),
        body: serde_json::to_vec(&json).unwrap(),
    }
}
