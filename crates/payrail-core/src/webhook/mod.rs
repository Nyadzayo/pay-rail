/// Two-phase webhook handler: verify, deduplicate, normalize, record, ACK.
pub mod receiver;

/// Framework-level HMAC verification and timing-safe comparison.
pub mod signature;

pub use receiver::{ReceiverError, WebhookNormalizer, WebhookOutcome, WebhookReceiver};
pub use signature::{
    EnvSecretStore, SecretStore, SignatureConfig, SignatureError, SignatureMethod,
    compute_hmac_sha256, constant_time_eq, verify_signature,
};
