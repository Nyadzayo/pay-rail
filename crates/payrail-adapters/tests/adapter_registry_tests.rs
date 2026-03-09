use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use chrono::Utc;

use payrail_adapters::{AdapterError, AdapterRegistry, PaymentAdapter, PaymentEvent};
use payrail_core::{
    CanonicalEvent, EventId, EventType, IdempotencyKey, Money, PaymentCommand, PaymentIntent,
    PaymentState, RawWebhook, ReceiverError, SignatureConfig, SignatureMethod, WebhookNormalizer,
};

/// Mock adapter for testing the registry, trait interface, and dual-trait
/// implementation (PaymentAdapter + WebhookNormalizer per AC 6).
struct MockAdapter {
    provider: String,
    sig_config: SignatureConfig,
}

impl MockAdapter {
    fn new(provider: &str) -> Self {
        Self {
            provider: provider.to_owned(),
            sig_config: SignatureConfig {
                method: SignatureMethod::HmacSha256,
                header_name: "X-Test-Signature".to_owned(),
                secret_env_var: "TEST_WEBHOOK_SECRET".to_owned(),
            },
        }
    }
}

impl PaymentAdapter for MockAdapter {
    fn execute(
        &self,
        command: PaymentCommand,
        _intent: &PaymentIntent,
    ) -> Pin<Box<dyn Future<Output = Result<PaymentEvent, AdapterError>> + Send + '_>> {
        let provider = self.provider.clone();
        Box::pin(async move {
            Err(AdapterError::UnsupportedCommand {
                provider,
                command: command.to_string(),
            })
        })
    }

    fn translate_webhook(&self, _raw: &RawWebhook) -> Result<CanonicalEvent, AdapterError> {
        Ok(CanonicalEvent {
            event_id: EventId::new(),
            event_type: EventType::new("payment.charge.authorized").unwrap(),
            payment_id: payrail_core::PaymentId::new(),
            provider: self.provider.clone(),
            timestamp: Utc::now(),
            state_before: PaymentState::Created,
            state_after: PaymentState::Authorized,
            amount: Money::new(10000, payrail_core::Currency::ZAR),
            idempotency_key: "test:merchant:webhook:evt_1".to_owned(),
            raw_provider_payload: serde_json::json!({}),
            metadata: serde_json::json!({}),
        })
    }

    fn signature_config(&self) -> &SignatureConfig {
        &self.sig_config
    }
}

/// MockAdapter also implements WebhookNormalizer (AC 6: dual trait).
impl WebhookNormalizer for MockAdapter {
    fn signature_config(&self) -> &SignatureConfig {
        &self.sig_config
    }

    fn extract_idempotency_key(&self, _raw: &RawWebhook) -> Result<IdempotencyKey, ReceiverError> {
        IdempotencyKey::from_webhook("test", "merchant", "evt_1")
            .map_err(|e| ReceiverError::NormalizationFailed(e.to_string()))
    }

    fn normalize(&self, raw: &RawWebhook) -> Result<CanonicalEvent, ReceiverError> {
        self.translate_webhook(raw)
            .map_err(|e| ReceiverError::NormalizationFailed(e.to_string()))
    }
}

// -- Registry tests --

#[test]
fn adapter_registry_register_and_get() {
    let mut registry = AdapterRegistry::new();
    registry.register("peach", Box::new(MockAdapter::new("peach")));

    let adapter = registry.get("peach");
    assert!(adapter.is_some());

    let config = adapter.unwrap().signature_config();
    assert_eq!(config.header_name, "X-Test-Signature");
}

#[test]
fn adapter_registry_get_unknown_returns_none() {
    let registry = AdapterRegistry::new();
    assert!(registry.get("nonexistent").is_none());
}

#[test]
fn adapter_registry_multiple_providers() {
    let mut registry = AdapterRegistry::new();
    registry.register("peach", Box::new(MockAdapter::new("peach")));
    registry.register("startbutton", Box::new(MockAdapter::new("startbutton")));

    assert!(registry.get("peach").is_some());
    assert!(registry.get("startbutton").is_some());
    assert!(registry.get("stripe").is_none());
}

#[test]
fn adapter_registry_providers_list() {
    let mut registry = AdapterRegistry::new();
    registry.register("peach", Box::new(MockAdapter::new("peach")));
    registry.register("startbutton", Box::new(MockAdapter::new("startbutton")));

    let mut providers = registry.providers();
    providers.sort();
    assert_eq!(providers, vec!["peach", "startbutton"]);
}

#[test]
fn adapter_registry_overwrite_provider() {
    let mut registry = AdapterRegistry::new();
    registry.register("peach", Box::new(MockAdapter::new("peach_v1")));
    registry.register("peach", Box::new(MockAdapter::new("peach_v2")));

    assert_eq!(registry.providers().len(), 1);
    assert!(registry.get("peach").is_some());
}

// -- Adapter trait tests --

#[tokio::test]
async fn mock_adapter_execute_returns_unsupported() {
    let adapter = MockAdapter::new("test_provider");
    let intent = PaymentIntent {
        id: payrail_core::PaymentId::new(),
        amount: Money::new(5000, payrail_core::Currency::ZAR),
        provider: "test_provider".to_owned(),
        metadata: serde_json::json!({}),
    };

    let result = adapter.execute(PaymentCommand::Authorize, &intent).await;
    assert_eq!(
        result.unwrap_err(),
        AdapterError::UnsupportedCommand {
            provider: "test_provider".to_owned(),
            command: "Authorize".to_owned(),
        }
    );
}

#[test]
fn mock_adapter_translate_webhook_returns_canonical_event() {
    let adapter = MockAdapter::new("peach");
    let raw = RawWebhook {
        headers: HashMap::new(),
        body: Vec::new(),
    };

    let event = adapter.translate_webhook(&raw).unwrap();
    assert_eq!(event.provider, "peach");
    assert_eq!(event.state_after, PaymentState::Authorized);
    // H2 fix: event_type now matches state_after semantically
    assert_eq!(event.event_type.as_str(), "payment.charge.authorized");
}

#[test]
fn payment_event_translates_canonical_types() {
    let event = PaymentEvent {
        provider: "peach_payments".to_owned(),
        provider_transaction_id: "txn_abc".to_owned(),
        state: PaymentState::Captured,
        amount: Money::new(25000, payrail_core::Currency::ZAR),
        raw_response: serde_json::json!({"result": {"code": "000.000.000"}}),
        timestamp: Utc::now(),
        metadata: serde_json::json!({"order_id": "ORD-1"}),
    };

    assert_eq!(event.amount.value, 25000);
    assert_eq!(event.amount.currency, payrail_core::Currency::ZAR);
    assert_eq!(event.state, PaymentState::Captured);
}

// -- M1 fix: Dual trait implementation test (AC 6) --

#[test]
fn adapter_implements_both_payment_adapter_and_webhook_normalizer() {
    let adapter = MockAdapter::new("peach");
    let raw = RawWebhook {
        headers: HashMap::new(),
        body: Vec::new(),
    };

    // Use as PaymentAdapter
    let adapter_event = adapter.translate_webhook(&raw).unwrap();

    // Use as WebhookNormalizer
    let normalizer: &dyn WebhookNormalizer = &adapter;
    let normalizer_event = normalizer.normalize(&raw).unwrap();

    // Both produce consistent results
    assert_eq!(adapter_event.provider, normalizer_event.provider);
    assert_eq!(adapter_event.state_after, normalizer_event.state_after);

    // Idempotency key extraction works
    let key = normalizer.extract_idempotency_key(&raw).unwrap();
    assert_eq!(key.as_ref(), "test:merchant:webhook:evt_1");

    // Signature configs match
    let adapter_sig = PaymentAdapter::signature_config(&adapter);
    let normalizer_sig = WebhookNormalizer::signature_config(normalizer);
    assert_eq!(adapter_sig.header_name, normalizer_sig.header_name);
    assert_eq!(adapter_sig.secret_env_var, normalizer_sig.secret_env_var);
}
