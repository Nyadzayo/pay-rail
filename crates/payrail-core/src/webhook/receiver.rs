use chrono::Duration;

use crate::event::store::{EventStore, EventStoreError};
use crate::event::types::{CanonicalEvent, RawWebhook};
use crate::idempotency::key::IdempotencyKey;
use crate::idempotency::store::{IdempotencyError, IdempotencyStore};
use crate::webhook::signature::{SecretStore, SignatureError, verify_signature};

// ---------------------------------------------------------------------------
// ReceiverError (Task 1)
// ---------------------------------------------------------------------------

/// Errors from webhook receiver processing.
///
/// Each variant follows `[WHAT] [WHY] [WHAT TO DO]` format and never
/// includes signing keys or customer PII.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ReceiverError {
    /// Webhook signature verification failed — reject immediately.
    #[error("[SignatureFailure] {0}")]
    SignatureFailure(SignatureError),

    /// Idempotency store returned a non-transient error.
    #[error("[IdempotencyUnavailable] {0} [Defer and retry with exponential backoff]")]
    IdempotencyUnavailable(String),

    /// Provider-to-canonical normalization failed.
    #[error("[NormalizationFailed] {0} [Check normalizer implementation for this provider]")]
    NormalizationFailed(String),

    /// Event store append failed.
    #[error("[EventStoreFailed] {0} [Check event store connectivity and schema]")]
    EventStoreFailed(String),

    /// State transition was invalid (reserved for future use).
    #[error("[InvalidTransition] {0} [Verify event represents a valid state change]")]
    InvalidTransition(String),
}

impl From<SignatureError> for ReceiverError {
    fn from(err: SignatureError) -> Self {
        ReceiverError::SignatureFailure(err)
    }
}

impl From<EventStoreError> for ReceiverError {
    fn from(err: EventStoreError) -> Self {
        ReceiverError::EventStoreFailed(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// WebhookNormalizer trait (Task 2)
// ---------------------------------------------------------------------------

/// Abstracts provider-specific webhook handling.
///
/// Adapters implement this trait to declare their signature configuration,
/// extract idempotency keys, and normalize raw webhooks into canonical events.
/// Uses dynamic dispatch (`&dyn WebhookNormalizer`) for runtime provider selection.
pub trait WebhookNormalizer: Send + Sync {
    /// Returns the provider's signature verification configuration.
    fn signature_config(&self) -> &crate::webhook::signature::SignatureConfig;

    /// Extracts a deterministic idempotency key from the raw webhook data.
    fn extract_idempotency_key(&self, raw: &RawWebhook) -> Result<IdempotencyKey, ReceiverError>;

    /// Translates a provider-specific webhook payload into a canonical event.
    fn normalize(&self, raw: &RawWebhook) -> Result<CanonicalEvent, ReceiverError>;
}

// ---------------------------------------------------------------------------
// WebhookOutcome (Task 3)
// ---------------------------------------------------------------------------

/// Result of webhook hot-path processing.
///
/// Each variant carries enough context for the caller to construct structured
/// log entries and appropriate HTTP responses.
#[derive(Debug, Clone, PartialEq)]
pub enum WebhookOutcome {
    /// Hot path succeeded — event recorded to store.
    Processed {
        /// The canonical event that was persisted.
        event: CanonicalEvent,
    },
    /// Duplicate webhook detected via idempotency key.
    Duplicate {
        /// The idempotency key that matched.
        idempotency_key: String,
        /// The previously stored result for this key.
        stored_result: String,
    },
    /// Processing deferred — idempotency store unreachable (fail-closed).
    /// Caller should return HTTP 503 to trigger provider-side retry.
    Deferred {
        /// Why processing was deferred.
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// WebhookReceiver (Tasks 4-5)
// ---------------------------------------------------------------------------

/// Two-phase webhook handler orchestrating the hot path.
///
/// Generic over `EventStore` and `IdempotencyStore` using RPITIT (static
/// dispatch). Provider-specific normalization is via `&dyn WebhookNormalizer`.
pub struct WebhookReceiver<E: EventStore, I: IdempotencyStore> {
    event_store: E,
    idempotency_store: I,
    default_ttl: Duration,
}

impl<E: EventStore, I: IdempotencyStore> WebhookReceiver<E, I> {
    /// Creates a new webhook receiver with the given stores and TTL.
    pub fn new(event_store: E, idempotency_store: I, default_ttl: Duration) -> Self {
        Self {
            event_store,
            idempotency_store,
            default_ttl,
        }
    }

    /// Processes a raw webhook through the hot path.
    ///
    /// # Hot Path Sequence
    /// 1. Verify signature via `verify_signature()`
    /// 2. Extract idempotency key via normalizer
    /// 3. Check idempotency store (fail-closed on unavailable)
    /// 4. Normalize to `CanonicalEvent`
    /// 5. Record event to `EventStore`
    /// 6. Store idempotency key (best-effort)
    ///
    /// Returns `WebhookOutcome::Processed` on success, `Duplicate` if already
    /// seen, or `Deferred` if the idempotency store is unreachable.
    pub async fn handle(
        &self,
        raw: &RawWebhook,
        normalizer: &dyn WebhookNormalizer,
        secret_store: &dyn SecretStore,
    ) -> Result<WebhookOutcome, ReceiverError> {
        // Step 1: Verify signature — reject immediately on failure
        verify_signature(
            normalizer.signature_config(),
            &raw.headers,
            &raw.body,
            secret_store,
        )?;

        // Step 2: Extract idempotency key
        let key = normalizer.extract_idempotency_key(raw)?;

        // Step 3: Check idempotency store
        match self.idempotency_store.check(&key).await {
            Ok(Some(record)) => {
                return Ok(WebhookOutcome::Duplicate {
                    idempotency_key: record.key,
                    stored_result: record.result,
                });
            }
            Ok(None) => { /* new key — continue processing */ }
            Err(IdempotencyError::StoreUnavailable(reason)) => {
                return Ok(WebhookOutcome::Deferred { reason });
            }
            Err(other) => {
                return Err(ReceiverError::IdempotencyUnavailable(other.to_string()));
            }
        }

        // Step 4: Normalize to canonical event
        let event = normalizer.normalize(raw)?;

        // Step 5: Record event to event store
        self.event_store.append(&event).await?;

        // Step 6: Store idempotency key (best-effort — event already recorded)
        let _ = self
            .idempotency_store
            .store(&key, &event.event_id.to_string(), self.default_ttl)
            .await;

        Ok(WebhookOutcome::Processed { event })
    }
}

// ---------------------------------------------------------------------------
// Unit tests (Task 7)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use chrono::Utc;
    use serde_json::json;

    use crate::event::store::EventStoreError;
    use crate::event::types::EventType;
    use crate::id::{EventId, PaymentId};
    use crate::idempotency::store::{IdempotencyOutcome, IdempotencyRecord};
    use crate::payment::{Currency, Money, PaymentState};
    use crate::webhook::signature::{SignatureConfig, SignatureMethod, compute_hmac_sha256};

    // -----------------------------------------------------------------------
    // Mock SecretStore
    // -----------------------------------------------------------------------

    struct MockSecretStore {
        secrets: HashMap<String, String>,
    }

    impl MockSecretStore {
        fn new(pairs: &[(&str, &str)]) -> Self {
            Self {
                secrets: pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            }
        }
    }

    impl SecretStore for MockSecretStore {
        fn get_secret(&self, key: &str) -> Result<String, SignatureError> {
            self.secrets
                .get(key)
                .cloned()
                .ok_or_else(|| SignatureError::SecretNotFound(key.to_owned()))
        }
    }

    // -----------------------------------------------------------------------
    // Mock WebhookNormalizer
    // -----------------------------------------------------------------------

    struct MockNormalizer {
        config: SignatureConfig,
        key_result: Result<IdempotencyKey, ReceiverError>,
        normalize_result: Result<CanonicalEvent, ReceiverError>,
    }

    impl MockNormalizer {
        fn success(secret_env: &str, header: &str) -> (Self, String) {
            let secret = "test_secret";
            let config = SignatureConfig {
                method: SignatureMethod::HmacSha256,
                header_name: header.to_owned(),
                secret_env_var: secret_env.to_owned(),
            };
            let normalizer = Self {
                config,
                key_result: Ok(
                    IdempotencyKey::generate("test", "m1", "webhook", "evt_001").unwrap()
                ),
                normalize_result: Ok(make_test_event()),
            };
            (normalizer, secret.to_owned())
        }
    }

    impl WebhookNormalizer for MockNormalizer {
        fn signature_config(&self) -> &SignatureConfig {
            &self.config
        }

        fn extract_idempotency_key(
            &self,
            _raw: &RawWebhook,
        ) -> Result<IdempotencyKey, ReceiverError> {
            self.key_result.clone()
        }

        fn normalize(&self, _raw: &RawWebhook) -> Result<CanonicalEvent, ReceiverError> {
            self.normalize_result.clone()
        }
    }

    // -----------------------------------------------------------------------
    // Mock EventStore (RPITIT — concrete struct)
    // -----------------------------------------------------------------------

    #[derive(Clone)]
    struct MockEventStore {
        events: Arc<Mutex<Vec<CanonicalEvent>>>,
        should_fail: bool,
    }

    impl MockEventStore {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
                should_fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
                should_fail: true,
            }
        }

        fn stored_events(&self) -> Vec<CanonicalEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl EventStore for MockEventStore {
        async fn append(&self, event: &CanonicalEvent) -> Result<(), EventStoreError> {
            if self.should_fail {
                return Err(EventStoreError::Sqlite("mock failure".to_owned()));
            }
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }

        async fn query_by_payment_id(
            &self,
            _id: &PaymentId,
        ) -> Result<Vec<CanonicalEvent>, EventStoreError> {
            Ok(vec![])
        }

        async fn query_by_event_id(
            &self,
            _id: &EventId,
        ) -> Result<Option<CanonicalEvent>, EventStoreError> {
            Ok(None)
        }

        async fn optimistic_state(
            &self,
            _payment_id: &PaymentId,
        ) -> Result<Option<PaymentState>, EventStoreError> {
            Ok(None)
        }

        async fn reconciled_state(
            &self,
            _payment_id: &PaymentId,
        ) -> Result<Option<PaymentState>, EventStoreError> {
            Ok(None)
        }
    }

    // -----------------------------------------------------------------------
    // Mock IdempotencyStore (RPITIT — concrete struct)
    // -----------------------------------------------------------------------

    #[derive(Clone)]
    struct MockIdempotencyStore {
        records: Arc<Mutex<HashMap<String, IdempotencyRecord>>>,
        check_behavior: MockIdempotencyBehavior,
        store_should_fail: bool,
    }

    #[derive(Clone)]
    enum MockIdempotencyBehavior {
        Normal,
        StoreUnavailable,
        OtherError,
    }

    impl MockIdempotencyStore {
        fn new() -> Self {
            Self {
                records: Arc::new(Mutex::new(HashMap::new())),
                check_behavior: MockIdempotencyBehavior::Normal,
                store_should_fail: false,
            }
        }

        fn with_existing_record(key: &str, result: &str) -> Self {
            let mut records = HashMap::new();
            records.insert(
                key.to_owned(),
                IdempotencyRecord {
                    key: key.to_owned(),
                    created_at: Utc::now(),
                    expires_at: Utc::now() + Duration::hours(72),
                    result: result.to_owned(),
                },
            );
            Self {
                records: Arc::new(Mutex::new(records)),
                check_behavior: MockIdempotencyBehavior::Normal,
                store_should_fail: false,
            }
        }

        fn unavailable() -> Self {
            Self {
                records: Arc::new(Mutex::new(HashMap::new())),
                check_behavior: MockIdempotencyBehavior::StoreUnavailable,
                store_should_fail: false,
            }
        }

        fn with_store_failure() -> Self {
            Self {
                records: Arc::new(Mutex::new(HashMap::new())),
                check_behavior: MockIdempotencyBehavior::Normal,
                store_should_fail: true,
            }
        }

        fn with_other_error() -> Self {
            Self {
                records: Arc::new(Mutex::new(HashMap::new())),
                check_behavior: MockIdempotencyBehavior::OtherError,
                store_should_fail: false,
            }
        }
    }

    impl IdempotencyStore for MockIdempotencyStore {
        async fn check(
            &self,
            key: &IdempotencyKey,
        ) -> Result<Option<IdempotencyRecord>, IdempotencyError> {
            let key_str = key.to_string();
            match &self.check_behavior {
                MockIdempotencyBehavior::StoreUnavailable => Err(
                    IdempotencyError::StoreUnavailable("mock store unavailable".to_owned()),
                ),
                MockIdempotencyBehavior::OtherError => {
                    Err(IdempotencyError::Sqlite("mock other error".to_owned()))
                }
                MockIdempotencyBehavior::Normal => {
                    Ok(self.records.lock().unwrap().get(&key_str).cloned())
                }
            }
        }

        async fn store(
            &self,
            key: &IdempotencyKey,
            result: &str,
            ttl: Duration,
        ) -> Result<(), IdempotencyError> {
            if self.store_should_fail {
                return Err(IdempotencyError::StoreUnavailable(
                    "mock store write failure".to_owned(),
                ));
            }
            let key_str = key.to_string();
            let now = Utc::now();
            self.records.lock().unwrap().insert(
                key_str.clone(),
                IdempotencyRecord {
                    key: key_str,
                    created_at: now,
                    expires_at: now + ttl,
                    result: result.to_owned(),
                },
            );
            Ok(())
        }

        async fn check_and_store(
            &self,
            _key: &IdempotencyKey,
            _result: &str,
            _ttl: Duration,
        ) -> Result<IdempotencyOutcome, IdempotencyError> {
            Ok(IdempotencyOutcome::New)
        }

        async fn cleanup_expired(&self) -> Result<u64, IdempotencyError> {
            Ok(0)
        }
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_test_event() -> CanonicalEvent {
        CanonicalEvent {
            event_id: EventId::new(),
            event_type: EventType::new("payment.charge.captured").unwrap(),
            payment_id: PaymentId::new(),
            provider: "test_provider".to_owned(),
            timestamp: Utc::now(),
            state_before: PaymentState::Authorized,
            state_after: PaymentState::Captured,
            amount: Money::new(15000, Currency::ZAR),
            idempotency_key: "test:m1:webhook:evt_001".to_owned(),
            raw_provider_payload: json!({"status": "captured"}),
            metadata: json!({}),
        }
    }

    fn make_signed_webhook(secret: &str, header_name: &str, body: &[u8]) -> RawWebhook {
        let sig = hex::encode(compute_hmac_sha256(secret.as_bytes(), body));
        let mut headers = HashMap::new();
        headers.insert(header_name.to_owned(), sig);
        RawWebhook {
            headers,
            body: body.to_vec(),
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    // 7.3
    #[tokio::test]
    async fn handle_valid_webhook_returns_processed() {
        let (normalizer, secret) = MockNormalizer::success("SECRET", "X-Sig");
        let secret_store = MockSecretStore::new(&[("SECRET", &secret)]);
        let event_store = MockEventStore::new();
        let idem_store = MockIdempotencyStore::new();
        let receiver = WebhookReceiver::new(event_store.clone(), idem_store, Duration::hours(72));

        let raw = make_signed_webhook(&secret, "X-Sig", b"test body");
        let result = receiver.handle(&raw, &normalizer, &secret_store).await;

        assert!(result.is_ok());
        match result.unwrap() {
            WebhookOutcome::Processed { event } => {
                assert_eq!(event.provider, "test_provider");
            }
            other => panic!("expected Processed, got {other:?}"),
        }
        assert_eq!(event_store.stored_events().len(), 1);
    }

    // 7.4
    #[tokio::test]
    async fn handle_invalid_signature_returns_error() {
        let (normalizer, _secret) = MockNormalizer::success("SECRET", "X-Sig");
        let secret_store = MockSecretStore::new(&[("SECRET", "real_secret")]);
        let event_store = MockEventStore::new();
        let idem_store = MockIdempotencyStore::new();
        let receiver = WebhookReceiver::new(event_store.clone(), idem_store, Duration::hours(72));

        // Body signed with wrong key
        let raw = make_signed_webhook("wrong_key", "X-Sig", b"test body");
        let result = receiver.handle(&raw, &normalizer, &secret_store).await;

        assert!(matches!(result, Err(ReceiverError::SignatureFailure(_))));
        assert!(event_store.stored_events().is_empty());
    }

    // 7.5
    #[tokio::test]
    async fn handle_missing_signature_header_returns_error() {
        let (normalizer, secret) = MockNormalizer::success("SECRET", "X-Sig");
        let secret_store = MockSecretStore::new(&[("SECRET", &secret)]);
        let event_store = MockEventStore::new();
        let idem_store = MockIdempotencyStore::new();
        let receiver = WebhookReceiver::new(event_store, idem_store, Duration::hours(72));

        // No signature header at all
        let raw = RawWebhook {
            headers: HashMap::new(),
            body: b"test body".to_vec(),
        };
        let result = receiver.handle(&raw, &normalizer, &secret_store).await;

        assert!(matches!(result, Err(ReceiverError::SignatureFailure(_))));
    }

    // 7.6
    #[tokio::test]
    async fn handle_duplicate_webhook_returns_duplicate() {
        let (normalizer, secret) = MockNormalizer::success("SECRET", "X-Sig");
        let secret_store = MockSecretStore::new(&[("SECRET", &secret)]);
        let event_store = MockEventStore::new();
        let idem_store =
            MockIdempotencyStore::with_existing_record("test:m1:webhook:evt_001", r#"{"ok":true}"#);
        let receiver = WebhookReceiver::new(event_store.clone(), idem_store, Duration::hours(72));

        let raw = make_signed_webhook(&secret, "X-Sig", b"test body");
        let result = receiver.handle(&raw, &normalizer, &secret_store).await;

        match result.unwrap() {
            WebhookOutcome::Duplicate {
                idempotency_key,
                stored_result,
            } => {
                assert_eq!(idempotency_key, "test:m1:webhook:evt_001");
                assert_eq!(stored_result, r#"{"ok":true}"#);
            }
            other => panic!("expected Duplicate, got {other:?}"),
        }
        assert!(event_store.stored_events().is_empty());
    }

    // 7.7
    #[tokio::test]
    async fn handle_idempotency_store_unavailable_returns_deferred() {
        let (normalizer, secret) = MockNormalizer::success("SECRET", "X-Sig");
        let secret_store = MockSecretStore::new(&[("SECRET", &secret)]);
        let event_store = MockEventStore::new();
        let idem_store = MockIdempotencyStore::unavailable();
        let receiver = WebhookReceiver::new(event_store.clone(), idem_store, Duration::hours(72));

        let raw = make_signed_webhook(&secret, "X-Sig", b"test body");
        let result = receiver.handle(&raw, &normalizer, &secret_store).await;

        match result.unwrap() {
            WebhookOutcome::Deferred { reason } => {
                assert!(reason.contains("unavailable"));
            }
            other => panic!("expected Deferred, got {other:?}"),
        }
        assert!(event_store.stored_events().is_empty());
    }

    // 7.7b — non-StoreUnavailable idempotency error propagates as IdempotencyUnavailable
    #[tokio::test]
    async fn handle_idempotency_other_error_returns_error() {
        let (normalizer, secret) = MockNormalizer::success("SECRET", "X-Sig");
        let secret_store = MockSecretStore::new(&[("SECRET", &secret)]);
        let event_store = MockEventStore::new();
        let idem_store = MockIdempotencyStore::with_other_error();
        let receiver = WebhookReceiver::new(event_store.clone(), idem_store, Duration::hours(72));

        let raw = make_signed_webhook(&secret, "X-Sig", b"test body");
        let result = receiver.handle(&raw, &normalizer, &secret_store).await;

        assert!(matches!(
            result,
            Err(ReceiverError::IdempotencyUnavailable(_))
        ));
        assert!(event_store.stored_events().is_empty());
    }

    // 7.8
    #[tokio::test]
    async fn handle_normalization_failure_returns_error() {
        let secret = "test_secret";
        let normalizer = MockNormalizer {
            config: SignatureConfig {
                method: SignatureMethod::HmacSha256,
                header_name: "X-Sig".to_owned(),
                secret_env_var: "SECRET".to_owned(),
            },
            key_result: Ok(IdempotencyKey::generate("test", "m1", "webhook", "evt_001").unwrap()),
            normalize_result: Err(ReceiverError::NormalizationFailed(
                "unknown event type".to_owned(),
            )),
        };
        let secret_store = MockSecretStore::new(&[("SECRET", secret)]);
        let event_store = MockEventStore::new();
        let idem_store = MockIdempotencyStore::new();
        let receiver = WebhookReceiver::new(event_store, idem_store, Duration::hours(72));

        let raw = make_signed_webhook(secret, "X-Sig", b"test body");
        let result = receiver.handle(&raw, &normalizer, &secret_store).await;

        assert!(matches!(result, Err(ReceiverError::NormalizationFailed(_))));
    }

    // 7.9
    #[tokio::test]
    async fn handle_event_store_failure_returns_error() {
        let (normalizer, secret) = MockNormalizer::success("SECRET", "X-Sig");
        let secret_store = MockSecretStore::new(&[("SECRET", &secret)]);
        let event_store = MockEventStore::failing();
        let idem_store = MockIdempotencyStore::new();
        let receiver = WebhookReceiver::new(event_store, idem_store, Duration::hours(72));

        let raw = make_signed_webhook(&secret, "X-Sig", b"test body");
        let result = receiver.handle(&raw, &normalizer, &secret_store).await;

        assert!(matches!(result, Err(ReceiverError::EventStoreFailed(_))));
    }

    // 7.10
    #[tokio::test]
    async fn handle_idempotency_store_write_failure_still_returns_processed() {
        let (normalizer, secret) = MockNormalizer::success("SECRET", "X-Sig");
        let secret_store = MockSecretStore::new(&[("SECRET", &secret)]);
        let event_store = MockEventStore::new();
        let idem_store = MockIdempotencyStore::with_store_failure();
        let receiver = WebhookReceiver::new(event_store.clone(), idem_store, Duration::hours(72));

        let raw = make_signed_webhook(&secret, "X-Sig", b"test body");
        let result = receiver.handle(&raw, &normalizer, &secret_store).await;

        // Event was recorded, idempotency write failed silently
        assert!(matches!(result, Ok(WebhookOutcome::Processed { .. })));
        assert_eq!(event_store.stored_events().len(), 1);
    }

    // 7.11
    #[test]
    fn receiver_error_from_signature_error_conversion() {
        let sig_err = SignatureError::MissingHeader("X-Sig".to_owned());
        let recv_err: ReceiverError = sig_err.clone().into();
        assert_eq!(recv_err, ReceiverError::SignatureFailure(sig_err));
    }

    // 7.12
    #[test]
    fn webhook_outcome_variants_carry_expected_fields() {
        let event = make_test_event();
        let processed = WebhookOutcome::Processed {
            event: event.clone(),
        };
        let duplicate = WebhookOutcome::Duplicate {
            idempotency_key: "k".to_owned(),
            stored_result: "r".to_owned(),
        };
        let deferred = WebhookOutcome::Deferred {
            reason: "unavailable".to_owned(),
        };

        // Verify PartialEq works and fields are accessible
        assert_eq!(
            processed,
            WebhookOutcome::Processed {
                event: event.clone()
            }
        );
        assert_ne!(processed, duplicate);
        assert_ne!(duplicate, deferred);
    }

    // E2-UNIT-002: every ReceiverError variant's Display starts with '['
    #[test]
    fn receiver_error_display_includes_brackets() {
        let variants: Vec<ReceiverError> = vec![
            ReceiverError::SignatureFailure(SignatureError::InvalidSignature("X-Sig".to_owned())),
            ReceiverError::IdempotencyUnavailable("store down".to_owned()),
            ReceiverError::NormalizationFailed("bad payload".to_owned()),
            ReceiverError::EventStoreFailed("write error".to_owned()),
            ReceiverError::InvalidTransition("invalid state".to_owned()),
        ];
        for variant in &variants {
            let msg = variant.to_string();
            assert!(
                msg.starts_with('['),
                "ReceiverError variant {variant:?} display does not start with '[': {msg}"
            );
        }
    }
}
