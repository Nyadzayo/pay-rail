use std::time::Duration;

use payrail_adapters::{AdapterConfig, AdapterError, PaymentAdapter, PeachPaymentsAdapter};
use payrail_core::{Currency, Money, PaymentCommand, PaymentId, PaymentIntent, PaymentState};

// ---------------------------------------------------------------------------
// Sandbox test helpers
// ---------------------------------------------------------------------------

fn sandbox_config() -> AdapterConfig {
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

fn sandbox_adapter() -> PeachPaymentsAdapter {
    PeachPaymentsAdapter::new(sandbox_config())
}

fn authorize_intent(amount_cents: i64) -> PaymentIntent {
    PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(amount_cents, Currency::ZAR),
        provider: "peach_payments".to_owned(),
        metadata: serde_json::json!({
            "card_number": "4200000000000000",
            "card_holder": "Test Holder",
            "card_expiry_month": "12",
            "card_expiry_year": "2030",
            "card_cvv": "123",
            "payment_brand": "VISA"
        }),
    }
}

fn followup_intent(amount_cents: i64, provider_txn_id: &str) -> PaymentIntent {
    PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(amount_cents, Currency::ZAR),
        provider: "peach_payments".to_owned(),
        metadata: serde_json::json!({
            "provider_transaction_id": provider_txn_id
        }),
    }
}

// ---------------------------------------------------------------------------
// Sandbox integration tests (require PEACH_SANDBOX_* env vars)
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn sandbox_authorize_and_capture() {
    let adapter = sandbox_adapter();

    // Authorize
    let auth_intent = authorize_intent(10000); // R100.00
    let auth_result = adapter
        .execute(PaymentCommand::Authorize, &auth_intent)
        .await
        .expect("authorize should succeed");

    assert_eq!(auth_result.state, PaymentState::Authorized);
    assert!(!auth_result.provider_transaction_id.is_empty());

    // Capture
    let cap_intent = followup_intent(10000, &auth_result.provider_transaction_id);
    let cap_result = adapter
        .execute(PaymentCommand::Capture, &cap_intent)
        .await
        .expect("capture should succeed");

    assert_eq!(cap_result.state, PaymentState::Captured);
}

#[tokio::test]
#[ignore]
async fn sandbox_authorize_and_refund() {
    let adapter = sandbox_adapter();

    // Authorize
    let auth_intent = authorize_intent(5000); // R50.00
    let auth_result = adapter
        .execute(PaymentCommand::Authorize, &auth_intent)
        .await
        .expect("authorize should succeed");

    assert_eq!(auth_result.state, PaymentState::Authorized);

    // Capture (required before refund)
    let cap_intent = followup_intent(5000, &auth_result.provider_transaction_id);
    let cap_result = adapter
        .execute(PaymentCommand::Capture, &cap_intent)
        .await
        .expect("capture should succeed");

    assert_eq!(cap_result.state, PaymentState::Captured);

    // Refund
    let ref_intent = followup_intent(5000, &cap_result.provider_transaction_id);
    let ref_result = adapter
        .execute(PaymentCommand::Refund, &ref_intent)
        .await
        .expect("refund should succeed");

    assert_eq!(ref_result.state, PaymentState::Refunded);
}

#[tokio::test]
#[ignore]
async fn sandbox_authorize_and_void() {
    let adapter = sandbox_adapter();

    // Authorize
    let auth_intent = authorize_intent(7500); // R75.00
    let auth_result = adapter
        .execute(PaymentCommand::Authorize, &auth_intent)
        .await
        .expect("authorize should succeed");

    assert_eq!(auth_result.state, PaymentState::Authorized);

    // Void
    let void_intent = followup_intent(7500, &auth_result.provider_transaction_id);
    let void_result = adapter
        .execute(PaymentCommand::Void, &void_intent)
        .await
        .expect("void should succeed");

    assert_eq!(void_result.state, PaymentState::Voided);
}

#[tokio::test]
#[ignore]
async fn sandbox_authorization_failure() {
    let adapter = sandbox_adapter();

    // Use decline test card
    let intent = PaymentIntent {
        id: PaymentId::new(),
        amount: Money::new(10000, Currency::ZAR),
        provider: "peach_payments".to_owned(),
        metadata: serde_json::json!({
            "card_number": "4444333322221111",
            "card_holder": "Test Holder",
            "card_expiry_month": "12",
            "card_expiry_year": "2030",
            "card_cvv": "123",
            "payment_brand": "VISA"
        }),
    };

    let result = adapter.execute(PaymentCommand::Authorize, &intent).await;

    // Declined card should either return Failed state or a ProviderError
    match result {
        Ok(event) => {
            assert_eq!(event.state, PaymentState::Failed);
        }
        Err(AdapterError::ProviderError { .. }) => {
            // Also acceptable — provider rejected the request
        }
        Err(other) => panic!("unexpected error type: {other:?}"),
    }
}

#[tokio::test]
async fn sandbox_credentials_missing_returns_config_error() {
    let adapter = PeachPaymentsAdapter::new(AdapterConfig {
        provider_name: "peach_payments".to_owned(),
        sandbox: true,
        base_url: "https://testsecure.peachpayments.com".to_owned(),
        api_key_env_var: "PAYRAIL_NONEXISTENT_KEY_12345".to_owned(),
        webhook_secret_env_var: "PAYRAIL_NONEXISTENT_SECRET_12345".to_owned(),
        entity_id_env_var: "PAYRAIL_NONEXISTENT_ENTITY_12345".to_owned(),
        timeout: Duration::from_secs(30),
    });

    let intent = authorize_intent(10000);
    let result = adapter.execute(PaymentCommand::Authorize, &intent).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::ConfigurationError(msg) => {
            assert!(msg.contains("PAYRAIL_NONEXISTENT"));
        }
        other => panic!("expected ConfigurationError, got: {other:?}"),
    }
}
