use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::discrepancy::{Resolution, ResolutionType};
use super::types::{DiscrepancyCategory, ReconciliationResult, ReconciliationStatus};

/// Breakdown of discrepancies by category.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscrepancyBreakdown {
    /// Count of timing delay discrepancies.
    pub timing_delay_count: u64,
    /// Count of material mismatch discrepancies.
    pub material_mismatch_count: u64,
    /// Count of permanent divergence discrepancies.
    pub permanent_divergence_count: u64,
}

/// Summary of how discrepancies were resolved.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolutionSummary {
    /// Number auto-resolved by the system.
    pub auto_resolved_count: u64,
    /// Number escalated for investigation.
    pub escalated_count: u64,
    /// Number manually resolved by operator.
    pub manually_resolved_count: u64,
}

/// A reconciliation report for a single provider over a time period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationReport {
    /// Provider name.
    pub provider: String,
    /// Start of the reporting period.
    pub period_start: DateTime<Utc>,
    /// End of the reporting period.
    pub period_end: DateTime<Utc>,
    /// Total payments checked.
    pub total_payments: u64,
    /// Number of payments where states matched.
    pub matched_count: u64,
    /// Match rate as percentage (0.0 - 100.0).
    pub match_rate: f64,
    /// Discrepancy breakdown by category.
    pub discrepancies: DiscrepancyBreakdown,
    /// Resolution summary.
    pub resolutions: ResolutionSummary,
    /// Number of payments transitioned to Settled.
    pub settlements: u64,
}

impl ReconciliationReport {
    /// Generates a report from reconciliation results and resolutions.
    pub fn from_results(
        provider: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        results: &[ReconciliationResult],
        resolutions: &[Resolution],
        settlements: u64,
    ) -> Self {
        let total_payments = results.len() as u64;
        let mut matched_count = 0u64;
        let mut breakdown = DiscrepancyBreakdown::default();

        for result in results {
            match &result.status {
                ReconciliationStatus::Matched => matched_count += 1,
                ReconciliationStatus::Discrepancy(category) => match category {
                    DiscrepancyCategory::TimingDelay => breakdown.timing_delay_count += 1,
                    DiscrepancyCategory::MaterialMismatch => breakdown.material_mismatch_count += 1,
                    DiscrepancyCategory::PermanentDivergence => {
                        breakdown.permanent_divergence_count += 1
                    }
                },
                ReconciliationStatus::Pending => {} // Not counted in discrepancies
            }
        }

        let match_rate = if total_payments > 0 {
            (matched_count as f64 / total_payments as f64) * 100.0
        } else {
            100.0
        };

        let mut resolution_summary = ResolutionSummary::default();
        for r in resolutions {
            match r.resolution_type {
                ResolutionType::AutoResolved => resolution_summary.auto_resolved_count += 1,
                ResolutionType::Escalated => resolution_summary.escalated_count += 1,
                ResolutionType::ManuallyResolved => resolution_summary.manually_resolved_count += 1,
            }
        }

        Self {
            provider: provider.to_string(),
            period_start,
            period_end,
            total_payments,
            matched_count,
            match_rate,
            discrepancies: breakdown,
            resolutions: resolution_summary,
            settlements,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::PaymentId;
    use crate::payment::types::PaymentState;

    fn fixed_time() -> DateTime<Utc> {
        DateTime::from_timestamp_millis(1_700_000_000_000).unwrap()
    }

    fn later(base: DateTime<Utc>, secs: i64) -> DateTime<Utc> {
        base + chrono::Duration::seconds(secs)
    }

    fn make_result(status: ReconciliationStatus) -> ReconciliationResult {
        ReconciliationResult {
            payment_id: PaymentId::new(),
            provider: "test".to_string(),
            optimistic_state: PaymentState::Captured,
            reconciled_state: Some(PaymentState::Captured),
            status,
            checked_at: fixed_time(),
        }
    }

    #[test]
    fn report_calculates_match_rate() {
        let now = fixed_time();
        let results = vec![
            make_result(ReconciliationStatus::Matched),
            make_result(ReconciliationStatus::Matched),
            make_result(ReconciliationStatus::Matched),
            make_result(ReconciliationStatus::Discrepancy(
                DiscrepancyCategory::TimingDelay,
            )),
        ];

        let report =
            ReconciliationReport::from_results("test", now, later(now, 3600), &results, &[], 3);
        assert_eq!(report.total_payments, 4);
        assert_eq!(report.matched_count, 3);
        assert!((report.match_rate - 75.0).abs() < 0.01);
        assert_eq!(report.discrepancies.timing_delay_count, 1);
        assert_eq!(report.settlements, 3);
    }

    #[test]
    fn report_empty_results_100_percent_match() {
        let now = fixed_time();
        let report = ReconciliationReport::from_results("test", now, later(now, 3600), &[], &[], 0);
        assert_eq!(report.total_payments, 0);
        assert!((report.match_rate - 100.0).abs() < 0.01);
    }

    #[test]
    fn report_counts_discrepancy_categories() {
        let now = fixed_time();
        let results = vec![
            make_result(ReconciliationStatus::Discrepancy(
                DiscrepancyCategory::TimingDelay,
            )),
            make_result(ReconciliationStatus::Discrepancy(
                DiscrepancyCategory::TimingDelay,
            )),
            make_result(ReconciliationStatus::Discrepancy(
                DiscrepancyCategory::MaterialMismatch,
            )),
            make_result(ReconciliationStatus::Discrepancy(
                DiscrepancyCategory::PermanentDivergence,
            )),
        ];

        let report =
            ReconciliationReport::from_results("test", now, later(now, 3600), &results, &[], 0);
        assert_eq!(report.discrepancies.timing_delay_count, 2);
        assert_eq!(report.discrepancies.material_mismatch_count, 1);
        assert_eq!(report.discrepancies.permanent_divergence_count, 1);
    }

    #[test]
    fn report_counts_resolution_types() {
        let now = fixed_time();
        let resolutions = vec![
            Resolution {
                payment_id: PaymentId::new(),
                resolution_type: ResolutionType::AutoResolved,
                resolved_at: now,
                details: "auto".to_string(),
            },
            Resolution {
                payment_id: PaymentId::new(),
                resolution_type: ResolutionType::Escalated,
                resolved_at: now,
                details: "escalated".to_string(),
            },
            Resolution {
                payment_id: PaymentId::new(),
                resolution_type: ResolutionType::Escalated,
                resolved_at: now,
                details: "escalated".to_string(),
            },
        ];

        let report =
            ReconciliationReport::from_results("test", now, later(now, 3600), &[], &resolutions, 0);
        assert_eq!(report.resolutions.auto_resolved_count, 1);
        assert_eq!(report.resolutions.escalated_count, 2);
        assert_eq!(report.resolutions.manually_resolved_count, 0);
    }

    #[test]
    fn report_serializes_to_json() {
        let now = fixed_time();
        let results = vec![
            make_result(ReconciliationStatus::Matched),
            make_result(ReconciliationStatus::Discrepancy(
                DiscrepancyCategory::TimingDelay,
            )),
        ];
        let report = ReconciliationReport::from_results(
            "peach_payments",
            now,
            later(now, 3600),
            &results,
            &[],
            1,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"provider\":\"peach_payments\""));
        assert!(json.contains("\"total_payments\":2"));
        assert!(json.contains("\"matched_count\":1"));
        assert!(json.contains("\"settlements\":1"));
    }
}
