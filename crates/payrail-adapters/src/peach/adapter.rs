use std::future::Future;
use std::pin::Pin;

use chrono::Utc;

use payrail_core::{
    CanonicalEvent, EnvSecretStore, EventId, IdempotencyKey, Money, PaymentCommand, PaymentId,
    PaymentIntent, PaymentState, RawWebhook, ReceiverError, SecretStore, SignatureConfig,
    WebhookNormalizer,
};

use super::mappings::{
    money_to_peach_amount, peach_amount_to_money, peach_event_to_canonical_state,
    peach_event_to_event_type,
};
use super::types::{PeachResultCode, PeachWebhookPayload};
use crate::{AdapterConfig, AdapterError, PaymentAdapter, PaymentEvent};

// ---------------------------------------------------------------------------
// PeachPaymentsAdapter
// ---------------------------------------------------------------------------

/// Peach Payments adapter.
///
/// Translates between Peach Payments' API format and PayRail's canonical types.
/// All state machine enforcement happens in payrail-core, not here.
pub struct PeachPaymentsAdapter {
    config: AdapterConfig,
    signature_config: SignatureConfig,
    client: reqwest::Client,
    secret_store: Box<dyn SecretStore>,
}

impl PeachPaymentsAdapter {
    /// Constructs a new adapter with the given configuration.
    ///
    /// Uses `EnvSecretStore` for credential loading and creates a shared
    /// `reqwest::Client` with the configured timeout for connection pooling.
    pub fn new(config: AdapterConfig) -> Self {
        Self::with_secret_store(config, Box::new(EnvSecretStore))
    }

    /// Constructs an adapter with a custom `SecretStore` for testing.
    pub fn with_secret_store(config: AdapterConfig, secret_store: Box<dyn SecretStore>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("failed to build HTTP client");
        Self {
            config,
            signature_config: SignatureConfig::peach_payments(),
            client,
            secret_store,
        }
    }

    fn load_api_key(&self) -> Result<String, AdapterError> {
        self.secret_store
            .get_secret(&self.config.api_key_env_var)
            .map_err(|_| {
                AdapterError::ConfigurationError(format!(
                    "missing environment variable '{}' for API key",
                    self.config.api_key_env_var
                ))
            })
    }

    fn load_entity_id(&self) -> Result<String, AdapterError> {
        self.secret_store
            .get_secret(&self.config.entity_id_env_var)
            .map_err(|_| {
                AdapterError::ConfigurationError(format!(
                    "missing environment variable '{}' for entity ID",
                    self.config.entity_id_env_var
                ))
            })
    }
}

// ---------------------------------------------------------------------------
// PaymentAdapter trait (Task 4)
// ---------------------------------------------------------------------------

impl PaymentAdapter for PeachPaymentsAdapter {
    fn execute(
        &self,
        command: PaymentCommand,
        intent: &PaymentIntent,
    ) -> Pin<Box<dyn Future<Output = Result<PaymentEvent, AdapterError>> + Send + '_>> {
        // Extract everything from intent before async block (intent borrow may not outlive future)
        let provider = self.config.provider_name.clone();
        let base_url = self.config.base_url.clone();
        let payment_id = intent.id.as_str().to_owned();
        let amount = intent.amount.clone();
        let metadata = intent.metadata.clone();

        Box::pin(async move {
            match command {
                PaymentCommand::CreateIntent => Err(AdapterError::UnsupportedCommand {
                    provider,
                    command: "CreateIntent".to_owned(),
                }),
                PaymentCommand::Authorize => {
                    let api_key = self.load_api_key()?;
                    let entity_id = self.load_entity_id()?;
                    let amount_str = money_to_peach_amount(&amount);
                    let currency_str = amount.currency.to_string();

                    // Card data from intent metadata.
                    // SAFETY(PCI): Card data passes through in-memory only and is never
                    // serialized or logged by the adapter. Callers MUST NOT persist
                    // PaymentIntent.metadata containing raw PAN/CVV. Production systems
                    // should use tokenized card references instead.
                    // TODO: Replace with tokenized card input for production (Epic 4+).
                    let card_number = metadata_str(&metadata, "card_number")?;
                    let card_holder = metadata_str(&metadata, "card_holder")?;
                    let card_expiry_month = metadata_str(&metadata, "card_expiry_month")?;
                    let card_expiry_year = metadata_str(&metadata, "card_expiry_year")?;
                    let card_cvv = metadata_str(&metadata, "card_cvv")?;
                    let payment_brand = metadata
                        .get("payment_brand")
                        .and_then(|v| v.as_str())
                        .unwrap_or("VISA")
                        .to_owned();

                    let resp = self
                        .client
                        .post(format!("{base_url}/v1/payments"))
                        .bearer_auth(&api_key)
                        .form(&[
                            ("entityId", entity_id.as_str()),
                            ("amount", amount_str.as_str()),
                            ("currency", currency_str.as_str()),
                            ("paymentBrand", payment_brand.as_str()),
                            ("paymentType", "PA"),
                            ("card.number", card_number.as_str()),
                            ("card.holder", card_holder.as_str()),
                            ("card.expiryMonth", card_expiry_month.as_str()),
                            ("card.expiryYear", card_expiry_year.as_str()),
                            ("card.cvv", card_cvv.as_str()),
                            ("merchantTransactionId", payment_id.as_str()),
                        ])
                        .send()
                        .await
                        .map_err(|e| AdapterError::ProviderError {
                            provider: provider.clone(),
                            message: format!("HTTP request failed: {e}"),
                        })?;

                    parse_peach_response(resp, &provider, &amount, PaymentState::Authorized).await
                }
                PaymentCommand::Capture | PaymentCommand::Refund | PaymentCommand::Void => {
                    let api_key = self.load_api_key()?;
                    let entity_id = self.load_entity_id()?;
                    let amount_str = money_to_peach_amount(&amount);
                    let currency_str = amount.currency.to_string();
                    let txn_id = metadata_str(&metadata, "provider_transaction_id")?;
                    validate_txn_id(&txn_id)?;

                    let (payment_type, success_state) = match command {
                        PaymentCommand::Capture => ("CP", PaymentState::Captured),
                        PaymentCommand::Refund => ("RF", PaymentState::Refunded),
                        PaymentCommand::Void => ("RV", PaymentState::Voided),
                        _ => unreachable!(),
                    };

                    let resp = self
                        .client
                        .post(format!("{base_url}/v1/payments/{txn_id}"))
                        .bearer_auth(&api_key)
                        .form(&[
                            ("entityId", entity_id.as_str()),
                            ("amount", amount_str.as_str()),
                            ("currency", currency_str.as_str()),
                            ("paymentType", payment_type),
                        ])
                        .send()
                        .await
                        .map_err(|e| AdapterError::ProviderError {
                            provider: provider.clone(),
                            message: format!("HTTP request failed: {e}"),
                        })?;

                    parse_peach_response(resp, &provider, &amount, success_state).await
                }
            }
        })
    }

    fn translate_webhook(&self, raw: &RawWebhook) -> Result<CanonicalEvent, AdapterError> {
        // 1. Parse webhook body
        let webhook: PeachWebhookPayload =
            serde_json::from_slice(&raw.body).map_err(|e| AdapterError::InvalidResponse {
                provider: "peach_payments".to_owned(),
                details: format!("webhook JSON parse failed: {e}"),
            })?;

        // 2. Extract fields
        let event_type_str = &webhook.event_type;
        let result_code = &webhook.payload.result.code;
        let amount_str = &webhook.payload.amount;
        let currency_str = &webhook.payload.currency;
        let merchant_txn_id = &webhook.payload.merchant_transaction_id;
        let peach_event_id = &webhook.id;

        // 3. Map to canonical state transition
        let (state_before, state_after) =
            peach_event_to_canonical_state(event_type_str, result_code)?;

        // 4. Map to canonical event type
        let event_type = peach_event_to_event_type(event_type_str)?;

        // 5. Parse amount to Money (integer cents, never float)
        let amount = peach_amount_to_money(amount_str, currency_str)?;

        // 6. Parse PaymentId from merchantTransactionId
        let payment_id: PaymentId =
            merchant_txn_id
                .parse()
                .map_err(|e| AdapterError::WebhookTranslationFailed {
                    provider: "peach_payments".to_owned(),
                    reason: format!("invalid merchantTransactionId '{merchant_txn_id}': {e}"),
                })?;

        // 7. Generate idempotency key: peach:{merchantId}:webhook:{event_id}
        let idempotency_key =
            IdempotencyKey::from_webhook("peach", merchant_txn_id, peach_event_id).map_err(
                |e| AdapterError::WebhookTranslationFailed {
                    provider: "peach_payments".to_owned(),
                    reason: format!("idempotency key generation failed: {e}"),
                },
            )?;

        // 8. Preserve raw payload for audit trail
        let raw_provider_payload: serde_json::Value =
            serde_json::from_slice(&raw.body).unwrap_or(serde_json::json!({}));

        // 9. Construct canonical event
        Ok(CanonicalEvent {
            event_id: EventId::new(),
            event_type,
            payment_id,
            provider: "peach_payments".to_owned(),
            timestamp: Utc::now(),
            state_before,
            state_after,
            amount,
            idempotency_key: idempotency_key.to_string(),
            raw_provider_payload,
            metadata: serde_json::json!({}),
        })
    }

    fn signature_config(&self) -> &SignatureConfig {
        &self.signature_config
    }
}

// ---------------------------------------------------------------------------
// WebhookNormalizer trait (Task 5)
// ---------------------------------------------------------------------------

impl WebhookNormalizer for PeachPaymentsAdapter {
    fn signature_config(&self) -> &SignatureConfig {
        &self.signature_config
    }

    fn extract_idempotency_key(&self, raw: &RawWebhook) -> Result<IdempotencyKey, ReceiverError> {
        let webhook: PeachWebhookPayload = serde_json::from_slice(&raw.body)
            .map_err(|e| ReceiverError::NormalizationFailed(format!("JSON parse failed: {e}")))?;

        IdempotencyKey::from_webhook(
            "peach",
            &webhook.payload.merchant_transaction_id,
            &webhook.id,
        )
        .map_err(|e| ReceiverError::NormalizationFailed(e.to_string()))
    }

    fn normalize(&self, raw: &RawWebhook) -> Result<CanonicalEvent, ReceiverError> {
        self.translate_webhook(raw)
            .map_err(|e| ReceiverError::NormalizationFailed(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Execute helpers
// ---------------------------------------------------------------------------

/// Validates that a provider transaction ID is safe to embed in a URL path.
///
/// Rejects IDs containing path separators, query strings, or other characters
/// that could manipulate the URL.
fn validate_txn_id(txn_id: &str) -> Result<(), AdapterError> {
    if txn_id.is_empty()
        || txn_id.contains('/')
        || txn_id.contains('?')
        || txn_id.contains('#')
        || txn_id.contains('&')
    {
        return Err(AdapterError::ConfigurationError(format!(
            "invalid provider_transaction_id '{txn_id}': contains disallowed characters"
        )));
    }
    Ok(())
}

fn metadata_str(metadata: &serde_json::Value, key: &str) -> Result<String, AdapterError> {
    metadata
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned())
        .ok_or_else(|| {
            AdapterError::ConfigurationError(format!("missing '{key}' in payment intent metadata"))
        })
}

async fn parse_peach_response(
    resp: reqwest::Response,
    provider: &str,
    amount: &Money,
    success_state: PaymentState,
) -> Result<PaymentEvent, AdapterError> {
    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AdapterError::InvalidResponse {
            provider: provider.to_owned(),
            details: format!("response JSON parse failed: {e}"),
        })?;

    if !status.is_success() {
        let desc = body
            .get("result")
            .and_then(|r| r.get("description"))
            .and_then(|d| d.as_str())
            .unwrap_or("unknown error");
        return Err(AdapterError::ProviderError {
            provider: provider.to_owned(),
            message: format!("HTTP {status}: {desc}"),
        });
    }

    let txn_id = body
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    let result_code = body
        .get("result")
        .and_then(|r| r.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or("999.999.999");

    let code = PeachResultCode::new(result_code);
    let state = if code.is_rejected() || code.is_timeout_or_error() {
        PaymentState::Failed
    } else {
        success_state
    };

    Ok(PaymentEvent {
        provider: provider.to_owned(),
        provider_transaction_id: txn_id,
        state,
        amount: amount.clone(),
        raw_response: body,
        timestamp: Utc::now(),
        metadata: serde_json::json!({}),
    })
}
