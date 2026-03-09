use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Verify threshold: facts with decayed confidence below this need re-verification.
pub const VERIFY_THRESHOLD: f64 = 0.7;

/// A confidence score clamped to the valid range `[0.0, 1.0]`.
///
/// Wraps an `f64` and enforces range validation on construction.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct ConfidenceScore(f64);

impl ConfidenceScore {
    /// Create a new confidence score. Returns `Err` if value is outside `[0.0, 1.0]`.
    pub fn new(value: f64) -> Result<Self, String> {
        if !(0.0..=1.0).contains(&value) {
            return Err(format!(
                "ConfidenceScore {value} is outside valid range 0.0..=1.0"
            ));
        }
        Ok(Self(value))
    }

    /// Returns the inner `f64` value.
    pub fn value(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for ConfidenceScore {
    type Error = String;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ConfidenceScore> for f64 {
    fn from(score: ConfidenceScore) -> Self {
        score.0
    }
}

impl fmt::Display for ConfidenceScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

/// Calculate the decayed confidence score given age in months.
///
/// Uses exponential decay: `original * (1.0 - decay_rate).powf(age_months)`.
/// The result is clamped to `[0.0, 1.0]`. Non-finite inputs return `0.0`.
pub fn decayed_score(original: f64, source: FactSource, age_months: f64) -> f64 {
    if !original.is_finite() || !age_months.is_finite() {
        return 0.0;
    }
    if age_months <= 0.0 {
        return original.clamp(0.0, 1.0);
    }
    let rate = source.decay_rate();
    let decayed = original * (1.0 - rate).powf(age_months);
    decayed.clamp(0.0, 1.0)
}

/// Check whether a fact needs re-verification based on temporal decay.
///
/// Returns `true` when the decayed score drops below [`VERIFY_THRESHOLD`].
pub fn needs_reverification(original: f64, source: FactSource, age_months: f64) -> bool {
    decayed_score(original, source, age_months) < VERIFY_THRESHOLD
}

/// Source of a knowledge pack fact, determining its base confidence score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FactSource {
    /// Confirmed via live sandbox API testing (base confidence: 0.95).
    SandboxTest,
    /// Extracted from current official provider documentation (base confidence: 0.85).
    OfficialDocs,
    /// From older versions of provider documentation (base confidence: 0.70).
    HistoricalDocs,
    /// From community forums, Stack Overflow, blog posts (base confidence: 0.65).
    CommunityReport,
    /// Deduced from patterns rather than directly documented (base confidence: 0.50).
    Inferred,
}

impl FactSource {
    /// Default confidence score for this source type.
    pub fn default_confidence(&self) -> f64 {
        match self {
            Self::SandboxTest => 0.95,
            Self::OfficialDocs => 0.85,
            Self::HistoricalDocs => 0.70,
            Self::CommunityReport => 0.65,
            Self::Inferred => 0.50,
        }
    }

    /// Monthly decay rate for this source type.
    pub fn decay_rate(&self) -> f64 {
        match self {
            Self::SandboxTest | Self::OfficialDocs | Self::HistoricalDocs => 0.05,
            Self::CommunityReport | Self::Inferred => 0.10,
        }
    }
}

/// A fact entry wrapping any value with confidence metadata and source attribution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FactEntry<T> {
    /// The fact value.
    pub value: T,
    /// Confidence score validated to `[0.0, 1.0]` at construction/deserialization.
    pub confidence_score: ConfidenceScore,
    /// Where this fact came from.
    pub source: FactSource,
    /// When this fact was last verified.
    pub verification_date: DateTime<Utc>,
    /// Monthly confidence decay rate (e.g., 0.05 = 5%/month).
    pub decay_rate: f64,
}

/// Provider metadata for a knowledge pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProviderMetadata {
    /// Provider identifier (kebab-case, e.g., "peach-payments").
    pub name: String,
    /// Human-readable display name (e.g., "Peach Payments").
    pub display_name: String,
    /// Knowledge pack version (e.g., "2026-03-06").
    pub version: String,
    /// Provider API base URL.
    pub base_url: String,
    /// Provider sandbox API URL.
    pub sandbox_url: String,
    /// Provider documentation URL.
    pub documentation_url: String,
}

/// A documented API endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EndpointFact {
    /// Endpoint URL path (e.g., "/v1/payments").
    pub url: String,
    /// HTTP method (e.g., "POST", "GET").
    pub method: String,
    /// Parameter names accepted by this endpoint.
    pub parameters: Vec<String>,
    /// Description of the response format.
    pub response_schema: String,
    /// Human-readable endpoint description.
    pub description: String,
}

/// A documented webhook event type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebhookEventFact {
    /// Event name (e.g., "charge.succeeded").
    pub event_name: String,
    /// Description of the webhook payload format.
    pub payload_schema: String,
    /// Conditions that trigger this event.
    pub trigger_conditions: String,
    /// Human-readable event description.
    pub description: String,
}

/// A mapping from provider status code to canonical payment state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StatusCodeMapping {
    /// Provider-specific status or result code.
    pub provider_code: String,
    /// Canonical PayRail payment state this maps to.
    pub canonical_state: String,
    /// Human-readable description of this mapping.
    pub description: String,
}

/// A documented provider error code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ErrorCodeFact {
    /// Provider error code (e.g., "800.100.151").
    pub code: String,
    /// What this error means.
    pub description: String,
    /// Recommended action to recover from this error.
    pub recovery_action: String,
}

/// A documented payment flow sequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PaymentFlowSequence {
    /// Flow name (e.g., "standard-checkout", "3ds-flow").
    pub name: String,
    /// Ordered steps in this flow.
    pub steps: Vec<String>,
    /// Human-readable flow description.
    pub description: String,
}

/// A complete knowledge pack for a payment provider.
///
/// This is the canonical schema for knowledge packs. YAML source files
/// (`pack.yaml`) and compiled JSON artifacts (`pack.json`) both conform
/// to this structure. Rust structs are the single source of truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KnowledgePack {
    /// Provider identification and metadata.
    pub metadata: ProviderMetadata,
    /// Documented API endpoints.
    pub endpoints: Vec<FactEntry<EndpointFact>>,
    /// Documented webhook event types.
    pub webhooks: Vec<FactEntry<WebhookEventFact>>,
    /// Status code to canonical state mappings.
    pub status_codes: Vec<FactEntry<StatusCodeMapping>>,
    /// Documented provider error codes.
    pub errors: Vec<FactEntry<ErrorCodeFact>>,
    /// Payment flow sequences.
    pub flows: Vec<FactEntry<PaymentFlowSequence>>,
}

impl<T> FactEntry<T> {
    /// Validate that decay_rate is within acceptable range.
    ///
    /// `confidence_score` is validated at construction via the `ConfidenceScore` newtype.
    /// Returns a list of validation error messages (empty if valid).
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if !(0.0..=1.0).contains(&self.decay_rate) {
            errors.push(format!(
                "decay_rate {} is outside valid range 0.0..=1.0",
                self.decay_rate
            ));
        }
        errors
    }

    /// Return the confidence score as an `f64` for arithmetic operations.
    pub fn confidence(&self) -> f64 {
        self.confidence_score.value()
    }
}

impl KnowledgePack {
    /// Validate all fact entries in the knowledge pack.
    /// Returns a list of validation error messages (empty if valid).
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (i, e) in self.endpoints.iter().enumerate() {
            for msg in e.validate() {
                errors.push(format!("endpoints[{i}]: {msg}"));
            }
        }
        for (i, e) in self.webhooks.iter().enumerate() {
            for msg in e.validate() {
                errors.push(format!("webhooks[{i}]: {msg}"));
            }
        }
        for (i, e) in self.status_codes.iter().enumerate() {
            for msg in e.validate() {
                errors.push(format!("status_codes[{i}]: {msg}"));
            }
        }
        for (i, e) in self.errors.iter().enumerate() {
            for msg in e.validate() {
                errors.push(format!("errors[{i}]: {msg}"));
            }
        }
        for (i, e) in self.flows.iter().enumerate() {
            for msg in e.validate() {
                errors.push(format!("flows[{i}]: {msg}"));
            }
        }
        errors
    }

    /// Create an empty knowledge pack scaffold for a provider.
    pub fn scaffold(name: &str, display_name: &str) -> Self {
        Self {
            metadata: ProviderMetadata {
                name: name.to_owned(),
                display_name: display_name.to_owned(),
                version: String::new(),
                base_url: String::new(),
                sandbox_url: String::new(),
                documentation_url: String::new(),
            },
            endpoints: Vec::new(),
            webhooks: Vec::new(),
            status_codes: Vec::new(),
            errors: Vec::new(),
            flows: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn cs(v: f64) -> ConfidenceScore {
        ConfidenceScore::new(v).unwrap()
    }

    fn sample_fact_entry() -> FactEntry<EndpointFact> {
        FactEntry {
            value: EndpointFact {
                url: "/v1/payments".to_owned(),
                method: "POST".to_owned(),
                parameters: vec!["amount".to_owned(), "currency".to_owned()],
                response_schema: "PaymentResponse JSON object".to_owned(),
                description: "Create a new payment".to_owned(),
            },
            confidence_score: cs(0.85),
            source: FactSource::OfficialDocs,
            verification_date: Utc.with_ymd_and_hms(2026, 3, 6, 12, 0, 0).unwrap(),
            decay_rate: 0.05,
        }
    }

    fn sample_knowledge_pack() -> KnowledgePack {
        let date = Utc.with_ymd_and_hms(2026, 3, 6, 12, 0, 0).unwrap();
        KnowledgePack {
            metadata: ProviderMetadata {
                name: "peach-payments".to_owned(),
                display_name: "Peach Payments".to_owned(),
                version: "2026-03-06".to_owned(),
                base_url: "https://oppwa.com/v1".to_owned(),
                sandbox_url: "https://eu-test.oppwa.com/v1".to_owned(),
                documentation_url: "https://developers.peachpayments.com".to_owned(),
            },
            endpoints: vec![sample_fact_entry()],
            webhooks: vec![FactEntry {
                value: WebhookEventFact {
                    event_name: "charge.succeeded".to_owned(),
                    payload_schema: "JSON with resultCode, id, amount".to_owned(),
                    trigger_conditions: "Successful charge completion".to_owned(),
                    description: "Fired when a charge succeeds".to_owned(),
                },
                confidence_score: cs(0.95),
                source: FactSource::SandboxTest,
                verification_date: date,
                decay_rate: 0.05,
            }],
            status_codes: vec![FactEntry {
                value: StatusCodeMapping {
                    provider_code: "000.100.110".to_owned(),
                    canonical_state: "Captured".to_owned(),
                    description: "Request successfully processed".to_owned(),
                },
                confidence_score: cs(0.95),
                source: FactSource::SandboxTest,
                verification_date: date,
                decay_rate: 0.05,
            }],
            errors: vec![FactEntry {
                value: ErrorCodeFact {
                    code: "800.100.151".to_owned(),
                    description: "Card expired".to_owned(),
                    recovery_action: "Request updated card details from customer".to_owned(),
                },
                confidence_score: cs(0.82),
                source: FactSource::CommunityReport,
                verification_date: date,
                decay_rate: 0.10,
            }],
            flows: vec![FactEntry {
                value: PaymentFlowSequence {
                    name: "standard-checkout".to_owned(),
                    steps: vec![
                        "Create payment intent".to_owned(),
                        "Authorize charge".to_owned(),
                        "Capture payment".to_owned(),
                    ],
                    description: "Standard one-step checkout flow".to_owned(),
                },
                confidence_score: cs(0.90),
                source: FactSource::OfficialDocs,
                verification_date: date,
                decay_rate: 0.05,
            }],
        }
    }

    #[test]
    fn fact_source_default_confidence() {
        assert_eq!(FactSource::SandboxTest.default_confidence(), 0.95);
        assert_eq!(FactSource::OfficialDocs.default_confidence(), 0.85);
        assert_eq!(FactSource::HistoricalDocs.default_confidence(), 0.70);
        assert_eq!(FactSource::CommunityReport.default_confidence(), 0.65);
        assert_eq!(FactSource::Inferred.default_confidence(), 0.50);
    }

    #[test]
    fn fact_source_decay_rates() {
        assert_eq!(FactSource::SandboxTest.decay_rate(), 0.05);
        assert_eq!(FactSource::OfficialDocs.decay_rate(), 0.05);
        assert_eq!(FactSource::HistoricalDocs.decay_rate(), 0.05);
        assert_eq!(FactSource::CommunityReport.decay_rate(), 0.10);
        assert_eq!(FactSource::Inferred.decay_rate(), 0.10);
    }

    #[test]
    fn fact_source_json_round_trip() {
        let sources = vec![
            FactSource::SandboxTest,
            FactSource::OfficialDocs,
            FactSource::HistoricalDocs,
            FactSource::CommunityReport,
            FactSource::Inferred,
        ];
        for source in &sources {
            let json = serde_json::to_string(source).unwrap();
            let deserialized: FactSource = serde_json::from_str(&json).unwrap();
            assert_eq!(*source, deserialized);
        }
    }

    #[test]
    fn fact_source_yaml_round_trip() {
        let sources = vec![
            FactSource::SandboxTest,
            FactSource::OfficialDocs,
            FactSource::HistoricalDocs,
            FactSource::CommunityReport,
            FactSource::Inferred,
        ];
        for source in &sources {
            let yaml = serde_yaml::to_string(source).unwrap();
            let deserialized: FactSource = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(*source, deserialized);
        }
    }

    #[test]
    fn fact_source_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&FactSource::SandboxTest).unwrap(),
            "\"sandbox_test\""
        );
        assert_eq!(
            serde_json::to_string(&FactSource::OfficialDocs).unwrap(),
            "\"official_docs\""
        );
        assert_eq!(
            serde_json::to_string(&FactSource::CommunityReport).unwrap(),
            "\"community_report\""
        );
    }

    #[test]
    fn fact_entry_json_round_trip() {
        let entry = sample_fact_entry();
        let json = serde_json::to_string_pretty(&entry).unwrap();
        let deserialized: FactEntry<EndpointFact> = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn fact_entry_yaml_round_trip() {
        let entry = sample_fact_entry();
        let yaml = serde_yaml::to_string(&entry).unwrap();
        let deserialized: FactEntry<EndpointFact> = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn knowledge_pack_json_round_trip() {
        let pack = sample_knowledge_pack();
        let json = serde_json::to_string_pretty(&pack).unwrap();
        let deserialized: KnowledgePack = serde_json::from_str(&json).unwrap();
        assert_eq!(pack, deserialized);
    }

    #[test]
    fn knowledge_pack_yaml_round_trip() {
        let pack = sample_knowledge_pack();
        let yaml = serde_yaml::to_string(&pack).unwrap();
        let deserialized: KnowledgePack = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(pack, deserialized);
    }

    #[test]
    fn empty_scaffold_round_trip() {
        let pack = KnowledgePack::scaffold("test-provider", "Test Provider");
        let json = serde_json::to_string(&pack).unwrap();
        let from_json: KnowledgePack = serde_json::from_str(&json).unwrap();
        assert_eq!(pack, from_json);

        let yaml = serde_yaml::to_string(&pack).unwrap();
        let from_yaml: KnowledgePack = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(pack, from_yaml);
    }

    #[test]
    fn empty_scaffold_has_empty_sections() {
        let pack = KnowledgePack::scaffold("test-provider", "Test Provider");
        assert!(pack.endpoints.is_empty());
        assert!(pack.webhooks.is_empty());
        assert!(pack.status_codes.is_empty());
        assert!(pack.errors.is_empty());
        assert!(pack.flows.is_empty());
        assert_eq!(pack.metadata.name, "test-provider");
        assert_eq!(pack.metadata.display_name, "Test Provider");
    }

    #[test]
    fn knowledge_pack_no_secret_fields() {
        // Verify that KnowledgePack struct field names contain no secret-like keys.
        // We check JSON keys (field names) only, not values, to avoid false positives
        // from legitimate data (e.g., an endpoint description mentioning "token").
        let pack = KnowledgePack::scaffold("test-provider", "Test Provider");
        let json = serde_json::to_string(&pack).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        fn check_keys(value: &serde_json::Value, path: &str) {
            match value {
                serde_json::Value::Object(map) => {
                    for (key, val) in map {
                        let lower_key = key.to_lowercase();
                        let full_path = format!("{path}.{key}");
                        assert!(
                            !lower_key.contains("api_key")
                                && !lower_key.contains("secret")
                                && !lower_key.contains("credential")
                                && !lower_key.contains("password"),
                            "Secret-like field name found at {full_path}"
                        );
                        check_keys(val, &full_path);
                    }
                }
                serde_json::Value::Array(arr) => {
                    for (i, val) in arr.iter().enumerate() {
                        check_keys(val, &format!("{path}[{i}]"));
                    }
                }
                _ => {}
            }
        }
        check_keys(&parsed, "root");
    }

    #[test]
    fn validate_fact_entry_valid() {
        let entry = sample_fact_entry();
        assert!(entry.validate().is_empty());
    }

    #[test]
    fn confidence_score_rejects_invalid_at_construction() {
        assert!(ConfidenceScore::new(1.5).is_err());
        assert!(ConfidenceScore::new(-0.1).is_err());
    }

    #[test]
    fn confidence_score_rejects_invalid_via_serde() {
        // ConfidenceScore in FactEntry rejects out-of-range values at deserialization
        let json = r#"{
            "value": {"url": "/v1/test", "method": "GET", "parameters": [], "response_schema": "", "description": ""},
            "confidence_score": 1.5,
            "source": "official_docs",
            "verification_date": "2026-03-06T12:00:00Z",
            "decay_rate": 0.05
        }"#;
        let result: Result<FactEntry<EndpointFact>, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "FactEntry must reject confidence_score > 1.0"
        );
    }

    #[test]
    fn validate_fact_entry_invalid_decay_rate() {
        let mut entry = sample_fact_entry();
        entry.decay_rate = -0.1;
        let errors = entry.validate();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("decay_rate"));
    }

    #[test]
    fn validate_knowledge_pack_catches_nested_errors() {
        let mut pack = sample_knowledge_pack();
        // confidence_score is now ConfidenceScore (type-enforced), so only decay_rate can be invalid
        pack.webhooks[0].decay_rate = -1.0;
        let errors = pack.validate();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("webhooks[0]"));
    }

    #[test]
    fn validate_empty_pack_is_valid() {
        let pack = KnowledgePack::scaffold("test", "Test");
        assert!(pack.validate().is_empty());
    }

    #[test]
    fn fact_source_is_mandatory_in_fact_entry() {
        // FactSource is not Optional — attempting to deserialize without it must fail.
        let json = r#"{
            "value": {"url": "/v1/test", "method": "GET", "parameters": [], "response_schema": "", "description": ""},
            "confidence_score": 0.85,
            "verification_date": "2026-03-06T12:00:00Z",
            "decay_rate": 0.05
        }"#;
        let result: Result<FactEntry<EndpointFact>, _> = serde_json::from_str(json);
        assert!(result.is_err(), "FactEntry must require source field");
    }

    // --- ConfidenceScore newtype tests ---

    #[test]
    fn confidence_score_valid_range() {
        assert!(ConfidenceScore::new(0.0).is_ok());
        assert!(ConfidenceScore::new(0.5).is_ok());
        assert!(ConfidenceScore::new(1.0).is_ok());
    }

    #[test]
    fn confidence_score_rejects_out_of_range() {
        assert!(ConfidenceScore::new(-0.01).is_err());
        assert!(ConfidenceScore::new(1.01).is_err());
        assert!(ConfidenceScore::new(-1.0).is_err());
        assert!(ConfidenceScore::new(2.0).is_err());
    }

    #[test]
    fn confidence_score_boundary_values() {
        let zero = ConfidenceScore::new(0.0).unwrap();
        assert_eq!(zero.value(), 0.0);
        let one = ConfidenceScore::new(1.0).unwrap();
        assert_eq!(one.value(), 1.0);
    }

    #[test]
    fn confidence_score_display() {
        let score = ConfidenceScore::new(0.85).unwrap();
        assert_eq!(format!("{score}"), "0.85");
    }

    #[test]
    fn confidence_score_serde_round_trip() {
        let score = ConfidenceScore::new(0.75).unwrap();
        let json = serde_json::to_string(&score).unwrap();
        let deserialized: ConfidenceScore = serde_json::from_str(&json).unwrap();
        assert_eq!(score, deserialized);
    }

    #[test]
    fn confidence_score_serde_rejects_invalid() {
        let json = "1.5";
        let result: Result<ConfidenceScore, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn confidence_score_ordering() {
        let low = ConfidenceScore::new(0.5).unwrap();
        let high = ConfidenceScore::new(0.9).unwrap();
        assert!(low < high);
    }

    // --- Temporal decay tests ---

    #[test]
    fn decayed_score_zero_age_returns_original() {
        assert_eq!(decayed_score(0.85, FactSource::OfficialDocs, 0.0), 0.85);
        assert_eq!(decayed_score(0.95, FactSource::SandboxTest, 0.0), 0.95);
    }

    #[test]
    fn decayed_score_negative_age_returns_original() {
        assert_eq!(decayed_score(0.85, FactSource::OfficialDocs, -1.0), 0.85);
    }

    #[test]
    fn decayed_score_official_docs_one_month() {
        // 0.85 * (1.0 - 0.05)^1 = 0.85 * 0.95 = 0.8075
        let result = decayed_score(0.85, FactSource::OfficialDocs, 1.0);
        assert!((result - 0.8075).abs() < 1e-10);
    }

    #[test]
    fn decayed_score_community_one_month() {
        // 0.65 * (1.0 - 0.10)^1 = 0.65 * 0.90 = 0.585
        let result = decayed_score(0.65, FactSource::CommunityReport, 1.0);
        assert!((result - 0.585).abs() < 1e-10);
    }

    #[test]
    fn decayed_score_six_months() {
        // 0.85 * (0.95)^6 ≈ 0.85 * 0.7351 ≈ 0.6248
        let result = decayed_score(0.85, FactSource::OfficialDocs, 6.0);
        assert!((result - 0.85 * 0.95_f64.powf(6.0)).abs() < 1e-10);
    }

    #[test]
    fn decayed_score_twelve_months() {
        let result = decayed_score(0.95, FactSource::SandboxTest, 12.0);
        let expected = 0.95 * 0.95_f64.powf(12.0);
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn decayed_score_community_twelve_months() {
        let result = decayed_score(0.65, FactSource::CommunityReport, 12.0);
        let expected = 0.65 * 0.90_f64.powf(12.0);
        assert!((result - expected).abs() < 1e-10);
    }

    #[test]
    fn decayed_score_clamped_to_zero() {
        // Very large age should result in near-zero but clamped to 0.0
        let result = decayed_score(0.50, FactSource::Inferred, 1000.0);
        assert!(result >= 0.0);
    }

    #[test]
    fn decayed_score_max_decay_community_vs_official() {
        // Community sources decay faster than official ones
        let official = decayed_score(0.85, FactSource::OfficialDocs, 6.0);
        let community = decayed_score(0.85, FactSource::CommunityReport, 6.0);
        assert!(official > community);
    }

    #[test]
    fn decayed_score_nan_returns_zero() {
        assert_eq!(decayed_score(f64::NAN, FactSource::OfficialDocs, 0.0), 0.0);
        assert_eq!(decayed_score(0.85, FactSource::OfficialDocs, f64::NAN), 0.0);
    }

    #[test]
    fn decayed_score_infinity_returns_zero() {
        assert_eq!(
            decayed_score(f64::INFINITY, FactSource::OfficialDocs, 0.0),
            0.0
        );
    }

    // --- Re-verification flag tests ---

    #[test]
    fn needs_reverification_fresh_official_docs_no() {
        // 0.85 at age 0 is above 0.7 threshold
        assert!(!needs_reverification(0.85, FactSource::OfficialDocs, 0.0));
    }

    #[test]
    fn needs_reverification_aged_official_docs_yes() {
        // After enough months, official docs drop below 0.7
        // 0.85 * 0.95^4 ≈ 0.6917 < 0.7
        assert!(needs_reverification(0.85, FactSource::OfficialDocs, 4.0));
    }

    #[test]
    fn needs_reverification_community_ages_faster() {
        // Community at 0.65 * 0.90^0 = 0.65 < 0.7 → already needs reverification
        assert!(needs_reverification(0.65, FactSource::CommunityReport, 0.0));
    }

    #[test]
    fn needs_reverification_sandbox_test_high_confidence() {
        // SandboxTest at 0.95 stays above 0.7 for a while
        assert!(!needs_reverification(0.95, FactSource::SandboxTest, 0.0));
        assert!(!needs_reverification(0.95, FactSource::SandboxTest, 3.0));
        // 0.95 * 0.95^6 ≈ 0.6984 < 0.7
        assert!(needs_reverification(0.95, FactSource::SandboxTest, 6.0));
    }

    #[test]
    fn needs_reverification_at_exact_threshold() {
        // Find exact boundary: score == VERIFY_THRESHOLD should NOT trigger
        // needs_reverification returns true when score < 0.7 (strictly less)
        // At exactly 0.7, no reverification needed
        assert!(!needs_reverification(0.7, FactSource::OfficialDocs, 0.0));
    }

    #[test]
    fn needs_reverification_just_below_threshold() {
        assert!(needs_reverification(0.699, FactSource::OfficialDocs, 0.0));
    }

    #[test]
    fn verify_threshold_value() {
        assert_eq!(VERIFY_THRESHOLD, 0.7);
    }
}
