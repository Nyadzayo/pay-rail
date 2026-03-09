use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde_json::json;

use payrail_core::webhook::signature::compute_hmac_sha256;
use payrail_core::{
    CanonicalEvent, Currency, EventId, EventStore, EventType, IdempotencyKey, Money, PaymentId,
    PaymentState, RawWebhook, ReceiverError, SecretStore, SignatureConfig, SignatureError,
    SignatureMethod, SqliteEventStore, SqliteIdempotencyStore, WebhookNormalizer, WebhookOutcome,
    WebhookReceiver,
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

struct TestSecretStore(HashMap<String, String>);

impl TestSecretStore {
    fn new(pairs: &[(&str, &str)]) -> Self {
        Self(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }
}

impl SecretStore for TestSecretStore {
    fn get_secret(&self, key: &str) -> Result<String, SignatureError> {
        self.0
            .get(key)
            .cloned()
            .ok_or_else(|| SignatureError::SecretNotFound(key.to_owned()))
    }
}

struct TestNormalizer {
    config: SignatureConfig,
    event: CanonicalEvent,
}

impl TestNormalizer {
    fn new(secret_env: &str, header: &str, event: CanonicalEvent) -> Self {
        Self {
            config: SignatureConfig {
                method: SignatureMethod::HmacSha256,
                header_name: header.to_owned(),
                secret_env_var: secret_env.to_owned(),
            },
            event,
        }
    }
}

impl WebhookNormalizer for TestNormalizer {
    fn signature_config(&self) -> &SignatureConfig {
        &self.config
    }

    fn extract_idempotency_key(&self, _raw: &RawWebhook) -> Result<IdempotencyKey, ReceiverError> {
        Ok(IdempotencyKey::generate("test", "m1", "webhook", "evt_int_001").unwrap())
    }

    fn normalize(&self, _raw: &RawWebhook) -> Result<CanonicalEvent, ReceiverError> {
        Ok(self.event.clone())
    }
}

fn make_event() -> CanonicalEvent {
    CanonicalEvent {
        event_id: EventId::new(),
        event_type: EventType::new("payment.charge.captured").unwrap(),
        payment_id: PaymentId::new(),
        provider: "integration_provider".to_owned(),
        timestamp: Utc::now(),
        state_before: PaymentState::Authorized,
        state_after: PaymentState::Captured,
        amount: Money::new(25000, Currency::ZAR),
        idempotency_key: "test:m1:webhook:evt_int_001".to_owned(),
        raw_provider_payload: json!({"result": "000.100.110"}),
        metadata: json!({}),
    }
}

fn make_signed_raw(secret: &str, header: &str, body: &[u8]) -> RawWebhook {
    let sig = hex::encode(compute_hmac_sha256(secret.as_bytes(), body));
    let mut headers = HashMap::new();
    headers.insert(header.to_owned(), sig);
    RawWebhook {
        headers,
        body: body.to_vec(),
    }
}

/// Create a pair of event stores sharing the same SQLite file.
fn make_shared_event_stores() -> (SqliteEventStore, SqliteEventStore) {
    let dir = std::env::temp_dir().join(format!("payrail_test_{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("events.db");
    let store1 = SqliteEventStore::new(&path).unwrap();
    let store2 = SqliteEventStore::new(&path).unwrap();
    (store1, store2)
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

// 8.1 — Full webhook processing flow
#[tokio::test]
async fn full_webhook_processing_flow() {
    let secret = "integration_secret";
    let event = make_event();
    let payment_id = event.payment_id.clone();

    let (event_store, query_store) = make_shared_event_stores();
    let idem_store = SqliteIdempotencyStore::new_in_memory().unwrap();
    let normalizer = TestNormalizer::new("SEC", "X-Sig", event);
    let secret_store = TestSecretStore::new(&[("SEC", secret)]);
    let receiver = WebhookReceiver::new(event_store, idem_store, Duration::hours(72));

    let raw = make_signed_raw(secret, "X-Sig", b"integration test body");
    let result = receiver
        .handle(&raw, &normalizer, &secret_store)
        .await
        .unwrap();

    match result {
        WebhookOutcome::Processed { event } => {
            assert_eq!(event.provider, "integration_provider");
            assert_eq!(event.amount, Money::new(25000, Currency::ZAR));
        }
        other => panic!("expected Processed, got {other:?}"),
    }

    // Verify event appears in store via second handle
    let events = query_store.query_by_payment_id(&payment_id).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].provider, "integration_provider");
}

// 8.2 — Duplicate webhook detected end-to-end
#[tokio::test]
async fn duplicate_webhook_detected_end_to_end() {
    let secret = "dup_secret";
    let event = make_event();

    let event_store = SqliteEventStore::new_in_memory().unwrap();
    let idem_store = SqliteIdempotencyStore::new_in_memory().unwrap();
    let normalizer = TestNormalizer::new("SEC", "X-Sig", event);
    let secret_store = TestSecretStore::new(&[("SEC", secret)]);
    let receiver = WebhookReceiver::new(event_store, idem_store, Duration::hours(72));

    let raw = make_signed_raw(secret, "X-Sig", b"dup test body");

    // First call — should be Processed
    let result1 = receiver
        .handle(&raw, &normalizer, &secret_store)
        .await
        .unwrap();
    assert!(matches!(result1, WebhookOutcome::Processed { .. }));

    // Second call — same idempotency key → Duplicate
    let result2 = receiver
        .handle(&raw, &normalizer, &secret_store)
        .await
        .unwrap();
    match result2 {
        WebhookOutcome::Duplicate {
            idempotency_key, ..
        } => {
            assert_eq!(idempotency_key, "test:m1:webhook:evt_int_001");
        }
        other => panic!("expected Duplicate, got {other:?}"),
    }
}

// 8.3 — Event store contains correct canonical event
#[tokio::test]
async fn event_store_contains_correct_canonical_event() {
    let secret = "field_secret";
    let event = make_event();
    let expected_event_id = event.event_id.clone();
    let expected_type = event.event_type.clone();

    let (event_store, query_store) = make_shared_event_stores();
    let idem_store = SqliteIdempotencyStore::new_in_memory().unwrap();
    let normalizer = TestNormalizer::new("SEC", "X-Sig", event);
    let secret_store = TestSecretStore::new(&[("SEC", secret)]);
    let receiver = WebhookReceiver::new(event_store, idem_store, Duration::hours(72));

    let raw = make_signed_raw(secret, "X-Sig", b"field test body");
    receiver
        .handle(&raw, &normalizer, &secret_store)
        .await
        .unwrap();

    // Query by event ID and verify all fields
    let stored = query_store
        .query_by_event_id(&expected_event_id)
        .await
        .unwrap()
        .expect("event should exist");

    assert_eq!(stored.event_id, expected_event_id);
    assert_eq!(stored.event_type, expected_type);
    assert_eq!(stored.provider, "integration_provider");
    assert_eq!(stored.state_before, PaymentState::Authorized);
    assert_eq!(stored.state_after, PaymentState::Captured);
    assert_eq!(stored.amount, Money::new(25000, Currency::ZAR));
    assert_eq!(stored.idempotency_key, "test:m1:webhook:evt_int_001");
    assert_eq!(
        stored.raw_provider_payload,
        json!({"result": "000.100.110"})
    );
}
