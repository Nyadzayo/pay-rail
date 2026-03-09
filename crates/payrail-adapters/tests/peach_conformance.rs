mod conformance;

use std::time::Duration;

use conformance::peach_fixtures::peach_test_adapter;
use conformance::{ConformanceTestable, OutputMode, run_conformance};

use payrail_adapters::PaymentAdapter;
use payrail_core::PaymentState;

// ---------------------------------------------------------------------------
// Task 7.1: Full conformance test
// ---------------------------------------------------------------------------

#[test]
fn peach_payments_full_conformance() {
    let adapter = peach_test_adapter();
    let suite = run_conformance(&adapter);

    assert!(
        suite.all_passed(),
        "Conformance failures:\n{}",
        suite.report(OutputMode::Verbose)
    );

    // Verify we tested all supported (non-skipped) transitions
    assert!(
        !suite.results.is_empty(),
        "No conformance results — check fixtures"
    );
}

// ---------------------------------------------------------------------------
// Task 7.2: Semantic failure detection
// ---------------------------------------------------------------------------

/// A deliberately broken adapter that maps charge.failed to Authorized (success).
struct BrokenAdapter;

impl PaymentAdapter for BrokenAdapter {
    fn execute(
        &self,
        _command: payrail_core::PaymentCommand,
        _intent: &payrail_core::PaymentIntent,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<payrail_adapters::PaymentEvent, payrail_adapters::AdapterError>,
                > + Send
                + '_,
        >,
    > {
        unimplemented!()
    }

    fn translate_webhook(
        &self,
        raw: &payrail_core::RawWebhook,
    ) -> Result<payrail_core::CanonicalEvent, payrail_adapters::AdapterError> {
        // Deliberately maps everything to Authorized (broken!)
        let webhook: serde_json::Value = serde_json::from_slice(&raw.body).unwrap();
        let pay_id = webhook["payload"]["merchantTransactionId"]
            .as_str()
            .unwrap()
            .parse::<payrail_core::PaymentId>()
            .unwrap();

        Ok(payrail_core::CanonicalEvent {
            event_id: payrail_core::EventId::new(),
            event_type: payrail_core::EventType::new("payment.charge.authorized").unwrap(),
            payment_id: pay_id,
            provider: "broken".to_owned(),
            timestamp: chrono::Utc::now(),
            state_before: PaymentState::Created,
            state_after: PaymentState::Authorized, // Always Authorized — WRONG for failure events
            amount: payrail_core::Money::new(10000, payrail_core::Currency::ZAR),
            idempotency_key: "broken:test:webhook:evt_001".to_owned(),
            raw_provider_payload: webhook,
            metadata: serde_json::json!({}),
        })
    }

    fn signature_config(&self) -> &payrail_core::SignatureConfig {
        // Use peach config for convenience
        static CONFIG: std::sync::OnceLock<payrail_core::SignatureConfig> =
            std::sync::OnceLock::new();
        CONFIG.get_or_init(payrail_core::SignatureConfig::peach_payments)
    }
}

impl ConformanceTestable for BrokenAdapter {
    fn provider_name(&self) -> &str {
        "Broken Test Provider"
    }

    fn make_webhook_for_transition(
        &self,
        from: PaymentState,
        to: PaymentState,
    ) -> Option<payrail_core::RawWebhook> {
        // Use the Peach adapter fixtures (same webhook format)
        let peach = peach_test_adapter();
        peach.make_webhook_for_transition(from, to)
    }

    fn make_self_transition_webhook(
        &self,
        state: PaymentState,
    ) -> Option<payrail_core::RawWebhook> {
        let peach = peach_test_adapter();
        peach.make_self_transition_webhook(state)
    }
}

#[test]
fn conformance_catches_semantic_failure() {
    let adapter = BrokenAdapter;
    let suite = run_conformance(&adapter);

    // The broken adapter maps everything to Authorized, so failure transitions should fail
    assert!(
        !suite.all_passed(),
        "Broken adapter should not pass conformance"
    );

    let failures = suite.failures();
    assert!(!failures.is_empty(), "Should have failure findings");

    // Check that semantic impact descriptions are present
    let has_critical = failures.iter().any(|f| {
        f.details.as_ref().is_some_and(|d| {
            d.contains("CRITICAL") || d.contains("failure would be treated as success")
        })
    });
    assert!(
        has_critical,
        "Should detect CRITICAL semantic failure where provider failure maps to success.\nFailures: {:?}",
        failures
    );
}

// ---------------------------------------------------------------------------
// Task 7.3: Self-transition validation
// ---------------------------------------------------------------------------

#[test]
fn conformance_self_transitions_produce_no_side_effects() {
    let adapter = peach_test_adapter();
    let suite = run_conformance(&adapter);

    // Filter to self-transition results
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
// Task 7.4: Output mode tests
// ---------------------------------------------------------------------------

#[test]
fn conformance_output_modes() {
    let adapter = peach_test_adapter();
    let suite = run_conformance(&adapter);

    // Summary mode
    let summary = suite.report(OutputMode::Summary);
    assert!(
        summary.contains("Peach Payments Conformance:"),
        "Summary should contain provider name"
    );
    assert!(summary.contains("passed"), "Summary should mention passed");

    // Verbose mode
    let verbose = suite.report(OutputMode::Verbose);
    assert!(
        verbose.contains("PASS"),
        "Verbose should list PASS transitions"
    );
    assert!(
        verbose.contains("Created -> Authorized"),
        "Verbose should list transition names"
    );

    // JSON mode
    let json = suite.report(OutputMode::Json);
    assert!(json.starts_with('{'), "JSON should be object");
    assert!(
        json.contains("\"provider\":\"Peach Payments\""),
        "JSON should contain provider"
    );
    assert!(json.contains("\"passed\""), "JSON should have passed field");
    assert!(
        json.contains("\"results\""),
        "JSON should have results array"
    );

    // Parse JSON to verify it's valid
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["total"].as_u64().unwrap() > 0);
    assert_eq!(parsed["failed"].as_u64().unwrap(), 0);
}

// ---------------------------------------------------------------------------
// Task 7.5: Performance test
// ---------------------------------------------------------------------------

#[test]
fn conformance_completes_within_60_seconds() {
    let adapter = peach_test_adapter();
    let suite = run_conformance(&adapter);

    assert!(
        suite.duration < Duration::from_secs(60),
        "Conformance suite took {:?} — exceeds 60s limit",
        suite.duration
    );
}
