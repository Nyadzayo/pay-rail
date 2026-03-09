#![allow(dead_code)]

pub mod peach_fixtures;
pub mod startbutton_fixtures;

use std::time::{Duration, Instant};

use payrail_adapters::PaymentAdapter;
use payrail_core::{PaymentState, RawWebhook};

// ---------------------------------------------------------------------------
// Task 1: Conformance framework types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ConformanceResult {
    pub transition: String,
    pub passed: bool,
    pub expected: PaymentState,
    pub actual: PaymentState,
    pub details: Option<String>,
    pub source_hint: Option<String>,
}

pub struct ConformanceSuite {
    pub provider: String,
    pub results: Vec<ConformanceResult>,
    pub skipped: Vec<String>,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Summary,
    Verbose,
    Json,
}

// ---------------------------------------------------------------------------
// Task 1.2-1.5: ConformanceSuite methods
// ---------------------------------------------------------------------------

impl ConformanceSuite {
    pub fn summary(&self) -> String {
        let passed = self.results.iter().filter(|r| r.passed).count();
        let total = self.results.len();
        let skipped = self.skipped.len();
        let mut s = if passed == total {
            format!("{} Conformance: {passed}/{total} passed", self.provider)
        } else {
            format!(
                "{} Conformance: {passed}/{total} passed ({} failures)",
                self.provider,
                total - passed
            )
        };
        if skipped > 0 {
            s.push_str(&format!(", {skipped} skipped"));
        }
        s
    }

    pub fn report(&self, mode: OutputMode) -> String {
        match mode {
            OutputMode::Summary => self.summary(),
            OutputMode::Verbose => self.format_verbose(),
            OutputMode::Json => self.format_json(),
        }
    }

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    pub fn failures(&self) -> Vec<&ConformanceResult> {
        self.results.iter().filter(|r| !r.passed).collect()
    }

    // -- Task 5: Output formatting --

    fn format_verbose(&self) -> String {
        let mut out = self.summary();
        for r in &self.results {
            if r.passed {
                out.push_str(&format!("\n  PASS  {}", r.transition));
            } else {
                out.push_str(&format!("\n  FAIL  {}", r.transition));
                if let Some(ref details) = r.details {
                    out.push_str(&format!(": {details}"));
                }
                if let Some(ref hint) = r.source_hint {
                    out.push_str(&format!("\n        Fix: {hint}"));
                }
            }
        }
        for s in &self.skipped {
            out.push_str(&format!("\n  SKIP  {s}"));
        }
        out
    }

    fn format_json(&self) -> String {
        let passed = self.results.iter().filter(|r| r.passed).count();
        let failed = self.results.len() - passed;
        let results_json: Vec<serde_json::Value> = self
            .results
            .iter()
            .map(|r| {
                let mut obj = serde_json::json!({
                    "transition": r.transition,
                    "passed": r.passed,
                    "expected": format!("{:?}", r.expected),
                    "actual": format!("{:?}", r.actual),
                });
                if let Some(ref d) = r.details {
                    obj["details"] = serde_json::json!(d);
                }
                if let Some(ref h) = r.source_hint {
                    obj["source_hint"] = serde_json::json!(h);
                }
                obj
            })
            .collect();
        let output = serde_json::json!({
            "provider": self.provider,
            "total": self.results.len(),
            "passed": passed,
            "failed": failed,
            "skipped": self.skipped.len(),
            "duration_ms": self.duration.as_millis() as u64,
            "results": results_json,
        });
        serde_json::to_string(&output).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Task 2: ConformanceTestable trait
// ---------------------------------------------------------------------------

pub trait ConformanceTestable {
    fn provider_name(&self) -> &str;

    /// Creates a synthetic webhook that should trigger the given state transition.
    /// Returns None if the adapter cannot handle this transition via webhooks
    /// (e.g., timeout transitions are engine-side).
    fn make_webhook_for_transition(
        &self,
        from: PaymentState,
        to: PaymentState,
    ) -> Option<RawWebhook>;

    /// Creates a webhook for a duplicate event in the given state.
    /// Returns None if self-transitions can't be tested for this state.
    fn make_self_transition_webhook(&self, state: PaymentState) -> Option<RawWebhook>;
}

// ---------------------------------------------------------------------------
// Task 3: Canonical state transition matrix
// ---------------------------------------------------------------------------

pub const CANONICAL_TRANSITIONS: &[(PaymentState, PaymentState)] = &[
    (PaymentState::Created, PaymentState::Authorized),
    (PaymentState::Created, PaymentState::Pending3ds),
    (PaymentState::Created, PaymentState::Failed),
    (PaymentState::Created, PaymentState::TimedOut),
    (PaymentState::Pending3ds, PaymentState::Authorized),
    (PaymentState::Pending3ds, PaymentState::Failed),
    (PaymentState::Pending3ds, PaymentState::TimedOut),
    (PaymentState::Authorized, PaymentState::Captured),
    (PaymentState::Authorized, PaymentState::Voided),
    (PaymentState::Authorized, PaymentState::Failed),
    (PaymentState::Authorized, PaymentState::TimedOut),
    (PaymentState::Captured, PaymentState::Refunded),
    (PaymentState::Captured, PaymentState::Failed),
    (PaymentState::Captured, PaymentState::TimedOut),
];

pub const SELF_TRANSITION_STATES: &[PaymentState] = &[
    PaymentState::Authorized,
    PaymentState::Captured,
    PaymentState::Refunded,
    PaymentState::Voided,
    PaymentState::Failed,
];

// ---------------------------------------------------------------------------
// Task 4: Conformance test runner
// ---------------------------------------------------------------------------

pub fn run_conformance<A: PaymentAdapter + ConformanceTestable>(adapter: &A) -> ConformanceSuite {
    let start = Instant::now();
    let mut results = Vec::new();
    let mut skipped = Vec::new();
    let provider = adapter.provider_name().to_owned();

    // Test each canonical transition
    for &(from, to) in CANONICAL_TRANSITIONS {
        let transition = format!("{from:?} -> {to:?}");

        let webhook = match adapter.make_webhook_for_transition(from, to) {
            Some(w) => w,
            None => {
                skipped.push(transition);
                continue;
            }
        };

        let result = adapter.translate_webhook(&webhook);
        match result {
            Ok(event) => {
                let state_ok = event.state_after == to && event.state_before == from;
                if state_ok {
                    results.push(ConformanceResult {
                        transition,
                        passed: true,
                        expected: to,
                        actual: event.state_after,
                        details: None,
                        source_hint: None,
                    });
                } else {
                    let impact = semantic_impact(from, to, event.state_after);
                    results.push(ConformanceResult {
                        transition,
                        passed: false,
                        expected: to,
                        actual: event.state_after,
                        details: Some(format!(
                            "Expected {from:?}->{to:?} but got {:?}->{:?}. Impact: {impact}",
                            event.state_before, event.state_after
                        )),
                        source_hint: Some(
                            "Check mappings.rs peach_event_to_canonical_state() for this transition".to_owned()
                        ),
                    });
                }
            }
            Err(e) => {
                results.push(ConformanceResult {
                    transition,
                    passed: false,
                    expected: to,
                    actual: from, // Use source state as actual on error
                    details: Some(format!("Adapter error: {e}")),
                    source_hint: Some(
                        "Check adapter translate_webhook() implementation".to_owned(),
                    ),
                });
            }
        }
    }

    // Test self-transitions
    for &state in SELF_TRANSITION_STATES {
        let transition = format!("{state:?} -> {state:?} (self)");

        let webhook = match adapter.make_self_transition_webhook(state) {
            Some(w) => w,
            None => {
                skipped.push(transition);
                continue;
            }
        };

        let result = adapter.translate_webhook(&webhook);
        match result {
            Ok(event) => {
                // Only check state_after for self-transitions. The adapter infers
                // state_before from event type (not current payment state), so it
                // may differ. The engine layer reconciles state_before at apply time.
                let passed = event.state_after == state;
                let details = if passed {
                    None
                } else {
                    Some(format!(
                        "Self-transition expected state_after={state:?} but got {:?}",
                        event.state_after
                    ))
                };
                results.push(ConformanceResult {
                    transition,
                    passed,
                    expected: state,
                    actual: event.state_after,
                    details,
                    source_hint: None,
                });
            }
            Err(e) => {
                results.push(ConformanceResult {
                    transition,
                    passed: false,
                    expected: state,
                    actual: state,
                    details: Some(format!("Adapter error on self-transition: {e}")),
                    source_hint: None,
                });
            }
        }
    }

    ConformanceSuite {
        provider,
        results,
        skipped,
        duration: start.elapsed(),
    }
}

/// Describes the impact of a semantic mapping failure.
fn semantic_impact(
    from: PaymentState,
    expected_to: PaymentState,
    actual_to: PaymentState,
) -> String {
    let is_failure_expected = matches!(expected_to, PaymentState::Failed | PaymentState::TimedOut);
    let is_success_actual = matches!(
        actual_to,
        PaymentState::Authorized | PaymentState::Captured | PaymentState::Refunded
    );

    if is_failure_expected && is_success_actual {
        "CRITICAL: Provider failure would be treated as success".to_owned()
    } else if !is_failure_expected
        && matches!(actual_to, PaymentState::Failed | PaymentState::TimedOut)
    {
        "Provider success would be treated as failure".to_owned()
    } else {
        format!(
            "State mismatch: expected {expected_to:?} but mapped to {actual_to:?} from {from:?}"
        )
    }
}
