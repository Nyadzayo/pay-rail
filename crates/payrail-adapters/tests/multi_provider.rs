//! Multi-provider end-to-end validation tests (Story 6.3).
//!
//! Proves that PayRail scales to multiple providers in a single deployment
//! without interference, shared mutable state, or architecture changes.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use chrono::Utc;

use payrail_adapters::{AdapterError, AdapterRegistry, PaymentAdapter, PaymentEvent};
use payrail_core::{
    CanonicalEvent, Currency, EventId, EventType, IdempotencyKey, Money, PaymentCommand, PaymentId,
    PaymentIntent, PaymentState, RawWebhook, ReceiverError, SignatureConfig, SignatureMethod,
    WebhookNormalizer,
};

// ---------------------------------------------------------------------------
// Provider-specific mock adapters
// ---------------------------------------------------------------------------

/// Mock adapter returning provider-specific responses for routing validation.
struct ProviderMock {
    provider: String,
    sig_config: SignatureConfig,
    state_after: PaymentState,
    amount_cents: i64,
}

impl ProviderMock {
    fn peach() -> Self {
        Self {
            provider: "peach_payments".to_owned(),
            sig_config: SignatureConfig {
                method: SignatureMethod::HmacSha256,
                header_name: "X-Peach-Signature".to_owned(),
                secret_env_var: "PEACH_SANDBOX_WEBHOOK_SECRET".to_owned(),
            },
            state_after: PaymentState::Authorized,
            amount_cents: 15000,
        }
    }

    fn startbutton() -> Self {
        Self {
            provider: "startbutton".to_owned(),
            sig_config: SignatureConfig {
                method: SignatureMethod::HmacSha256,
                header_name: "X-Startbutton-Signature".to_owned(),
                secret_env_var: "STARTBUTTON_SANDBOX_WEBHOOK_SECRET".to_owned(),
            },
            state_after: PaymentState::Captured,
            amount_cents: 9900,
        }
    }
}

impl PaymentAdapter for ProviderMock {
    fn execute(
        &self,
        _command: PaymentCommand,
        intent: &PaymentIntent,
    ) -> Pin<Box<dyn Future<Output = Result<PaymentEvent, AdapterError>> + Send + '_>> {
        let provider = self.provider.clone();
        let state = self.state_after;
        let amount = intent.amount.clone();
        let ts = Utc::now();
        Box::pin(async move {
            Ok(PaymentEvent {
                provider,
                provider_transaction_id: format!("txn_{}", PaymentId::new()),
                state,
                amount,
                raw_response: serde_json::json!({"mock": true}),
                timestamp: ts,
                metadata: serde_json::json!({}),
            })
        })
    }

    fn translate_webhook(&self, _raw: &RawWebhook) -> Result<CanonicalEvent, AdapterError> {
        // Event type matches state_after for semantic consistency
        let event_type_str = match self.state_after {
            PaymentState::Authorized => "payment.charge.authorized",
            PaymentState::Captured => "payment.charge.captured",
            PaymentState::Failed => "payment.charge.failed",
            _ => "payment.charge.captured",
        };
        Ok(CanonicalEvent {
            event_id: EventId::new(),
            event_type: EventType::new(event_type_str).unwrap(),
            payment_id: PaymentId::new(),
            provider: self.provider.clone(),
            timestamp: Utc::now(),
            state_before: PaymentState::Created,
            state_after: self.state_after,
            amount: Money::new(self.amount_cents, Currency::ZAR),
            idempotency_key: format!("{}:merchant:webhook:evt_1", self.provider),
            raw_provider_payload: serde_json::json!({}),
            metadata: serde_json::json!({}),
        })
    }

    fn signature_config(&self) -> &SignatureConfig {
        &self.sig_config
    }
}

impl WebhookNormalizer for ProviderMock {
    fn signature_config(&self) -> &SignatureConfig {
        &self.sig_config
    }

    fn extract_idempotency_key(&self, _raw: &RawWebhook) -> Result<IdempotencyKey, ReceiverError> {
        IdempotencyKey::from_webhook(&self.provider, "merchant", "evt_1")
            .map_err(|e| ReceiverError::NormalizationFailed(e.to_string()))
    }

    fn normalize(&self, raw: &RawWebhook) -> Result<CanonicalEvent, ReceiverError> {
        self.translate_webhook(raw)
            .map_err(|e| ReceiverError::NormalizationFailed(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Task 1: Multi-provider adapter registry and routing
// ---------------------------------------------------------------------------

#[test]
fn registry_routes_to_correct_adapter_by_provider() {
    let mut registry = AdapterRegistry::new();
    registry.register("peach", Box::new(ProviderMock::peach()));
    registry.register("startbutton", Box::new(ProviderMock::startbutton()));

    // Peach adapter returns its signature header
    let peach = registry.get("peach").expect("peach registered");
    assert_eq!(peach.signature_config().header_name, "X-Peach-Signature");

    // Startbutton adapter returns its signature header
    let sb = registry.get("startbutton").expect("startbutton registered");
    assert_eq!(sb.signature_config().header_name, "X-Startbutton-Signature");
}

#[tokio::test]
async fn concurrent_execute_no_interference() {
    let mut registry = AdapterRegistry::new();
    registry.register("peach", Box::new(ProviderMock::peach()));
    registry.register("startbutton", Box::new(ProviderMock::startbutton()));

    let peach_intent = PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(15000, Currency::ZAR),
        provider: "peach_payments".to_owned(),
        metadata: serde_json::json!({}),
    };

    let sb_intent = PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(9900, Currency::USD),
        provider: "startbutton".to_owned(),
        metadata: serde_json::json!({}),
    };

    // Execute both adapters CONCURRENTLY via tokio::join! — proves no shared mutable state
    let peach_adapter = registry.get("peach").unwrap();
    let sb_adapter = registry.get("startbutton").unwrap();

    let (peach_result, sb_result) = tokio::join!(
        peach_adapter.execute(PaymentCommand::Authorize, &peach_intent),
        sb_adapter.execute(PaymentCommand::Capture, &sb_intent),
    );

    let peach_result = peach_result.unwrap();
    let sb_result = sb_result.unwrap();

    // Results are provider-specific — no cross-contamination
    assert_eq!(peach_result.provider, "peach_payments");
    assert_eq!(peach_result.state, PaymentState::Authorized);
    assert_eq!(peach_result.amount.value, 15000);
    assert_eq!(peach_result.amount.currency, Currency::ZAR);

    assert_eq!(sb_result.provider, "startbutton");
    assert_eq!(sb_result.state, PaymentState::Captured);
    assert_eq!(sb_result.amount.value, 9900);
    assert_eq!(sb_result.amount.currency, Currency::USD);
}

#[test]
fn routing_correctness_wrong_provider_returns_none() {
    let mut registry = AdapterRegistry::new();
    registry.register("peach", Box::new(ProviderMock::peach()));
    registry.register("startbutton", Box::new(ProviderMock::startbutton()));

    assert!(registry.get("stripe").is_none());
    assert!(registry.get("").is_none());
    assert!(registry.get("PEACH").is_none()); // case-sensitive
}

// ---------------------------------------------------------------------------
// Task 2: Provider-namespaced webhook routing
// ---------------------------------------------------------------------------

#[test]
fn webhook_routes_to_correct_adapter_translate() {
    let mut registry = AdapterRegistry::new();
    registry.register("peach", Box::new(ProviderMock::peach()));
    registry.register("startbutton", Box::new(ProviderMock::startbutton()));

    let raw = RawWebhook {
        headers: HashMap::new(),
        body: Vec::new(),
    };

    // Peach webhook translation
    let peach = registry.get("peach").unwrap();
    let peach_event = peach.translate_webhook(&raw).unwrap();
    assert_eq!(peach_event.provider, "peach_payments");
    assert_eq!(peach_event.state_after, PaymentState::Authorized);

    // Startbutton webhook translation
    let sb = registry.get("startbutton").unwrap();
    let sb_event = sb.translate_webhook(&raw).unwrap();
    assert_eq!(sb_event.provider, "startbutton");
    assert_eq!(sb_event.state_after, PaymentState::Captured);
}

#[test]
fn idempotency_keys_namespaced_by_provider() {
    let peach_key = IdempotencyKey::from_webhook("peach", "m001", "evt_123").unwrap();
    let sb_key = IdempotencyKey::from_webhook("startbutton", "m001", "evt_123").unwrap();

    // Same event ID, same merchant — different providers = different keys
    assert_ne!(peach_key, sb_key);
    assert_eq!(peach_key.as_ref(), "peach:m001:webhook:evt_123");
    assert_eq!(sb_key.as_ref(), "startbutton:m001:webhook:evt_123");
}

#[test]
fn idempotency_keys_same_provider_same_event_are_equal() {
    let k1 = IdempotencyKey::from_webhook("peach", "m001", "evt_123").unwrap();
    let k2 = IdempotencyKey::from_webhook("peach", "m001", "evt_123").unwrap();
    assert_eq!(k1, k2);
}

#[test]
fn event_store_provider_attribution() {
    let raw = RawWebhook {
        headers: HashMap::new(),
        body: Vec::new(),
    };

    let peach = ProviderMock::peach();
    let sb = ProviderMock::startbutton();

    let peach_event = peach.translate_webhook(&raw).unwrap();
    let sb_event = sb.translate_webhook(&raw).unwrap();

    // Provider attribution is preserved on every event
    assert_eq!(peach_event.provider, "peach_payments");
    assert_eq!(sb_event.provider, "startbutton");

    // Idempotency keys include provider prefix
    assert!(peach_event.idempotency_key.starts_with("peach_payments:"));
    assert!(sb_event.idempotency_key.starts_with("startbutton:"));
}

#[test]
fn webhook_normalizer_routes_correctly() {
    let peach: &dyn WebhookNormalizer = &ProviderMock::peach();
    let sb: &dyn WebhookNormalizer = &ProviderMock::startbutton();

    let raw = RawWebhook {
        headers: HashMap::new(),
        body: Vec::new(),
    };

    // Each normalizer produces provider-attributed events
    let peach_event = peach.normalize(&raw).unwrap();
    let sb_event = sb.normalize(&raw).unwrap();

    assert_eq!(peach_event.provider, "peach_payments");
    assert_eq!(sb_event.provider, "startbutton");

    // Idempotency keys are provider-scoped
    let peach_key = peach.extract_idempotency_key(&raw).unwrap();
    let sb_key = sb.extract_idempotency_key(&raw).unwrap();
    assert_ne!(peach_key, sb_key);
    assert!(peach_key.as_ref().starts_with("peach_payments:"));
    assert!(sb_key.as_ref().starts_with("startbutton:"));

    // Signature configs are provider-specific
    assert_eq!(peach.signature_config().header_name, "X-Peach-Signature");
    assert_eq!(sb.signature_config().header_name, "X-Startbutton-Signature");
}

// ---------------------------------------------------------------------------
// Task 2.4: Real adapter webhook translation produces correct provider attribution
// ---------------------------------------------------------------------------

#[test]
fn real_adapters_produce_provider_attributed_events() {
    use payrail_adapters::{AdapterConfig, PeachPaymentsAdapter, StartbuttonAdapter};
    use std::time::Duration;

    let peach = PeachPaymentsAdapter::new(AdapterConfig {
        provider_name: "peach_payments".to_owned(),
        sandbox: true,
        base_url: "https://testsecure.peachpayments.com".to_owned(),
        api_key_env_var: "PEACH_SANDBOX_API_KEY".to_owned(),
        webhook_secret_env_var: "PEACH_SANDBOX_WEBHOOK_SECRET".to_owned(),
        entity_id_env_var: "PEACH_SANDBOX_ENTITY_ID".to_owned(),
        timeout: Duration::from_secs(30),
    });

    let sb = StartbuttonAdapter::new(AdapterConfig {
        provider_name: "startbutton".to_owned(),
        sandbox: true,
        base_url: "https://sandbox.startbutton.co/v1".to_owned(),
        api_key_env_var: "STARTBUTTON_SANDBOX_API_KEY".to_owned(),
        webhook_secret_env_var: "STARTBUTTON_SANDBOX_WEBHOOK_SECRET".to_owned(),
        entity_id_env_var: "STARTBUTTON_SANDBOX_ENTITY_ID".to_owned(),
        timeout: Duration::from_secs(30),
    });

    let peach_payment_id = PaymentId::new();
    let sb_payment_id = PaymentId::new();

    // Peach webhook: charge.succeeded (matches PeachWebhookPayload schema)
    let peach_raw = RawWebhook {
        headers: HashMap::from([("content-type".to_owned(), "application/json".to_owned())]),
        body: serde_json::to_vec(&serde_json::json!({
            "id": "evt_peach_001",
            "type": "charge.succeeded",
            "payload": {
                "id": "txn_peach_001",
                "result": {"code": "000.000.000", "description": "Success"},
                "amount": "150.00",
                "currency": "ZAR",
                "merchantTransactionId": peach_payment_id.to_string()
            }
        }))
        .unwrap(),
    };

    // Startbutton webhook: payment.authorized (matches StartbuttonWebhookPayload schema)
    let sb_raw = RawWebhook {
        headers: HashMap::from([("content-type".to_owned(), "application/json".to_owned())]),
        body: serde_json::to_vec(&serde_json::json!({
            "event": "payment.authorized",
            "data": {
                "id": "txn_sb_001",
                "status": "AUTHORIZED",
                "amount": 9900,
                "currency": "USD",
                "merchant_reference": sb_payment_id.to_string()
            }
        }))
        .unwrap(),
    };

    let peach_event = peach.translate_webhook(&peach_raw).unwrap();
    let sb_event = sb.translate_webhook(&sb_raw).unwrap();

    // Provider attribution is correct on events from real adapters
    assert_eq!(peach_event.provider, "peach_payments");
    assert_eq!(sb_event.provider, "startbutton");

    // States mapped correctly by real adapter logic
    // Peach: charge.succeeded + 000.000.000 = Authorized (pre-auth success)
    assert_eq!(peach_event.state_after, PaymentState::Authorized);
    // Startbutton: payment.authorized + AUTHORIZED = Authorized
    assert_eq!(sb_event.state_after, PaymentState::Authorized);

    // Idempotency keys are provider-namespaced (short prefix: "peach", "startbutton")
    assert!(peach_event.idempotency_key.starts_with("peach:"));
    assert!(sb_event.idempotency_key.starts_with("startbutton:"));
    assert_ne!(peach_event.idempotency_key, sb_event.idempotency_key);
}

// ---------------------------------------------------------------------------
// Task 1.3: Register real PeachPaymentsAdapter and StartbuttonAdapter
// ---------------------------------------------------------------------------

#[test]
fn registry_with_real_adapters() {
    use payrail_adapters::{AdapterConfig, PeachPaymentsAdapter, StartbuttonAdapter};
    use std::time::Duration;

    let peach = PeachPaymentsAdapter::new(AdapterConfig {
        provider_name: "peach_payments".to_owned(),
        sandbox: true,
        base_url: "https://testsecure.peachpayments.com".to_owned(),
        api_key_env_var: "PEACH_SANDBOX_API_KEY".to_owned(),
        webhook_secret_env_var: "PEACH_SANDBOX_WEBHOOK_SECRET".to_owned(),
        entity_id_env_var: "PEACH_SANDBOX_ENTITY_ID".to_owned(),
        timeout: Duration::from_secs(30),
    });

    let sb = StartbuttonAdapter::new(AdapterConfig {
        provider_name: "startbutton".to_owned(),
        sandbox: true,
        base_url: "https://sandbox.startbutton.co/v1".to_owned(),
        api_key_env_var: "STARTBUTTON_SANDBOX_API_KEY".to_owned(),
        webhook_secret_env_var: "STARTBUTTON_SANDBOX_WEBHOOK_SECRET".to_owned(),
        entity_id_env_var: "STARTBUTTON_SANDBOX_ENTITY_ID".to_owned(),
        timeout: Duration::from_secs(30),
    });

    let mut registry = AdapterRegistry::new();
    registry.register("peach", Box::new(peach));
    registry.register("startbutton", Box::new(sb));

    // Both real adapters registered and routable
    assert_eq!(registry.providers().len(), 2);
    let peach_adapter = registry.get("peach").unwrap();
    let sb_adapter = registry.get("startbutton").unwrap();

    // Signature configs are provider-specific (real values, not mocks)
    assert_eq!(
        peach_adapter.signature_config().header_name,
        "X-Peach-Signature"
    );
    assert_eq!(
        sb_adapter.signature_config().header_name,
        "X-Startbutton-Signature"
    );

    // Different secret env vars
    assert_ne!(
        peach_adapter.signature_config().secret_env_var,
        sb_adapter.signature_config().secret_env_var,
    );
}

// ---------------------------------------------------------------------------
// Task 5: Zero core architecture changes validation
// ---------------------------------------------------------------------------

#[test]
fn adding_provider_requires_no_core_changes() {
    // This test validates the architectural claim:
    // Adding a second provider is adapter + knowledge pack, NOT an architecture change.
    //
    // Evidence:
    // 1. AdapterRegistry accepts any Box<dyn PaymentAdapter> — no provider-specific code in core
    // 2. WebhookNormalizer dispatch is runtime — core doesn't know about specific providers
    // 3. IdempotencyKey format is generic: {provider}:{merchant}:{scope}:{id}
    // 4. CanonicalEvent.provider is a String, not an enum — open for extension
    // 5. PaymentState, PaymentCommand, Money, Currency — all provider-agnostic

    let mut registry = AdapterRegistry::new();

    // Register a hypothetical third provider — zero core changes needed
    registry.register(
        "stripe",
        Box::new(ProviderMock {
            provider: "stripe".to_owned(),
            sig_config: SignatureConfig {
                method: SignatureMethod::HmacSha256,
                header_name: "Stripe-Signature".to_owned(),
                secret_env_var: "STRIPE_WEBHOOK_SECRET".to_owned(),
            },
            state_after: PaymentState::Captured,
            amount_cents: 5000,
        }),
    );

    assert_eq!(registry.providers().len(), 1);
    let stripe = registry.get("stripe").unwrap();
    assert_eq!(stripe.signature_config().header_name, "Stripe-Signature");

    // Idempotency key works for the new provider
    let key = IdempotencyKey::from_webhook("stripe", "m1", "evt_1").unwrap();
    assert_eq!(key.as_ref(), "stripe:m1:webhook:evt_1");
}
