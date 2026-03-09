use std::collections::HashMap;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

// ---------------------------------------------------------------------------
// SignatureError (Task 4)
// ---------------------------------------------------------------------------

/// Errors arising from webhook signature verification.
///
/// Error messages follow the `[WHAT] [WHY] [WHAT TO DO]` format and never
/// include actual signing keys or expected signature values.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SignatureError {
    /// The provided signature did not match the computed HMAC digest.
    #[error(
        "[InvalidSignature] Webhook signature mismatch for header '{0}'. [Verify the signing key and raw body are correct]"
    )]
    InvalidSignature(String),

    /// The expected signature header was not present in the request.
    #[error(
        "[MissingHeader] Required signature header '{0}' not found in request. [Ensure the provider sends the signature header]"
    )]
    MissingHeader(String),

    /// The signing secret could not be loaded from the secret store.
    #[error(
        "[SecretNotFound] Signing secret '{0}' not available. [Set the environment variable or configure the secret store]"
    )]
    SecretNotFound(String),

    /// A low-level verification failure (e.g. hex decoding).
    #[error("[VerificationFailed] {0}. [Check that the signature is hex-encoded and well-formed]")]
    VerificationFailed(String),
}

// ---------------------------------------------------------------------------
// SignatureMethod & SignatureConfig (Task 2)
// ---------------------------------------------------------------------------

/// Supported webhook signature algorithms.
///
/// Marked `#[non_exhaustive]` so future methods (RSA, Ed25519) can be added
/// without breaking downstream code.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SignatureMethod {
    /// HMAC with SHA-256 — the most common provider scheme.
    HmacSha256,
}

/// Declares how a provider signs its webhooks.
///
/// Adapters construct a `SignatureConfig` to tell the framework which header
/// carries the signature, which algorithm is used, and which secret to load.
#[derive(Debug, Clone)]
pub struct SignatureConfig {
    /// The signature algorithm.
    pub method: SignatureMethod,
    /// HTTP header name that carries the hex-encoded signature.
    pub header_name: String,
    /// Name of the secret (environment variable) holding the signing key.
    pub secret_env_var: String,
}

impl SignatureConfig {
    /// Convenience constructor for Peach Payments webhook verification.
    pub fn peach_payments() -> Self {
        Self {
            method: SignatureMethod::HmacSha256,
            header_name: "X-Peach-Signature".to_owned(),
            secret_env_var: "PEACH_SANDBOX_WEBHOOK_SECRET".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// SecretStore trait & EnvSecretStore (Task 3)
// ---------------------------------------------------------------------------

/// Retrieves signing secrets for webhook verification.
///
/// MVP implementation reads from environment variables. The trait is `Send +
/// Sync` for thread safety and synchronous because env-var reads are cheap.
pub trait SecretStore: Send + Sync {
    /// Returns the secret value associated with `key`.
    fn get_secret(&self, key: &str) -> Result<String, SignatureError>;
}

/// Reads secrets from process environment variables.
pub struct EnvSecretStore;

impl SecretStore for EnvSecretStore {
    fn get_secret(&self, key: &str) -> Result<String, SignatureError> {
        std::env::var(key).map_err(|_| SignatureError::SecretNotFound(key.to_owned()))
    }
}

// ---------------------------------------------------------------------------
// HMAC SHA-256 helpers (Task 5)
// ---------------------------------------------------------------------------

type HmacSha256 = Hmac<Sha256>;

/// Computes the HMAC-SHA256 digest over `body` using `key`.
///
/// Operates on raw bytes — never parsed JSON.
pub fn compute_hmac_sha256(key: &[u8], body: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(body);
    mac.finalize().into_bytes().to_vec()
}

/// Timing-safe constant-time comparison of two byte slices.
///
/// Uses `subtle::ConstantTimeEq` so the comparison takes the same amount of
/// time regardless of where (or whether) the slices differ.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

// ---------------------------------------------------------------------------
// Public verification API (Task 6)
// ---------------------------------------------------------------------------

/// Verifies a webhook signature against the raw request body.
///
/// # Steps
/// 1. Extract the signature header (case-insensitive lookup).
/// 2. Retrieve the signing key from the `SecretStore`.
/// 3. Compute HMAC-SHA256 over the raw body.
/// 4. Hex-decode the provided signature.
/// 5. Constant-time compare the two digests.
///
/// Returns `Ok(())` on success, or a specific [`SignatureError`] on failure.
pub fn verify_signature(
    config: &SignatureConfig,
    headers: &HashMap<String, String>,
    raw_body: &[u8],
    secret_store: &dyn SecretStore,
) -> Result<(), SignatureError> {
    // 1. Case-insensitive header lookup
    let header_lower = config.header_name.to_lowercase();
    let provided_sig_hex = headers
        .iter()
        .find(|(k, _)| k.to_lowercase() == header_lower)
        .map(|(_, v)| v)
        .ok_or_else(|| SignatureError::MissingHeader(config.header_name.clone()))?;

    // 2. Retrieve signing key
    let secret = secret_store.get_secret(&config.secret_env_var)?;

    // 3. Compute expected digest based on configured method
    let computed = match config.method {
        SignatureMethod::HmacSha256 => compute_hmac_sha256(secret.as_bytes(), raw_body),
    };

    // 4. Hex-decode the provided signature
    let provided = hex::decode(provided_sig_hex).map_err(|e| {
        SignatureError::VerificationFailed(format!("Failed to hex-decode signature: {e}"))
    })?;

    // 5. Constant-time compare
    if !constant_time_eq(&computed, &provided) {
        return Err(SignatureError::InvalidSignature(config.header_name.clone()));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests (Task 8)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: in-memory secret store for deterministic testing.
    struct TestSecretStore {
        secrets: HashMap<String, String>,
    }

    impl TestSecretStore {
        fn new(pairs: &[(&str, &str)]) -> Self {
            Self {
                secrets: pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            }
        }
    }

    impl SecretStore for TestSecretStore {
        fn get_secret(&self, key: &str) -> Result<String, SignatureError> {
            self.secrets
                .get(key)
                .cloned()
                .ok_or_else(|| SignatureError::SecretNotFound(key.to_owned()))
        }
    }

    // 8.1 — RFC 4231 Test Case 2 (key = "Jefe", data = "what do ya want for nothing?")
    #[test]
    fn hmac_sha256_produces_correct_digest() {
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        let digest = compute_hmac_sha256(key, data);
        let expected =
            hex::decode("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
                .unwrap();
        assert_eq!(digest, expected);
    }

    // 8.2
    #[test]
    fn constant_time_eq_matches_equal_values() {
        let a = b"hello world";
        let b = b"hello world";
        assert!(constant_time_eq(a, b));
    }

    // 8.3
    #[test]
    fn constant_time_eq_rejects_unequal_values() {
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"short", b"longer_value"));
    }

    // 8.4
    #[test]
    fn verify_signature_accepts_valid() {
        let secret = "test_secret_key";
        let body = b"raw webhook body";
        let expected_sig = hex::encode(compute_hmac_sha256(secret.as_bytes(), body));

        let store = TestSecretStore::new(&[("WEBHOOK_SECRET", secret)]);
        let config = SignatureConfig {
            method: SignatureMethod::HmacSha256,
            header_name: "X-Signature".to_owned(),
            secret_env_var: "WEBHOOK_SECRET".to_owned(),
        };
        let mut headers = HashMap::new();
        headers.insert("X-Signature".to_owned(), expected_sig);

        assert!(verify_signature(&config, &headers, body, &store).is_ok());
    }

    // 8.5
    #[test]
    fn verify_signature_rejects_invalid() {
        let store = TestSecretStore::new(&[("WEBHOOK_SECRET", "real_key")]);
        let config = SignatureConfig {
            method: SignatureMethod::HmacSha256,
            header_name: "X-Signature".to_owned(),
            secret_env_var: "WEBHOOK_SECRET".to_owned(),
        };
        let mut headers = HashMap::new();
        headers.insert(
            "X-Signature".to_owned(),
            hex::encode(compute_hmac_sha256(b"wrong_key", b"body")),
        );

        let result = verify_signature(&config, &headers, b"body", &store);
        assert_eq!(
            result,
            Err(SignatureError::InvalidSignature("X-Signature".to_owned()))
        );
    }

    // 8.6
    #[test]
    fn verify_signature_rejects_missing_header() {
        let store = TestSecretStore::new(&[("WEBHOOK_SECRET", "key")]);
        let config = SignatureConfig {
            method: SignatureMethod::HmacSha256,
            header_name: "X-Signature".to_owned(),
            secret_env_var: "WEBHOOK_SECRET".to_owned(),
        };
        let headers = HashMap::new(); // empty

        let result = verify_signature(&config, &headers, b"body", &store);
        assert_eq!(
            result,
            Err(SignatureError::MissingHeader("X-Signature".to_owned()))
        );
    }

    // 8.7
    #[test]
    fn verify_signature_rejects_missing_secret() {
        let store = TestSecretStore::new(&[]); // no secrets
        let config = SignatureConfig {
            method: SignatureMethod::HmacSha256,
            header_name: "X-Signature".to_owned(),
            secret_env_var: "WEBHOOK_SECRET".to_owned(),
        };
        let mut headers = HashMap::new();
        headers.insert("X-Signature".to_owned(), "deadbeef".to_owned());

        let result = verify_signature(&config, &headers, b"body", &store);
        assert_eq!(
            result,
            Err(SignatureError::SecretNotFound("WEBHOOK_SECRET".to_owned()))
        );
    }

    // 8.8
    #[test]
    fn signature_config_peach_payments() {
        let config = SignatureConfig::peach_payments();
        assert_eq!(config.method, SignatureMethod::HmacSha256);
        assert_eq!(config.header_name, "X-Peach-Signature");
        assert_eq!(config.secret_env_var, "PEACH_SANDBOX_WEBHOOK_SECRET");
    }

    // 8.9
    #[test]
    fn env_secret_store_reads_env_var() {
        let unique_key = "PAYRAIL_TEST_SIG_SECRET_8_9";
        unsafe { std::env::set_var(unique_key, "my_test_value") };
        let store = EnvSecretStore;
        assert_eq!(store.get_secret(unique_key).unwrap(), "my_test_value");
        unsafe { std::env::remove_var(unique_key) };
    }

    // 8.10
    #[test]
    fn env_secret_store_returns_error_on_missing() {
        let store = EnvSecretStore;
        let result = store.get_secret("PAYRAIL_DEFINITELY_NOT_SET_XYZ");
        assert_eq!(
            result,
            Err(SignatureError::SecretNotFound(
                "PAYRAIL_DEFINITELY_NOT_SET_XYZ".to_owned()
            ))
        );
    }

    // 8.11
    #[test]
    fn header_lookup_is_case_insensitive() {
        let secret = "key123";
        let body = b"data";
        let sig = hex::encode(compute_hmac_sha256(secret.as_bytes(), body));

        let store = TestSecretStore::new(&[("SEC", secret)]);
        let config = SignatureConfig {
            method: SignatureMethod::HmacSha256,
            header_name: "X-My-Signature".to_owned(),
            secret_env_var: "SEC".to_owned(),
        };

        // Header with different casing
        let mut headers = HashMap::new();
        headers.insert("x-my-signature".to_owned(), sig.clone());
        assert!(verify_signature(&config, &headers, body, &store).is_ok());

        let mut headers2 = HashMap::new();
        headers2.insert("X-MY-SIGNATURE".to_owned(), sig);
        assert!(verify_signature(&config, &headers2, body, &store).is_ok());
    }

    // 8.12a — hex-decode failure in verify_signature
    #[test]
    fn verify_signature_rejects_non_hex_signature() {
        let store = TestSecretStore::new(&[("SEC", "key")]);
        let config = SignatureConfig {
            method: SignatureMethod::HmacSha256,
            header_name: "X-Sig".to_owned(),
            secret_env_var: "SEC".to_owned(),
        };
        let mut headers = HashMap::new();
        headers.insert("X-Sig".to_owned(), "not-valid-hex!!!".to_owned());

        let result = verify_signature(&config, &headers, b"body", &store);
        assert!(matches!(result, Err(SignatureError::VerificationFailed(_))));
    }

    // E2-UNIT-001: verify_signature accepts a valid HMAC for an empty body
    #[test]
    fn verify_signature_with_empty_body() {
        let secret = "empty_body_secret";
        let body: &[u8] = b"";
        let expected_sig = hex::encode(compute_hmac_sha256(secret.as_bytes(), body));

        let store = TestSecretStore::new(&[("SEC", secret)]);
        let config = SignatureConfig {
            method: SignatureMethod::HmacSha256,
            header_name: "X-Sig".to_owned(),
            secret_env_var: "SEC".to_owned(),
        };
        let mut headers = HashMap::new();
        headers.insert("X-Sig".to_owned(), expected_sig);

        assert!(verify_signature(&config, &headers, body, &store).is_ok());
    }

    // E2-UNIT-005: SignatureMethod is non_exhaustive but internal match works without wildcard
    #[test]
    fn signature_method_is_non_exhaustive() {
        let method = SignatureMethod::HmacSha256;
        let label = match method {
            SignatureMethod::HmacSha256 => "hmac-sha256",
        };
        assert_eq!(label, "hmac-sha256");
    }

    // 8.12
    #[test]
    fn error_messages_do_not_contain_secret() {
        let actual_secret = "super_secret_key_12345";
        let expected_sig_hex = hex::encode(compute_hmac_sha256(b"wrong", b"body"));

        // SecretNotFound error
        let err = SignatureError::SecretNotFound("MY_VAR".to_owned());
        let msg = err.to_string();
        assert!(!msg.contains(actual_secret));

        // InvalidSignature error
        let err = SignatureError::InvalidSignature("X-Sig".to_owned());
        let msg = err.to_string();
        assert!(!msg.contains(actual_secret));
        assert!(!msg.contains(&expected_sig_hex));

        // VerificationFailed error
        let err = SignatureError::VerificationFailed("bad hex".to_owned());
        let msg = err.to_string();
        assert!(!msg.contains(actual_secret));
    }
}
