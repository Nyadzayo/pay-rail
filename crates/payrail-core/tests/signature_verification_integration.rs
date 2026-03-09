use std::collections::HashMap;

use payrail_core::webhook::signature::{
    SecretStore, SignatureConfig, SignatureError, SignatureMethod, compute_hmac_sha256,
    verify_signature,
};

/// In-memory secret store for integration tests.
struct MemSecretStore(HashMap<String, String>);

impl MemSecretStore {
    fn new(pairs: &[(&str, &str)]) -> Self {
        Self(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }
}

impl SecretStore for MemSecretStore {
    fn get_secret(&self, key: &str) -> Result<String, SignatureError> {
        self.0
            .get(key)
            .cloned()
            .ok_or_else(|| SignatureError::SecretNotFound(key.to_owned()))
    }
}

// 9.1 — Full signature verification flow
#[test]
fn full_signature_verification_flow() {
    let secret = "integration_test_secret";
    let raw_body = br#"{"event":"payment.charge.captured","amount":15000}"#;

    // Compute a valid signature
    let sig_hex = hex::encode(compute_hmac_sha256(secret.as_bytes(), raw_body));

    let config = SignatureConfig {
        method: SignatureMethod::HmacSha256,
        header_name: "X-Provider-Signature".to_owned(),
        secret_env_var: "TEST_WEBHOOK_SECRET".to_owned(),
    };

    let store = MemSecretStore::new(&[("TEST_WEBHOOK_SECRET", secret)]);

    let mut headers = HashMap::new();
    headers.insert("X-Provider-Signature".to_owned(), sig_hex);

    assert!(verify_signature(&config, &headers, raw_body, &store).is_ok());
}

// 9.2 — Tampered body is rejected
#[test]
fn tampered_body_rejected() {
    let secret = "tamper_test_secret";
    let original_body = br#"{"amount":15000}"#;
    let tampered_body = br#"{"amount":99999}"#;

    // Signature computed for the original body
    let sig_hex = hex::encode(compute_hmac_sha256(secret.as_bytes(), original_body));

    let config = SignatureConfig {
        method: SignatureMethod::HmacSha256,
        header_name: "X-Sig".to_owned(),
        secret_env_var: "SEC".to_owned(),
    };

    let store = MemSecretStore::new(&[("SEC", secret)]);

    let mut headers = HashMap::new();
    headers.insert("X-Sig".to_owned(), sig_hex);

    // Verify against tampered body — must fail
    let result = verify_signature(&config, &headers, tampered_body, &store);
    assert_eq!(
        result,
        Err(SignatureError::InvalidSignature("X-Sig".to_owned()))
    );
}

// 9.3 — Wrong provider key is rejected
#[test]
fn wrong_provider_key_rejected() {
    let correct_key = "correct_key";
    let wrong_key = "wrong_key";
    let body = br#"{"event":"test"}"#;

    // Signature computed with the wrong key
    let sig_hex = hex::encode(compute_hmac_sha256(wrong_key.as_bytes(), body));

    let config = SignatureConfig {
        method: SignatureMethod::HmacSha256,
        header_name: "X-Sig".to_owned(),
        secret_env_var: "KEY".to_owned(),
    };

    // Store has the correct key — signature was made with wrong key
    let store = MemSecretStore::new(&[("KEY", correct_key)]);

    let mut headers = HashMap::new();
    headers.insert("X-Sig".to_owned(), sig_hex);

    let result = verify_signature(&config, &headers, body, &store);
    assert_eq!(
        result,
        Err(SignatureError::InvalidSignature("X-Sig".to_owned()))
    );
}
