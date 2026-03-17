use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::PaymentId;
use crate::payment::types::PaymentState;

/// Configuration for the reconciliation engine.
#[derive(Debug, Clone)]
pub struct ReconciliationConfig {
    /// How often the reconciliation loop runs (default: 5 minutes).
    pub interval: Duration,
    /// Per-provider confirmation windows. Discrepancies within this window
    /// are classified as timing delays rather than material mismatches.
    pub provider_confirmation_windows: HashMap<String, Duration>,
    /// Default confirmation window for providers not explicitly configured.
    pub default_confirmation_window: Duration,
}

impl Default for ReconciliationConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(300), // 5 minutes
            provider_confirmation_windows: HashMap::new(),
            default_confirmation_window: Duration::from_secs(60), // 1 minute
        }
    }
}

impl ReconciliationConfig {
    /// Returns the confirmation window for a given provider.
    pub fn confirmation_window_for(&self, provider: &str) -> Duration {
        self.provider_confirmation_windows
            .get(provider)
            .copied()
            .unwrap_or(self.default_confirmation_window)
    }
}

/// Category of discrepancy between optimistic and reconciled state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscrepancyCategory {
    /// Provider confirmation hasn't arrived yet but within expected window.
    TimingDelay,
    /// Optimistic and reconciled states disagree (e.g., Captured vs Failed).
    MaterialMismatch,
    /// Provider confirmation never arrived after timeout window.
    PermanentDivergence,
}

/// Status of a single payment's reconciliation check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconciliationStatus {
    /// Optimistic and reconciled states match.
    Matched,
    /// A discrepancy was detected.
    Discrepancy(DiscrepancyCategory),
    /// No reconciled state available (no provider events yet).
    Pending,
}

/// Result of reconciling a single payment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationResult {
    /// The payment being reconciled.
    pub payment_id: PaymentId,
    /// The provider this payment belongs to.
    pub provider: String,
    /// State as understood by the application (latest event).
    pub optimistic_state: PaymentState,
    /// State as confirmed by the provider (latest provider event), if any.
    pub reconciled_state: Option<PaymentState>,
    /// Result of the comparison.
    pub status: ReconciliationStatus,
    /// When this reconciliation check was performed.
    pub checked_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_5_minute_interval() {
        let config = ReconciliationConfig::default();
        assert_eq!(config.interval, Duration::from_secs(300));
    }

    #[test]
    fn default_confirmation_window_is_60_seconds() {
        let config = ReconciliationConfig::default();
        assert_eq!(config.default_confirmation_window, Duration::from_secs(60));
    }

    #[test]
    fn confirmation_window_uses_provider_specific_if_set() {
        let mut config = ReconciliationConfig::default();
        config
            .provider_confirmation_windows
            .insert("peach_payments".to_string(), Duration::from_secs(30));
        assert_eq!(
            config.confirmation_window_for("peach_payments"),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn confirmation_window_falls_back_to_default() {
        let config = ReconciliationConfig::default();
        assert_eq!(
            config.confirmation_window_for("unknown_provider"),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn discrepancy_category_serde_round_trip() {
        let json = serde_json::to_string(&DiscrepancyCategory::MaterialMismatch).unwrap();
        let parsed: DiscrepancyCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, DiscrepancyCategory::MaterialMismatch);
    }

    #[test]
    fn reconciliation_status_matched_is_not_discrepancy() {
        let status = ReconciliationStatus::Matched;
        assert_ne!(
            status,
            ReconciliationStatus::Discrepancy(DiscrepancyCategory::TimingDelay)
        );
    }
}
