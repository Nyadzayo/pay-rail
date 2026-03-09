use std::fmt;

/// A deterministic idempotency key for deduplicating payment operations.
///
/// Keys use the format `{provider}:{merchant}:{scope}:{id}` and are
/// content-addressable: the same input always produces the same key.
///
/// # Examples
///
/// ```
/// use payrail_core::idempotency::IdempotencyKey;
///
/// let key = IdempotencyKey::generate("peach", "m123", "webhook", "evt_abc").unwrap();
/// assert_eq!(key.as_ref(), "peach:m123:webhook:evt_abc");
///
/// // Convenience for webhook scope
/// let key2 = IdempotencyKey::from_webhook("peach", "m123", "evt_abc").unwrap();
/// assert_eq!(key, key2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

/// Errors from idempotency key generation.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum KeyError {
    /// A segment was empty.
    #[error(
        "[IDEMPOTENCY_KEY_INVALID] Segment '{0}' is empty [Provide non-empty provider, merchant, scope, and id]"
    )]
    EmptySegment(&'static str),
    /// A segment contained a colon delimiter.
    #[error(
        "[IDEMPOTENCY_KEY_INVALID] Segment '{0}' contains ':' [Remove colons from individual key segments]"
    )]
    ColonInSegment(&'static str),
}

impl IdempotencyKey {
    /// Generate a key from four segments: `{provider}:{merchant}:{scope}:{id}`.
    ///
    /// Returns `KeyError` if any segment is empty or contains `:`.
    pub fn generate(
        provider: &str,
        merchant: &str,
        scope: &str,
        id: &str,
    ) -> Result<Self, KeyError> {
        Self::validate_segment(provider, "provider")?;
        Self::validate_segment(merchant, "merchant")?;
        Self::validate_segment(scope, "scope")?;
        Self::validate_segment(id, "id")?;
        Ok(Self(format!("{provider}:{merchant}:{scope}:{id}")))
    }

    /// Convenience constructor for webhook-scoped keys.
    ///
    /// Equivalent to `generate(provider, merchant, "webhook", event_id)`.
    pub fn from_webhook(provider: &str, merchant: &str, event_id: &str) -> Result<Self, KeyError> {
        Self::generate(provider, merchant, "webhook", event_id)
    }

    fn validate_segment(value: &str, name: &'static str) -> Result<(), KeyError> {
        if value.is_empty() {
            return Err(KeyError::EmptySegment(name));
        }
        if value.contains(':') {
            return Err(KeyError::ColonInSegment(name));
        }
        Ok(())
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for IdempotencyKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for IdempotencyKey {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<IdempotencyKey> for String {
    fn from(key: IdempotencyKey) -> Self {
        key.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_generation_deterministic() {
        let k1 = IdempotencyKey::generate("peach", "m123", "webhook", "evt_abc").unwrap();
        let k2 = IdempotencyKey::generate("peach", "m123", "webhook", "evt_abc").unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn key_format_correct() {
        let key = IdempotencyKey::generate("peach", "m123", "webhook", "evt_abc").unwrap();
        assert_eq!(key.as_ref(), "peach:m123:webhook:evt_abc");
        assert_eq!(key.to_string(), "peach:m123:webhook:evt_abc");
    }

    #[test]
    fn key_validation_rejects_empty_segments() {
        assert_eq!(
            IdempotencyKey::generate("", "m123", "webhook", "evt_abc"),
            Err(KeyError::EmptySegment("provider"))
        );
        assert_eq!(
            IdempotencyKey::generate("peach", "", "webhook", "evt_abc"),
            Err(KeyError::EmptySegment("merchant"))
        );
        assert_eq!(
            IdempotencyKey::generate("peach", "m123", "", "evt_abc"),
            Err(KeyError::EmptySegment("scope"))
        );
        assert_eq!(
            IdempotencyKey::generate("peach", "m123", "webhook", ""),
            Err(KeyError::EmptySegment("id"))
        );
    }

    #[test]
    fn key_validation_rejects_colon_in_segments() {
        assert_eq!(
            IdempotencyKey::generate("pe:ach", "m123", "webhook", "evt_abc"),
            Err(KeyError::ColonInSegment("provider"))
        );
        assert_eq!(
            IdempotencyKey::generate("peach", "m:123", "webhook", "evt_abc"),
            Err(KeyError::ColonInSegment("merchant"))
        );
    }

    #[test]
    fn key_from_webhook_convenience() {
        let k1 = IdempotencyKey::from_webhook("peach", "m123", "evt_abc").unwrap();
        let k2 = IdempotencyKey::generate("peach", "m123", "webhook", "evt_abc").unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn key_from_string_round_trip() {
        let original = IdempotencyKey::generate("peach", "m123", "webhook", "evt_abc").unwrap();
        let as_string: String = original.clone().into();
        let restored: IdempotencyKey = as_string.into();
        assert_eq!(original, restored);
    }

    // E2-UNIT-004: from_webhook format matches generate with "webhook" scope
    #[test]
    fn from_webhook_format_matches_generate() {
        let from_wh = IdempotencyKey::from_webhook("peach", "merchant1", "evt_123").unwrap();
        assert_eq!(
            from_wh.to_string(),
            "peach:merchant1:webhook:evt_123",
            "from_webhook key did not match expected format"
        );
        let from_gen =
            IdempotencyKey::generate("peach", "merchant1", "webhook", "evt_123").unwrap();
        assert_eq!(
            from_wh, from_gen,
            "from_webhook and generate should produce the same key"
        );
    }
}
