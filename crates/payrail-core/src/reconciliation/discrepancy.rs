use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::PaymentId;
use crate::payment::types::{Money, PaymentState};

use super::types::DiscrepancyCategory;

/// Severity level of a discrepancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DiscrepancySeverity {
    /// Timing delays — expected to auto-resolve.
    Low,
    /// Material mismatch — requires investigation.
    Medium,
    /// Permanent divergence — high priority escalation.
    High,
}

impl From<DiscrepancyCategory> for DiscrepancySeverity {
    fn from(category: DiscrepancyCategory) -> Self {
        match category {
            DiscrepancyCategory::TimingDelay => DiscrepancySeverity::Low,
            DiscrepancyCategory::MaterialMismatch => DiscrepancySeverity::Medium,
            DiscrepancyCategory::PermanentDivergence => DiscrepancySeverity::High,
        }
    }
}

/// A detected discrepancy between optimistic and reconciled state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discrepancy {
    /// The payment with the discrepancy.
    pub payment_id: PaymentId,
    /// The provider this payment belongs to.
    pub provider: String,
    /// State as understood by the application.
    pub optimistic_state: PaymentState,
    /// State confirmed by provider, if any.
    pub reconciled_state: Option<PaymentState>,
    /// What kind of discrepancy.
    pub category: DiscrepancyCategory,
    /// Derived severity.
    pub severity: DiscrepancySeverity,
    /// When this discrepancy was first detected.
    pub detected_at: DateTime<Utc>,
    /// Payment amount for context.
    pub amount: Option<Money>,
}

/// How a discrepancy was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionType {
    /// System auto-resolved (e.g., provider confirmation arrived).
    AutoResolved,
    /// Escalated to operational summary.
    Escalated,
    /// Manually resolved by operator.
    ManuallyResolved,
}

/// Record of how a discrepancy was resolved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    /// The payment this resolution applies to.
    pub payment_id: PaymentId,
    /// How it was resolved.
    pub resolution_type: ResolutionType,
    /// When it was resolved.
    pub resolved_at: DateTime<Utc>,
    /// Human-readable details.
    pub details: String,
}

/// An escalation event for a discrepancy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escalation {
    /// The discrepancy being escalated.
    pub discrepancy: Discrepancy,
    /// Severity level.
    pub severity: DiscrepancySeverity,
    /// When escalated.
    pub escalated_at: DateTime<Utc>,
    /// Whether this requires investigation.
    pub requires_investigation: bool,
}

/// Trait for receiving escalation events. Pluggable backends.
pub trait EscalationSink: Send + Sync {
    /// Escalate a discrepancy.
    fn escalate(
        &self,
        escalation: &Escalation,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EscalationError>> + Send + '_>>;
}

/// Error from escalation sink.
#[derive(Debug, thiserror::Error)]
pub enum EscalationError {
    /// Escalation sink failed.
    #[error("RECONCILIATION_ESCALATION_FAILED: {0}")]
    SinkError(String),
}

/// In-memory escalation sink for testing.
#[derive(Debug, Default)]
pub struct InMemoryEscalationSink {
    escalations: std::sync::Mutex<Vec<Escalation>>,
}

impl InMemoryEscalationSink {
    /// Returns all recorded escalations.
    pub fn escalations(&self) -> Vec<Escalation> {
        self.escalations.lock().unwrap().clone()
    }
}

impl EscalationSink for InMemoryEscalationSink {
    fn escalate(
        &self,
        escalation: &Escalation,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EscalationError>> + Send + '_>>
    {
        self.escalations.lock().unwrap().push(escalation.clone());
        Box::pin(async { Ok(()) })
    }
}

/// Log-based escalation sink for production use.
#[derive(Debug, Default)]
pub struct LogEscalationSink;

impl EscalationSink for LogEscalationSink {
    fn escalate(
        &self,
        escalation: &Escalation,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EscalationError>> + Send + '_>>
    {
        // In production, this would log to structured logging
        eprintln!(
            "ESCALATION [{:?}] payment={} provider={} category={:?} severity={:?}",
            escalation.escalated_at,
            escalation.discrepancy.payment_id,
            escalation.discrepancy.provider,
            escalation.discrepancy.category,
            escalation.severity,
        );
        Box::pin(async { Ok(()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::PaymentId;

    #[test]
    fn severity_from_timing_delay_is_low() {
        assert_eq!(
            DiscrepancySeverity::from(DiscrepancyCategory::TimingDelay),
            DiscrepancySeverity::Low
        );
    }

    #[test]
    fn severity_from_material_mismatch_is_medium() {
        assert_eq!(
            DiscrepancySeverity::from(DiscrepancyCategory::MaterialMismatch),
            DiscrepancySeverity::Medium
        );
    }

    #[test]
    fn severity_from_permanent_divergence_is_high() {
        assert_eq!(
            DiscrepancySeverity::from(DiscrepancyCategory::PermanentDivergence),
            DiscrepancySeverity::High
        );
    }

    #[test]
    fn severity_ordering() {
        assert!(DiscrepancySeverity::Low < DiscrepancySeverity::Medium);
        assert!(DiscrepancySeverity::Medium < DiscrepancySeverity::High);
    }

    #[tokio::test]
    async fn in_memory_sink_records_escalations() {
        let sink = InMemoryEscalationSink::default();
        let escalation = Escalation {
            discrepancy: Discrepancy {
                payment_id: PaymentId::new(),
                provider: "test".to_string(),
                optimistic_state: PaymentState::Captured,
                reconciled_state: Some(PaymentState::Failed),
                category: DiscrepancyCategory::MaterialMismatch,
                severity: DiscrepancySeverity::Medium,
                detected_at: Utc::now(),
                amount: None,
            },
            severity: DiscrepancySeverity::Medium,
            escalated_at: Utc::now(),
            requires_investigation: true,
        };
        sink.escalate(&escalation).await.unwrap();
        assert_eq!(sink.escalations().len(), 1);
    }

    #[test]
    fn resolution_type_serde_round_trip() {
        let json = serde_json::to_string(&ResolutionType::AutoResolved).unwrap();
        let parsed: ResolutionType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ResolutionType::AutoResolved);
    }
}
