mod conformance;

use std::time::Duration;

use conformance::startbutton_fixtures::startbutton_test_adapter;
use conformance::{OutputMode, run_conformance};

use payrail_adapters::PaymentAdapter;
use payrail_core::PaymentState;

// ---------------------------------------------------------------------------
// Task 5.1-5.2: Full conformance test
// ---------------------------------------------------------------------------

#[test]
fn startbutton_full_conformance() {
    let adapter = startbutton_test_adapter();
    let suite = run_conformance(&adapter);

    assert!(
        suite.all_passed(),
        "Conformance failures:\n{}",
        suite.report(OutputMode::Verbose)
    );

    assert!(
        !suite.results.is_empty(),
        "No conformance results — check fixtures"
    );
}

// ---------------------------------------------------------------------------
// Task 5.3: Self-transition validation
// ---------------------------------------------------------------------------

#[test]
fn startbutton_self_transitions_produce_no_side_effects() {
    let adapter = startbutton_test_adapter();
    let suite = run_conformance(&adapter);

    let self_results: Vec<_> = suite
        .results
        .iter()
        .filter(|r| r.transition.contains("(self)"))
        .collect();

    assert!(
        !self_results.is_empty(),
        "Should have self-transition results"
    );

    for r in &self_results {
        assert!(
            r.passed,
            "Self-transition should pass: {} — {:?}",
            r.transition, r.details
        );
        assert_eq!(
            r.expected, r.actual,
            "Self-transition expected == actual for {}",
            r.transition
        );
    }
}

// ---------------------------------------------------------------------------
// Task 5.4: Invalid transition rejection
// ---------------------------------------------------------------------------

#[test]
fn startbutton_unknown_event_rejected() {
    let adapter = startbutton_test_adapter();
    let raw = payrail_core::RawWebhook {
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "event": "payment.unknown",
            "data": {
                "id": "sb_txn_001",
                "status": "AUTHORIZED",
                "amount": 10000,
                "currency": "ZAR",
                "merchant_reference": payrail_core::PaymentId::new().as_str().to_owned()
            }
        }))
        .unwrap(),
    };

    let result = adapter.translate_webhook(&raw);
    assert!(result.is_err(), "Unknown event should be rejected");
}

#[test]
fn startbutton_unknown_status_rejected() {
    let adapter = startbutton_test_adapter();
    let raw = payrail_core::RawWebhook {
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "event": "payment.authorized",
            "data": {
                "id": "sb_txn_001",
                "status": "MYSTERY_STATUS",
                "amount": 10000,
                "currency": "ZAR",
                "merchant_reference": payrail_core::PaymentId::new().as_str().to_owned()
            }
        }))
        .unwrap(),
    };

    let result = adapter.translate_webhook(&raw);
    assert!(result.is_err(), "Unknown status should be rejected");
}

// ---------------------------------------------------------------------------
// Task 5.5: Webhook normalization
// ---------------------------------------------------------------------------

#[test]
fn startbutton_webhook_normalization_authorized() {
    let adapter = startbutton_test_adapter();
    let pay_id = payrail_core::PaymentId::new();
    let raw = payrail_core::RawWebhook {
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "event": "payment.authorized",
            "data": {
                "id": "sb_txn_001",
                "status": "AUTHORIZED",
                "amount": 15000,
                "currency": "ZAR",
                "merchant_reference": pay_id.as_str()
            }
        }))
        .unwrap(),
    };

    let event = adapter.translate_webhook(&raw).unwrap();
    assert_eq!(event.provider, "startbutton");
    assert_eq!(event.payment_id, pay_id);
    assert_eq!(event.state_before, PaymentState::Created);
    assert_eq!(event.state_after, PaymentState::Authorized);
    assert_eq!(event.amount.value, 15000);
    assert_eq!(event.event_type.as_str(), "payment.charge.authorized");
}

#[test]
fn startbutton_webhook_normalization_refund() {
    let adapter = startbutton_test_adapter();
    let pay_id = payrail_core::PaymentId::new();
    let raw = payrail_core::RawWebhook {
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "event": "payment.refunded",
            "data": {
                "id": "sb_txn_002",
                "status": "REFUNDED",
                "amount": 5000,
                "currency": "USD",
                "merchant_reference": pay_id.as_str()
            }
        }))
        .unwrap(),
    };

    let event = adapter.translate_webhook(&raw).unwrap();
    assert_eq!(event.state_before, PaymentState::Captured);
    assert_eq!(event.state_after, PaymentState::Refunded);
    assert_eq!(event.amount.value, 5000);
    assert_eq!(event.event_type.as_str(), "payment.refund.succeeded");
}

#[test]
fn startbutton_webhook_preserves_raw_payload() {
    let adapter = startbutton_test_adapter();
    let raw = payrail_core::RawWebhook {
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "event": "payment.captured",
            "data": {
                "id": "sb_txn_003",
                "status": "CAPTURED",
                "amount": 10000,
                "currency": "ZAR",
                "merchant_reference": payrail_core::PaymentId::new().as_str().to_owned()
            }
        }))
        .unwrap(),
    };

    let event = adapter.translate_webhook(&raw).unwrap();
    assert!(event.raw_provider_payload.is_object());
    assert_eq!(event.raw_provider_payload["event"], "payment.captured");
}

#[test]
fn startbutton_webhook_invalid_json_returns_error() {
    let adapter = startbutton_test_adapter();
    let raw = payrail_core::RawWebhook {
        headers: std::collections::HashMap::new(),
        body: b"not json".to_vec(),
    };

    let result = adapter.translate_webhook(&raw);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Task 5.6: Signature verification
// ---------------------------------------------------------------------------

#[test]
fn startbutton_signature_config_correct() {
    let adapter = startbutton_test_adapter();
    let config = adapter.signature_config();
    assert_eq!(config.header_name, "X-Startbutton-Signature");
    assert_eq!(config.secret_env_var, "STARTBUTTON_SANDBOX_WEBHOOK_SECRET");
    assert_eq!(config.method, payrail_core::SignatureMethod::HmacSha256);
}

// ---------------------------------------------------------------------------
// Task 5.7: Output format validation
// ---------------------------------------------------------------------------

#[test]
fn startbutton_conformance_output_modes() {
    let adapter = startbutton_test_adapter();
    let suite = run_conformance(&adapter);

    let summary = suite.report(OutputMode::Summary);
    assert!(summary.contains("Startbutton Conformance:"));
    assert!(summary.contains("passed"));

    let verbose = suite.report(OutputMode::Verbose);
    assert!(verbose.contains("PASS"));

    let json = suite.report(OutputMode::Json);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["total"].as_u64().unwrap() > 0);
    assert_eq!(parsed["failed"].as_u64().unwrap(), 0);
}

// ---------------------------------------------------------------------------
// Task 5.8: Performance
// ---------------------------------------------------------------------------

#[test]
fn startbutton_conformance_completes_within_60_seconds() {
    let adapter = startbutton_test_adapter();
    let suite = run_conformance(&adapter);

    assert!(
        suite.duration < Duration::from_secs(60),
        "Conformance suite took {:?} — exceeds 60s limit",
        suite.duration
    );
}

// ---------------------------------------------------------------------------
// Task 6: Graceful degradation for VERIFY facts
// ---------------------------------------------------------------------------

#[test]
fn startbutton_graceful_degradation_unknown_3ds_result() {
    let adapter = startbutton_test_adapter();
    // VERIFY: 3ds_completed with unexpected auth_result should degrade gracefully
    let raw = payrail_core::RawWebhook {
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "event": "payment.3ds_completed",
            "data": {
                "id": "sb_txn_3ds",
                "status": "AUTHORIZED",
                "amount": 10000,
                "currency": "ZAR",
                "merchant_reference": payrail_core::PaymentId::new().as_str().to_owned(),
                "auth_result": "unexpected_value"
            }
        }))
        .unwrap(),
    };

    // Should NOT panic — graceful degradation
    let result = adapter.translate_webhook(&raw);
    assert!(
        result.is_ok(),
        "Should handle unexpected 3DS result gracefully"
    );
    let event = result.unwrap();
    assert_eq!(event.state_after, PaymentState::Authorized);
}

#[test]
fn startbutton_graceful_degradation_reversed_with_reason() {
    let adapter = startbutton_test_adapter();
    let raw = payrail_core::RawWebhook {
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "event": "payment.reversed",
            "data": {
                "id": "sb_txn_rev",
                "status": "REVERSED",
                "amount": 10000,
                "currency": "ZAR",
                "merchant_reference": payrail_core::PaymentId::new().as_str().to_owned(),
                "reason": "provider_initiated"
            }
        }))
        .unwrap(),
    };

    let result = adapter.translate_webhook(&raw);
    assert!(result.is_ok(), "Should handle reversed event gracefully");
    let event = result.unwrap();
    assert_eq!(event.state_after, PaymentState::Failed);
}

#[test]
fn startbutton_graceful_degradation_expired_event() {
    let adapter = startbutton_test_adapter();
    // VERIFY: expired event (confidence: 0.75, source: inferred)
    let raw = payrail_core::RawWebhook {
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "event": "payment.expired",
            "data": {
                "id": "sb_txn_exp",
                "status": "EXPIRED",
                "amount": 10000,
                "currency": "ZAR",
                "merchant_reference": payrail_core::PaymentId::new().as_str().to_owned()
            }
        }))
        .unwrap(),
    };

    let result = adapter.translate_webhook(&raw);
    assert!(result.is_ok(), "Should handle expired event gracefully");
    let event = result.unwrap();
    assert_eq!(event.state_after, PaymentState::TimedOut);
}
