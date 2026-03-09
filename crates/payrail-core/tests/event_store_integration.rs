//! Integration tests for the SQLite event store.
//!
//! These tests use only the public API (root re-exports).

use chrono::{Duration, Utc};
use payrail_core::{
    CanonicalEvent, Currency, EventId, EventStore, EventStoreError, EventType, Money, PaymentId,
    PaymentState, SqliteEventStore,
};

fn make_event(
    payment_id: &PaymentId,
    state_before: PaymentState,
    state_after: PaymentState,
    provider: &str,
    timestamp: chrono::DateTime<Utc>,
) -> CanonicalEvent {
    CanonicalEvent {
        event_id: EventId::new(),
        event_type: EventType::new("payment.charge.captured").unwrap(),
        payment_id: payment_id.clone(),
        provider: provider.to_owned(),
        timestamp,
        state_before,
        state_after,
        amount: Money::new(15000, Currency::ZAR),
        idempotency_key: format!("test:m1:webhook:{}", EventId::new()),
        raw_provider_payload: serde_json::json!({"status": "ok"}),
        metadata: serde_json::json!({"order": "ORD-001"}),
    }
}

/// E2-INT-001: Full payment lifecycle audit trail.
#[tokio::test]
async fn full_payment_lifecycle_audit_trail() {
    let store = SqliteEventStore::new_in_memory().unwrap();
    let pid = PaymentId::new();
    let now = Utc::now();

    let events = vec![
        make_event(
            &pid,
            PaymentState::Created,
            PaymentState::Authorized,
            "peach",
            now,
        ),
        make_event(
            &pid,
            PaymentState::Authorized,
            PaymentState::Captured,
            "peach",
            now + Duration::seconds(1),
        ),
        make_event(
            &pid,
            PaymentState::Captured,
            PaymentState::Refunded,
            "peach",
            now + Duration::seconds(2),
        ),
    ];

    for e in &events {
        store.append(e).await.unwrap();
    }

    let trail = store.query_by_payment_id(&pid).await.unwrap();
    assert_eq!(trail.len(), 3);

    // Verify chronological ordering and state chain
    assert_eq!(trail[0].state_before, PaymentState::Created);
    assert_eq!(trail[0].state_after, PaymentState::Authorized);
    assert_eq!(trail[1].state_before, PaymentState::Authorized);
    assert_eq!(trail[1].state_after, PaymentState::Captured);
    assert_eq!(trail[2].state_before, PaymentState::Captured);
    assert_eq!(trail[2].state_after, PaymentState::Refunded);

    // Verify timestamps are ordered
    assert!(trail[0].timestamp <= trail[1].timestamp);
    assert!(trail[1].timestamp <= trail[2].timestamp);
}

/// E2-INT-002: Dual-track state views diverge when app and provider differ.
#[tokio::test]
async fn dual_track_divergence() {
    let store = SqliteEventStore::new_in_memory().unwrap();
    let pid = PaymentId::new();
    let now = Utc::now();

    // Provider confirms authorization
    let e1 = make_event(
        &pid,
        PaymentState::Created,
        PaymentState::Authorized,
        "peach",
        now,
    );
    // App optimistically records capture (not yet confirmed by provider)
    let e2 = make_event(
        &pid,
        PaymentState::Authorized,
        PaymentState::Captured,
        "app",
        now + Duration::seconds(1),
    );

    store.append(&e1).await.unwrap();
    store.append(&e2).await.unwrap();

    // Optimistic: latest event = Captured (app thinks it's captured)
    let optimistic = store.optimistic_state(&pid).await.unwrap();
    assert_eq!(optimistic, Some(PaymentState::Captured));

    // Reconciled: latest provider event = Authorized (provider only confirmed auth)
    let reconciled = store.reconciled_state(&pid).await.unwrap();
    assert_eq!(reconciled, Some(PaymentState::Authorized));
}

/// E2-INT-003: Events from different payments are isolated.
#[tokio::test]
async fn multiple_payments_isolated() {
    let store = SqliteEventStore::new_in_memory().unwrap();
    let pid1 = PaymentId::new();
    let pid2 = PaymentId::new();
    let now = Utc::now();

    let e1 = make_event(
        &pid1,
        PaymentState::Created,
        PaymentState::Authorized,
        "peach",
        now,
    );
    let e2 = make_event(
        &pid2,
        PaymentState::Created,
        PaymentState::Captured,
        "peach",
        now,
    );

    store.append(&e1).await.unwrap();
    store.append(&e2).await.unwrap();

    let trail1 = store.query_by_payment_id(&pid1).await.unwrap();
    let trail2 = store.query_by_payment_id(&pid2).await.unwrap();

    assert_eq!(trail1.len(), 1);
    assert_eq!(trail2.len(), 1);
    assert_eq!(trail1[0].state_after, PaymentState::Authorized);
    assert_eq!(trail2[0].state_after, PaymentState::Captured);
}

/// E2-INT-004: Duplicate event_id is rejected.
#[tokio::test]
async fn duplicate_event_id_rejected() {
    let store = SqliteEventStore::new_in_memory().unwrap();
    let pid = PaymentId::new();
    let now = Utc::now();

    let event = make_event(
        &pid,
        PaymentState::Created,
        PaymentState::Authorized,
        "peach",
        now,
    );

    // First append succeeds
    store.append(&event).await.unwrap();

    // Second append with same event_id should fail
    let result = store.append(&event).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), EventStoreError::DuplicateEvent,);
}

/// E2-INT-005: Duplicate idempotency_key is rejected.
#[tokio::test]
async fn duplicate_idempotency_key_rejected() {
    let store = SqliteEventStore::new_in_memory().unwrap();
    let pid = PaymentId::new();
    let now = Utc::now();

    let e1 = make_event(
        &pid,
        PaymentState::Created,
        PaymentState::Authorized,
        "peach",
        now,
    );

    // Create a second event with a different event_id but same idempotency_key
    let e2 = CanonicalEvent {
        event_id: EventId::new(),
        idempotency_key: e1.idempotency_key.clone(),
        event_type: EventType::new("payment.charge.captured").unwrap(),
        payment_id: pid.clone(),
        provider: "peach".to_owned(),
        timestamp: now + Duration::seconds(1),
        state_before: PaymentState::Authorized,
        state_after: PaymentState::Captured,
        amount: Money::new(15000, Currency::ZAR),
        raw_provider_payload: serde_json::json!({"status": "ok"}),
        metadata: serde_json::json!({"order": "ORD-002"}),
    };

    store.append(&e1).await.unwrap();

    let result = store.append(&e2).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), EventStoreError::DuplicateEvent,);
}

/// E2-INT-006: Query by event_id returns the correct event.
#[tokio::test]
async fn query_by_event_id_returns_correct_event() {
    let store = SqliteEventStore::new_in_memory().unwrap();
    let pid = PaymentId::new();
    let now = Utc::now();

    let event = make_event(
        &pid,
        PaymentState::Created,
        PaymentState::Authorized,
        "peach",
        now,
    );
    let target_id = event.event_id.clone();

    store.append(&event).await.unwrap();

    let found = store.query_by_event_id(&target_id).await.unwrap();
    assert!(found.is_some());

    let found = found.unwrap();
    assert_eq!(found.event_id, target_id);
    assert_eq!(found.payment_id, pid);
    assert_eq!(found.provider, "peach");
    assert_eq!(found.state_before, PaymentState::Created);
    assert_eq!(found.state_after, PaymentState::Authorized);
    assert_eq!(found.amount, Money::new(15000, Currency::ZAR));
    assert_eq!(found.idempotency_key, event.idempotency_key);
}

/// E2-INT-009: Query by payment_id with no events returns empty vec.
#[tokio::test]
async fn query_empty_payment_returns_empty_vec() {
    let store = SqliteEventStore::new_in_memory().unwrap();
    let pid = PaymentId::new();

    let trail = store.query_by_payment_id(&pid).await.unwrap();
    assert!(trail.is_empty());
}

/// E2-INT-010: Optimistic state with no events returns None.
#[tokio::test]
async fn optimistic_state_no_events_returns_none() {
    let store = SqliteEventStore::new_in_memory().unwrap();
    let pid = PaymentId::new();

    let state = store.optimistic_state(&pid).await.unwrap();
    assert_eq!(state, None);
}

/// E2-INT-011: Reconciled state with no events returns None.
#[tokio::test]
async fn reconciled_state_no_events_returns_none() {
    let store = SqliteEventStore::new_in_memory().unwrap();
    let pid = PaymentId::new();

    let state = store.reconciled_state(&pid).await.unwrap();
    assert_eq!(state, None);
}
