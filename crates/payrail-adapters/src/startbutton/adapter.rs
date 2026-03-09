use std::future::Future;
use std::pin::Pin;

use chrono::Utc;

use payrail_core::{
    CanonicalEvent, EnvSecretStore, EventId, IdempotencyKey, Money, PaymentCommand, PaymentId,
    PaymentIntent, PaymentState, RawWebhook, ReceiverError, SecretStore, SignatureConfig,
    WebhookNormalizer,
};

use super::mappings::{
    startbutton_amount_to_money, startbutton_event_to_canonical_state,
    startbutton_event_to_event_type,
};
use super::types::StartbuttonWebhookPayload;
use crate::{AdapterConfig, AdapterError, PaymentAdapter, PaymentEvent};

// ---------------------------------------------------------------------------
// StartbuttonAdapter
// ---------------------------------------------------------------------------

/// Startbutton payment adapter.
///
/// Translates between Startbutton's API format and PayRail's canonical types.
/// All state machine enforcement happens in payrail-core, not here.
pub struct StartbuttonAdapter {
    config: AdapterConfig,
    signature_config: SignatureConfig,
    client: reqwest::Client,
    secret_store: Box<dyn SecretStore>,
}

impl StartbuttonAdapter {
    pub fn new(config: AdapterConfig) -> Self {
        Self::with_secret_store(config, Box::new(EnvSecretStore))
    }

    pub fn with_secret_store(config: AdapterConfig, secret_store: Box<dyn SecretStore>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("failed to build HTTP client");
        Self {
            config,
            // VERIFY: Startbutton signature header name (confidence: 0.78, source: community_report,
            // check: test against sandbox — may use different header name)
            signature_config: SignatureConfig {
                method: payrail_core::SignatureMethod::HmacSha256,
                header_name: "X-Startbutton-Signature".to_owned(),
                secret_env_var: "STARTBUTTON_SANDBOX_WEBHOOK_SECRET".to_owned(),
            },
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
}

// ---------------------------------------------------------------------------
// PaymentAdapter trait
// ---------------------------------------------------------------------------

impl PaymentAdapter for StartbuttonAdapter {
    fn execute(
        &self,
        command: PaymentCommand,
        intent: &PaymentIntent,
    ) -> Pin<Box<dyn Future<Output = Result<PaymentEvent, AdapterError>> + Send + '_>> {
        let provider = self.config.provider_name.clone();
        let base_url = self.config.base_url.clone();
        let payment_id = intent.id.as_str().to_owned();
        let amount = intent.amount.clone();
        let metadata = intent.metadata.clone();

        Box::pin(async move {
            let api_key = self.load_api_key()?;

            match command {
                PaymentCommand::CreateIntent => Err(AdapterError::UnsupportedCommand {
                    provider,
                    command: "CreateIntent".to_owned(),
                }),
                PaymentCommand::Authorize => {
                    let body = serde_json::json!({
                        "amount": amount.value,
                        "currency": amount.currency.to_string(),
                        "merchant_reference": payment_id,
                        "payment_method": metadata.get("payment_method")
                            .and_then(|v| v.as_str())
                            .unwrap_or("card"),
                    });

                    let resp = self
                        .client
                        .post(format!("{base_url}/payments"))
                        .bearer_auth(&api_key)
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| AdapterError::ProviderError {
                            provider: provider.clone(),
                            message: format!("HTTP request failed: {e}"),
                        })?;

                    parse_startbutton_response(resp, &provider, &amount, PaymentState::Authorized)
                        .await
                }
                PaymentCommand::Capture => {
                    let txn_id = metadata_str(&metadata, "provider_transaction_id")?;
                    validate_txn_id(&txn_id)?;
                    let body = serde_json::json!({ "amount": amount.value });

                    let resp = self
                        .client
                        .post(format!("{base_url}/payments/{txn_id}/capture"))
                        .bearer_auth(&api_key)
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| AdapterError::ProviderError {
                            provider: provider.clone(),
                            message: format!("HTTP request failed: {e}"),
                        })?;

                    parse_startbutton_response(resp, &provider, &amount, PaymentState::Captured)
                        .await
                }
                PaymentCommand::Refund => {
                    let txn_id = metadata_str(&metadata, "provider_transaction_id")?;
                    validate_txn_id(&txn_id)?;
                    let body = serde_json::json!({
                        "amount": amount.value,
                        "reason": metadata.get("reason").and_then(|v| v.as_str()).unwrap_or("customer_request"),
                    });

                    let resp = self
                        .client
                        .post(format!("{base_url}/payments/{txn_id}/refund"))
                        .bearer_auth(&api_key)
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| AdapterError::ProviderError {
                            provider: provider.clone(),
                            message: format!("HTTP request failed: {e}"),
                        })?;

                    parse_startbutton_response(resp, &provider, &amount, PaymentState::Refunded)
                        .await
                }
                PaymentCommand::Void => {
                    let txn_id = metadata_str(&metadata, "provider_transaction_id")?;
                    validate_txn_id(&txn_id)?;

                    let resp = self
                        .client
                        .post(format!("{base_url}/payments/{txn_id}/void"))
                        .bearer_auth(&api_key)
                        .send()
                        .await
                        .map_err(|e| AdapterError::ProviderError {
                            provider: provider.clone(),
                            message: format!("HTTP request failed: {e}"),
                        })?;

                    parse_startbutton_response(resp, &provider, &amount, PaymentState::Voided).await
                }
            }
        })
    }

    fn translate_webhook(&self, raw: &RawWebhook) -> Result<CanonicalEvent, AdapterError> {
        // 1. Parse webhook body
        let webhook: StartbuttonWebhookPayload =
            serde_json::from_slice(&raw.body).map_err(|e| AdapterError::InvalidResponse {
                provider: "startbutton".to_owned(),
                details: format!("webhook JSON parse failed: {e}"),
            })?;

        // 2. Extract fields
        let event_type_str = &webhook.event;
        let status = &webhook.data.status;
        let merchant_ref = &webhook.data.merchant_reference;
        let startbutton_txn_id = &webhook.data.id;

        // 3. Map to canonical state transition
        let (state_before, state_after) =
            startbutton_event_to_canonical_state(event_type_str, status)?;

        // 4. Map to canonical event type
        let event_type = startbutton_event_to_event_type(event_type_str)?;

        // 5. Parse amount (integer cents — no decimal conversion needed)
        let amount = startbutton_amount_to_money(webhook.data.amount, &webhook.data.currency)?;

        // 6. Parse PaymentId from merchant_reference
        let payment_id: PaymentId =
            merchant_ref
                .parse()
                .map_err(|e| AdapterError::WebhookTranslationFailed {
                    provider: "startbutton".to_owned(),
                    reason: format!("invalid merchant_reference '{merchant_ref}': {e}"),
                })?;

        // 7. Generate idempotency key: startbutton:{merchantRef}:webhook:{txnId}
        let idempotency_key =
            IdempotencyKey::from_webhook("startbutton", merchant_ref, startbutton_txn_id).map_err(
                |e| AdapterError::WebhookTranslationFailed {
                    provider: "startbutton".to_owned(),
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
            provider: "startbutton".to_owned(),
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
// WebhookNormalizer trait
// ---------------------------------------------------------------------------

impl WebhookNormalizer for StartbuttonAdapter {
    fn signature_config(&self) -> &SignatureConfig {
        &self.signature_config
    }

    fn extract_idempotency_key(&self, raw: &RawWebhook) -> Result<IdempotencyKey, ReceiverError> {
        let webhook: StartbuttonWebhookPayload = serde_json::from_slice(&raw.body)
            .map_err(|e| ReceiverError::NormalizationFailed(format!("JSON parse failed: {e}")))?;

        IdempotencyKey::from_webhook(
            "startbutton",
            &webhook.data.merchant_reference,
            &webhook.data.id,
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

async fn parse_startbutton_response(
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
        let error_msg = body
            .get("error_message")
            .or_else(|| body.get("message"))
            .and_then(|d| d.as_str())
            .unwrap_or("unknown error");
        return Err(AdapterError::ProviderError {
            provider: provider.to_owned(),
            message: format!("HTTP {status}: {error_msg}"),
        });
    }

    let txn_id = body
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    // Startbutton returns explicit status — check for failure states
    let sb_status = body
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN");

    let state = match sb_status {
        "FAILED" | "DECLINED" | "REVERSED" => PaymentState::Failed,
        _ => success_state,
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
