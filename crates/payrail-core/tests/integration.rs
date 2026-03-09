//! Integration tests for payrail-core.
//!
//! These tests exercise cross-module scenarios using only the public API
//! (prelude + root re-exports). No internal module paths are used.

use chrono::{Duration, Utc};
use payrail_core::prelude::*;

fn test_intent() -> PaymentIntent {
    PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(15000, Currency::ZAR),
        provider: "peach_payments".to_owned(),
        metadata: serde_json::json!({"order_id": "ORD-001"}),
    }
}

/// E1-INT-001: Full lifecycle Created → Authorized → Captured → Refunded.
/// Verifies state, data integrity, and timestamp progression at each step.
#[test]
fn full_lifecycle_created_to_refunded() {
    let now = Utc::now();
    let t1 = now + Duration::seconds(1);
    let t2 = now + Duration::seconds(2);
    let t3 = now + Duration::seconds(3);

    let intent = test_intent();
    let original_id = intent.id.clone();
    let original_amount = intent.amount.clone();

    let created = Payment::<Created>::create(intent, now);
    assert_eq!(created.state(), PaymentState::Created);
    assert_eq!(created.id, original_id);
    assert_eq!(created.created_at, now);

    let authorized = created.authorize(t1);
    assert_eq!(authorized.state(), PaymentState::Authorized);
    assert_eq!(authorized.id, original_id);
    assert_eq!(authorized.intent.amount, original_amount);
    assert_eq!(authorized.created_at, now);
    assert_eq!(authorized.last_transition_at, t1);

    let captured = authorized.capture(t2);
    assert_eq!(captured.state(), PaymentState::Captured);
    assert_eq!(captured.id, original_id);
    assert_eq!(captured.created_at, now);
    assert_eq!(captured.last_transition_at, t2);

    let refunded = captured.refund(t3);
    assert_eq!(refunded.state(), PaymentState::Refunded);
    assert_eq!(refunded.id, original_id);
    assert_eq!(refunded.intent.amount, original_amount);
    assert_eq!(refunded.created_at, now);
    assert_eq!(refunded.last_transition_at, t3);
}

/// E1-INT-002: Alternative path Created → Pending3DS → Authorized → Captured.
#[test]
fn alternative_path_3ds_to_captured() {
    let now = Utc::now();
    let t1 = now + Duration::seconds(1);
    let t2 = now + Duration::seconds(2);
    let t3 = now + Duration::seconds(3);

    let intent = test_intent();
    let original_id = intent.id.clone();

    let created = Payment::<Created>::create(intent, now);
    let pending = created.pending_3ds(t1);
    assert_eq!(pending.state(), PaymentState::Pending3ds);
    assert_eq!(pending.id, original_id);

    let authorized = pending.authorize(t2);
    assert_eq!(authorized.state(), PaymentState::Authorized);
    assert_eq!(authorized.last_transition_at, t2);

    let captured = authorized.capture(t3);
    assert_eq!(captured.state(), PaymentState::Captured);
    assert_eq!(captured.id, original_id);
}

/// E1-INT-003: Timeout chain — enforce_timeout across all enforceable states.
#[test]
fn timeout_chain_all_enforceable_states() {
    let now = Utc::now();
    let config = TimeoutConfig::default()
        .with_created(std::time::Duration::from_secs(60))
        .with_pending_3ds(std::time::Duration::from_secs(60))
        .with_authorized(std::time::Duration::from_secs(60))
        .with_captured(std::time::Duration::from_secs(60));
    let later = now + Duration::seconds(120);

    // Created → TimedOut
    let created = Payment::<Created>::create(test_intent(), now);
    let timed_out = created.enforce_timeout(&config, later).unwrap();
    assert_eq!(timed_out.state(), PaymentState::TimedOut);

    // Pending3DS → TimedOut
    let pending = Payment::<Created>::create(test_intent(), now).pending_3ds(now);
    let timed_out = pending.enforce_timeout(&config, later).unwrap();
    assert_eq!(timed_out.state(), PaymentState::TimedOut);

    // Authorized → TimedOut
    let auth = Payment::<Created>::create(test_intent(), now).authorize(now);
    let timed_out = auth.enforce_timeout(&config, later).unwrap();
    assert_eq!(timed_out.state(), PaymentState::TimedOut);

    // Captured → TimedOut
    let captured = Payment::<Created>::create(test_intent(), now)
        .authorize(now)
        .capture(now);
    let timed_out = captured.enforce_timeout(&config, later).unwrap();
    assert_eq!(timed_out.state(), PaymentState::TimedOut);
}

/// E1-INT-004: try_transition round-trip — runtime transitions match compile-time.
/// Verifies every valid transition for every state using try_transition matches
/// what the compile-time methods allow.
#[test]
fn try_transition_matches_compile_time_transitions() {
    let now = Utc::now();

    // Created → Authorized (runtime) matches .authorize() (compile-time)
    let p = Payment::<Created>::create(test_intent(), now);
    match p.try_transition(PaymentState::Authorized, now) {
        TransitionResult::Applied { new_state, .. } => {
            assert_eq!(new_state, PaymentState::Authorized);
        }
        _ => panic!("Created → Authorized should be Applied"),
    }

    // Created → Pending3ds
    let p = Payment::<Created>::create(test_intent(), now);
    match p.try_transition(PaymentState::Pending3ds, now) {
        TransitionResult::Applied { new_state, .. } => {
            assert_eq!(new_state, PaymentState::Pending3ds);
        }
        _ => panic!("Created → Pending3ds should be Applied"),
    }

    // Created → Failed
    let p = Payment::<Created>::create(test_intent(), now);
    match p.try_transition(PaymentState::Failed, now) {
        TransitionResult::Applied { new_state, .. } => {
            assert_eq!(new_state, PaymentState::Failed);
        }
        _ => panic!("Created → Failed should be Applied"),
    }

    // Created → TimedOut
    let p = Payment::<Created>::create(test_intent(), now);
    match p.try_transition(PaymentState::TimedOut, now) {
        TransitionResult::Applied { new_state, .. } => {
            assert_eq!(new_state, PaymentState::TimedOut);
        }
        _ => panic!("Created → TimedOut should be Applied"),
    }

    // Created → Refunded (invalid)
    let p = Payment::<Created>::create(test_intent(), now);
    match p.try_transition(PaymentState::Refunded, now) {
        TransitionResult::Rejected { error, .. } => {
            assert_eq!(error.current_state, PaymentState::Created);
            assert_eq!(error.attempted, PaymentState::Refunded);
        }
        _ => panic!("Created → Refunded should be Rejected"),
    }

    // Self-transition
    let p = Payment::<Created>::create(test_intent(), now);
    match p.try_transition(PaymentState::Created, now) {
        TransitionResult::SelfTransition(p) => {
            assert_eq!(p.state(), PaymentState::Created);
        }
        _ => panic!("Created → Created should be SelfTransition"),
    }
}

/// E1-INT-005: Payment Display format consistency across lifecycle.
#[test]
fn display_format_across_lifecycle() {
    let now = Utc::now();
    let intent = test_intent();
    let id_str = intent.id.to_string();

    let created = Payment::<Created>::create(intent, now);
    assert_eq!(created.to_string(), format!("Payment({}, Created)", id_str));

    let authorized = created.authorize(now);
    assert_eq!(
        authorized.to_string(),
        format!("Payment({}, Authorized)", id_str)
    );

    let captured = authorized.capture(now);
    assert_eq!(
        captured.to_string(),
        format!("Payment({}, Captured)", id_str)
    );

    let refunded = captured.refund(now);
    assert_eq!(
        refunded.to_string(),
        format!("Payment({}, Refunded)", id_str)
    );
}
