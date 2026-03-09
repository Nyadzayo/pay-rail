use std::collections::HashMap;
use std::time::Duration;

use payrail_adapters::{AdapterConfig, AdapterError, PaymentAdapter, PeachPaymentsAdapter};
use payrail_core::{
    Currency, Money, PaymentCommand, PaymentId, PaymentIntent, PaymentState, RawWebhook,
    SecretStore, SignatureError, WebhookNormalizer,
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn test_config() -> AdapterConfig {
    AdapterConfig {
        provider_name: "peach_payments".to_owned(),
        sandbox: true,
        base_url: "https://testsecure.peachpayments.com".to_owned(),
        api_key_env_var: "PEACH_SANDBOX_API_KEY".to_owned(),
        webhook_secret_env_var: "PEACH_SANDBOX_WEBHOOK_SECRET".to_owned(),
        entity_id_env_var: "PEACH_SANDBOX_ENTITY_ID".to_owned(),
        timeout: Duration::from_secs(30),
    }
}

fn test_adapter() -> PeachPaymentsAdapter {
    PeachPaymentsAdapter::new(test_config())
}

/// Creates a RawWebhook with the given JSON body and no headers.
fn raw_webhook(json: &serde_json::Value) -> RawWebhook {
    RawWebhook {
        headers: HashMap::new(),
        body: serde_json::to_vec(json).unwrap(),
    }
}

/// Returns a valid PaymentId string for use in test payloads.
fn test_payment_id() -> PaymentId {
    PaymentId::new()
}

/// Builds a Peach webhook JSON payload with the given parameters.
fn peach_webhook_json(
    event_id: &str,
    event_type: &str,
    result_code: &str,
    amount: &str,
    currency: &str,
    merchant_txn_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": event_id,
        "type": event_type,
        "payload": {
            "id": "txn_peach_001",
            "result": {
                "code": result_code,
                "description": "Test description"
            },
            "amount": amount,
            "currency": currency,
            "merchantTransactionId": merchant_txn_id
        }
    })
}

// ---------------------------------------------------------------------------
// Task 7.2: PeachPaymentsAdapter tests
// ---------------------------------------------------------------------------

#[test]
fn translate_webhook_valid_charge_succeeded() {
    let adapter = test_adapter();
    let pay_id = test_payment_id();
    let json = peach_webhook_json(
        "evt_001",
        "charge.succeeded",
        "000.000.000",
        "150.00",
        "ZAR",
        pay_id.as_str(),
    );
    let raw = raw_webhook(&json);

    let event = adapter.translate_webhook(&raw).unwrap();

    assert_eq!(event.provider, "peach_payments");
    assert_eq!(event.state_before, PaymentState::Created);
    assert_eq!(event.state_after, PaymentState::Authorized);
    assert_eq!(event.event_type.as_str(), "payment.charge.authorized");
    assert_eq!(event.amount.value, 15000);
    assert_eq!(event.amount.currency, Currency::ZAR);
    assert_eq!(event.payment_id, pay_id);
}

#[test]
fn translate_webhook_valid_refund() {
    let adapter = test_adapter();
    let pay_id = test_payment_id();
    let json = peach_webhook_json(
        "evt_002",
        "refund.succeeded",
        "000.000.000",
        "50.00",
        "ZAR",
        pay_id.as_str(),
    );
    let raw = raw_webhook(&json);

    let event = adapter.translate_webhook(&raw).unwrap();

    assert_eq!(event.state_before, PaymentState::Captured);
    assert_eq!(event.state_after, PaymentState::Refunded);
    assert_eq!(event.event_type.as_str(), "payment.refund.succeeded");
    assert_eq!(event.amount.value, 5000);
}

#[test]
fn translate_webhook_3ds_flow() {
    let adapter = test_adapter();
    let pay_id = test_payment_id();

    // Step 1: 3DS redirect
    let json_redirect = peach_webhook_json(
        "evt_003",
        "charge.pending",
        "000.100.112",
        "200.00",
        "ZAR",
        pay_id.as_str(),
    );
    let event_redirect = adapter
        .translate_webhook(&raw_webhook(&json_redirect))
        .unwrap();
    assert_eq!(event_redirect.state_before, PaymentState::Created);
    assert_eq!(event_redirect.state_after, PaymentState::Pending3ds);

    // Step 2: 3DS verification success
    let json_success = peach_webhook_json(
        "evt_004",
        "charge.succeeded",
        "000.100.110",
        "200.00",
        "ZAR",
        pay_id.as_str(),
    );
    let event_success = adapter
        .translate_webhook(&raw_webhook(&json_success))
        .unwrap();
    assert_eq!(event_success.state_before, PaymentState::Pending3ds);
    assert_eq!(event_success.state_after, PaymentState::Authorized);
}

#[test]
fn translate_webhook_invalid_json_returns_error() {
    let adapter = test_adapter();
    let raw = RawWebhook {
        headers: HashMap::new(),
        body: b"not valid json".to_vec(),
    };

    let result = adapter.translate_webhook(&raw);
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::InvalidResponse { provider, .. } => {
            assert_eq!(provider, "peach_payments");
        }
        other => panic!("expected InvalidResponse, got: {other:?}"),
    }
}

#[test]
fn translate_webhook_missing_required_fields_returns_error() {
    let adapter = test_adapter();
    // Valid JSON structure but missing required fields (amount, currency, merchantTransactionId)
    let json = serde_json::json!({
        "id": "evt_partial",
        "type": "charge.succeeded",
        "payload": {
            "id": "txn_001",
            "result": {
                "code": "000.000.000",
                "description": "Success"
            }
        }
    });
    let raw = raw_webhook(&json);

    let result = adapter.translate_webhook(&raw);
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::InvalidResponse { provider, .. } => {
            assert_eq!(provider, "peach_payments");
        }
        other => panic!("expected InvalidResponse, got: {other:?}"),
    }
}

#[test]
fn translate_webhook_preserves_raw_payload() {
    let adapter = test_adapter();
    let pay_id = test_payment_id();
    let json = peach_webhook_json(
        "evt_005",
        "charge.succeeded",
        "000.000.000",
        "100.00",
        "ZAR",
        pay_id.as_str(),
    );
    let raw = raw_webhook(&json);

    let event = adapter.translate_webhook(&raw).unwrap();

    // raw_provider_payload should contain the original webhook data
    assert_eq!(event.raw_provider_payload["id"], "evt_005");
    assert_eq!(event.raw_provider_payload["type"], "charge.succeeded");
    assert_eq!(event.raw_provider_payload["payload"]["amount"], "100.00");
}

#[test]
fn signature_config_returns_peach_config() {
    let adapter = test_adapter();
    let config = PaymentAdapter::signature_config(&adapter);

    assert_eq!(config.header_name, "X-Peach-Signature");
    assert_eq!(config.secret_env_var, "PEACH_SANDBOX_WEBHOOK_SECRET");
}

// ---------------------------------------------------------------------------
// Task 7.3: WebhookNormalizer tests
// ---------------------------------------------------------------------------

#[test]
fn extract_idempotency_key_correct_format() {
    let adapter = test_adapter();
    let pay_id = test_payment_id();
    let json = peach_webhook_json(
        "evt_006",
        "charge.succeeded",
        "000.000.000",
        "100.00",
        "ZAR",
        pay_id.as_str(),
    );
    let raw = raw_webhook(&json);

    let key = adapter.extract_idempotency_key(&raw).unwrap();
    let expected = format!("peach:{}:webhook:evt_006", pay_id.as_str());
    assert_eq!(key.as_ref(), expected);
}

#[test]
fn normalize_delegates_to_translate_webhook() {
    let adapter = test_adapter();
    let pay_id = test_payment_id();
    let json = peach_webhook_json(
        "evt_007",
        "charge.succeeded",
        "000.000.000",
        "100.00",
        "ZAR",
        pay_id.as_str(),
    );
    let raw = raw_webhook(&json);

    // Both should produce consistent results
    let adapter_event = adapter.translate_webhook(&raw).unwrap();
    let normalizer: &dyn WebhookNormalizer = &adapter;
    let normalizer_event = normalizer.normalize(&raw).unwrap();

    assert_eq!(adapter_event.provider, normalizer_event.provider);
    assert_eq!(adapter_event.state_after, normalizer_event.state_after);
    assert_eq!(adapter_event.amount, normalizer_event.amount);
    assert_eq!(adapter_event.payment_id, normalizer_event.payment_id);
}

#[test]
fn normalizer_signature_config_matches_adapter() {
    let adapter = test_adapter();
    let adapter_sig = PaymentAdapter::signature_config(&adapter);
    let normalizer_sig = WebhookNormalizer::signature_config(&adapter);
    assert_eq!(adapter_sig.header_name, normalizer_sig.header_name);
    assert_eq!(adapter_sig.secret_env_var, normalizer_sig.secret_env_var);
}

// ---------------------------------------------------------------------------
// Task 7.4: execute() stub test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_create_intent_returns_unsupported() {
    let adapter = test_adapter();
    let intent = PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(10000, Currency::ZAR),
        provider: "peach_payments".to_owned(),
        metadata: serde_json::json!({}),
    };

    let result = adapter.execute(PaymentCommand::CreateIntent, &intent).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::UnsupportedCommand { provider, .. } => {
            assert_eq!(provider, "peach_payments");
        }
        other => panic!("expected UnsupportedCommand, got: {other:?}"),
    }
}

fn config_with_no_credentials() -> AdapterConfig {
    AdapterConfig {
        provider_name: "peach_payments".to_owned(),
        sandbox: true,
        base_url: "https://testsecure.peachpayments.com".to_owned(),
        api_key_env_var: "PAYRAIL_TEST_NONEXISTENT_API_KEY".to_owned(),
        webhook_secret_env_var: "PAYRAIL_TEST_NONEXISTENT_SECRET".to_owned(),
        entity_id_env_var: "PAYRAIL_TEST_NONEXISTENT_ENTITY".to_owned(),
        timeout: Duration::from_secs(30),
    }
}

#[tokio::test]
async fn execute_authorize_without_credentials_returns_config_error() {
    let adapter = PeachPaymentsAdapter::new(config_with_no_credentials());
    let intent = PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(10000, Currency::ZAR),
        provider: "peach_payments".to_owned(),
        metadata: serde_json::json!({
            "card_number": "4200000000000000",
            "card_holder": "Test Holder",
            "card_expiry_month": "12",
            "card_expiry_year": "2030",
            "card_cvv": "123"
        }),
    };

    let result = adapter.execute(PaymentCommand::Authorize, &intent).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::ConfigurationError(msg) => {
            assert!(msg.contains("API key") || msg.contains("NONEXISTENT"));
        }
        other => panic!("expected ConfigurationError, got: {other:?}"),
    }
}

#[tokio::test]
async fn execute_capture_without_credentials_returns_config_error() {
    let adapter = PeachPaymentsAdapter::new(config_with_no_credentials());
    let intent = PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(10000, Currency::ZAR),
        provider: "peach_payments".to_owned(),
        metadata: serde_json::json!({
            "provider_transaction_id": "txn_test_123"
        }),
    };

    let result = adapter.execute(PaymentCommand::Capture, &intent).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::ConfigurationError(msg) => {
            assert!(msg.contains("API key") || msg.contains("NONEXISTENT"));
        }
        other => panic!("expected ConfigurationError, got: {other:?}"),
    }
}

#[tokio::test]
async fn execute_authorize_missing_card_data_returns_config_error() {
    let adapter = test_adapter();
    let intent = PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(10000, Currency::ZAR),
        provider: "peach_payments".to_owned(),
        metadata: serde_json::json!({}),
    };

    // Set fake env vars so we get past credential check
    // SAFETY: test-only, no concurrent access to these env vars in this test
    unsafe {
        std::env::set_var("PEACH_SANDBOX_API_KEY", "test_key");
        std::env::set_var("PEACH_SANDBOX_ENTITY_ID", "test_entity");
    }

    let result = adapter.execute(PaymentCommand::Authorize, &intent).await;

    // SAFETY: test cleanup
    unsafe {
        std::env::remove_var("PEACH_SANDBOX_API_KEY");
        std::env::remove_var("PEACH_SANDBOX_ENTITY_ID");
    }

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::ConfigurationError(msg) => {
            assert!(msg.contains("card_number"));
        }
        other => panic!("expected ConfigurationError for missing card data, got: {other:?}"),
    }
}

/// A mock secret store that always returns a fixed value.
struct FakeSecretStore;
impl SecretStore for FakeSecretStore {
    fn get_secret(&self, _key: &str) -> Result<String, SignatureError> {
        Ok("fake_value".to_owned())
    }
}

#[tokio::test]
async fn execute_capture_missing_provider_txn_id_returns_config_error() {
    let adapter = PeachPaymentsAdapter::with_secret_store(test_config(), Box::new(FakeSecretStore));
    let intent = PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(10000, Currency::ZAR),
        provider: "peach_payments".to_owned(),
        metadata: serde_json::json!({}), // no provider_transaction_id
    };

    let result = adapter.execute(PaymentCommand::Capture, &intent).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::ConfigurationError(msg) => {
            assert!(
                msg.contains("provider_transaction_id"),
                "expected error about provider_transaction_id, got: {msg}"
            );
        }
        other => panic!("expected ConfigurationError, got: {other:?}"),
    }
}
