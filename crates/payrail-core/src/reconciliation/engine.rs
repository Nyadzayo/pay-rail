use chrono::{DateTime, Utc};

use crate::event::store::EventStore;
use crate::event::types::{CanonicalEvent, EventType};
use crate::id::{EventId, PaymentId};
use crate::payment::types::{Currency, Money, PaymentState};

use super::discrepancy::{
    Discrepancy, DiscrepancySeverity, Escalation, EscalationSink, Resolution, ResolutionType,
};
use super::report::ReconciliationReport;
use super::types::{
    DiscrepancyCategory, ReconciliationConfig, ReconciliationResult, ReconciliationStatus,
};

/// Engine that compares optimistic vs reconciled state for payments.
///
/// Generic over `EventStore` to support different storage backends.
pub struct ReconciliationEngine<E: EventStore> {
    event_store: E,
    config: ReconciliationConfig,
}

impl<E: EventStore> ReconciliationEngine<E> {
    /// Creates a new reconciliation engine with the given event store and config.
    pub fn new(event_store: E, config: ReconciliationConfig) -> Self {
        Self {
            event_store,
            config,
        }
    }

    /// Returns a reference to the reconciliation config.
    pub fn config(&self) -> &ReconciliationConfig {
        &self.config
    }

    /// Reconciles a single payment by comparing optimistic vs reconciled state.
    pub async fn reconcile_payment(
        &self,
        payment_id: &PaymentId,
        provider: &str,
        now: DateTime<Utc>,
    ) -> Result<ReconciliationResult, crate::event::store::EventStoreError> {
        let optimistic = self.event_store.optimistic_state(payment_id).await?;
        let reconciled = self.event_store.reconciled_state(payment_id).await?;

        let optimistic_state = match optimistic {
            Some(s) => s,
            None => {
                return Ok(ReconciliationResult {
                    payment_id: payment_id.clone(),
                    provider: provider.to_string(),
                    optimistic_state: PaymentState::Created,
                    reconciled_state: None,
                    status: ReconciliationStatus::Pending,
                    checked_at: now,
                });
            }
        };

        let status = match reconciled {
            Some(reconciled_state) if reconciled_state == optimistic_state => {
                ReconciliationStatus::Matched
            }
            Some(reconciled_state) => {
                // States disagree — this is always a material mismatch
                // when both optimistic and reconciled states exist but differ
                let _ = reconciled_state; // used in status construction below
                ReconciliationStatus::Discrepancy(DiscrepancyCategory::MaterialMismatch)
            }
            None => {
                // No reconciled state — check if within confirmation window
                let events = self.event_store.query_by_payment_id(payment_id).await?;
                let last_app_event_time = events
                    .iter()
                    .filter(|e| e.provider == "app")
                    .map(|e| e.timestamp)
                    .max();

                match last_app_event_time {
                    Some(app_time) => {
                        let window = self.config.confirmation_window_for(provider);
                        let window_chrono = chrono::Duration::from_std(window)
                            .expect("confirmation window exceeds chrono::Duration range");
                        let deadline = app_time + window_chrono;
                        if now <= deadline {
                            ReconciliationStatus::Discrepancy(DiscrepancyCategory::TimingDelay)
                        } else {
                            ReconciliationStatus::Discrepancy(
                                DiscrepancyCategory::PermanentDivergence,
                            )
                        }
                    }
                    None => ReconciliationStatus::Pending,
                }
            }
        };

        let reconciled_state_value = reconciled;

        Ok(ReconciliationResult {
            payment_id: payment_id.clone(),
            provider: provider.to_string(),
            optimistic_state,
            reconciled_state: reconciled_state_value,
            status,
            checked_at: now,
        })
    }

    /// Reconciles all payments for a provider, returning results for each.
    pub async fn reconcile_provider(
        &self,
        provider: &str,
        payment_ids: &[PaymentId],
        now: DateTime<Utc>,
    ) -> Result<Vec<ReconciliationResult>, crate::event::store::EventStoreError> {
        let mut results = Vec::with_capacity(payment_ids.len());
        for id in payment_ids {
            let result = self.reconcile_payment(id, provider, now).await?;
            results.push(result);
        }
        Ok(results)
    }

    /// Extracts discrepancy objects from reconciliation results.
    pub fn extract_discrepancies(results: &[ReconciliationResult]) -> Vec<Discrepancy> {
        results
            .iter()
            .filter_map(|r| {
                if let ReconciliationStatus::Discrepancy(category) = r.status {
                    let severity = DiscrepancySeverity::from(category);
                    Some(Discrepancy {
                        payment_id: r.payment_id.clone(),
                        provider: r.provider.clone(),
                        optimistic_state: r.optimistic_state,
                        reconciled_state: r.reconciled_state,
                        category,
                        severity,
                        detected_at: r.checked_at,
                        amount: None,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Checks timing delay discrepancies for auto-resolution.
    /// Returns resolutions for delays where provider confirmation has arrived.
    pub async fn auto_resolve_timing_delays(
        &self,
        discrepancies: &[Discrepancy],
        now: DateTime<Utc>,
    ) -> Result<Vec<Resolution>, crate::event::store::EventStoreError> {
        let mut resolutions = Vec::new();
        for d in discrepancies {
            if d.category != DiscrepancyCategory::TimingDelay {
                continue;
            }
            // Re-check: has provider confirmation arrived since detection?
            let reconciled = self.event_store.reconciled_state(&d.payment_id).await?;
            if reconciled.is_some() {
                resolutions.push(Resolution {
                    payment_id: d.payment_id.clone(),
                    resolution_type: ResolutionType::AutoResolved,
                    resolved_at: now,
                    details: format!(
                        "Provider confirmation arrived. Reconciled state: {:?}",
                        reconciled
                    ),
                });
            }
        }
        Ok(resolutions)
    }

    /// Escalates material mismatches and permanent divergences to the given sink.
    pub async fn escalate_discrepancies<S: EscalationSink>(
        &self,
        discrepancies: &[Discrepancy],
        sink: &S,
        now: DateTime<Utc>,
    ) -> Result<Vec<Resolution>, super::discrepancy::EscalationError> {
        let mut resolutions = Vec::new();
        for d in discrepancies {
            match d.category {
                DiscrepancyCategory::TimingDelay => continue,
                DiscrepancyCategory::MaterialMismatch => {
                    let escalation = Escalation {
                        discrepancy: d.clone(),
                        severity: DiscrepancySeverity::Medium,
                        escalated_at: now,
                        requires_investigation: true,
                    };
                    sink.escalate(&escalation).await?;
                    resolutions.push(Resolution {
                        payment_id: d.payment_id.clone(),
                        resolution_type: ResolutionType::Escalated,
                        resolved_at: now,
                        details: format!(
                            "Material mismatch escalated: optimistic={}, reconciled={:?}",
                            d.optimistic_state, d.reconciled_state
                        ),
                    });
                }
                DiscrepancyCategory::PermanentDivergence => {
                    let escalation = Escalation {
                        discrepancy: d.clone(),
                        severity: DiscrepancySeverity::High,
                        escalated_at: now,
                        requires_investigation: true,
                    };
                    sink.escalate(&escalation).await?;
                    resolutions.push(Resolution {
                        payment_id: d.payment_id.clone(),
                        resolution_type: ResolutionType::Escalated,
                        resolved_at: now,
                        details: format!(
                            "Permanent divergence escalated as HIGH priority: payment {} has no provider confirmation after timeout",
                            d.payment_id
                        ),
                    });
                }
            }
        }
        Ok(resolutions)
    }

    /// Settles all matched payments by recording settlement events in the event store.
    /// Returns the number of payments successfully settled.
    pub async fn settle_matched_payments(
        &self,
        results: &[ReconciliationResult],
        now: DateTime<Utc>,
    ) -> Result<u64, crate::event::store::EventStoreError> {
        let mut settled_count = 0u64;

        for result in results {
            if result.status != ReconciliationStatus::Matched {
                continue;
            }

            // Only settle Captured or Refunded payments
            match result.optimistic_state {
                PaymentState::Captured | PaymentState::Refunded => {}
                _ => continue,
            }

            // Check if already settled
            let current = self
                .event_store
                .optimistic_state(&result.payment_id)
                .await?;
            if current == Some(PaymentState::Settled) {
                continue; // Idempotent: already settled
            }

            // Find the provider confirmation event for provenance
            let events = self
                .event_store
                .query_by_payment_id(&result.payment_id)
                .await?;
            let confirmation_event_id = events
                .iter()
                .filter(|e| e.provider != "app")
                .max_by_key(|e| e.timestamp)
                .map(|e| e.event_id.to_string())
                .unwrap_or_default();

            let settlement_event = CanonicalEvent {
                event_id: EventId::new(),
                event_type: EventType::new("payment.reconciliation.settled").unwrap(),
                payment_id: result.payment_id.clone(),
                provider: result.provider.clone(),
                timestamp: now,
                state_before: result.optimistic_state,
                state_after: PaymentState::Settled,
                amount: Money::new(0, Currency::ZAR), // Settlement events don't carry amount
                idempotency_key: format!("settle_{}_{}", result.payment_id, now.timestamp_millis()),
                raw_provider_payload: serde_json::json!({}),
                metadata: serde_json::json!({
                    "reconciliation_timestamp": now.to_rfc3339(),
                    "confirmation_event_id": confirmation_event_id,
                    "match_type": "exact_state_match"
                }),
            };

            self.event_store.append(&settlement_event).await?;
            settled_count += 1;
        }

        Ok(settled_count)
    }

    /// Generates a reconciliation report from cycle results.
    pub fn generate_report(
        provider: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        results: &[ReconciliationResult],
        resolutions: &[Resolution],
        settlements: u64,
    ) -> ReconciliationReport {
        ReconciliationReport::from_results(
            provider,
            period_start,
            period_end,
            results,
            resolutions,
            settlements,
        )
    }

    /// Full reconciliation cycle: reconcile, detect discrepancies, auto-resolve, escalate, settle.
    pub async fn reconcile_and_handle<S: EscalationSink>(
        &self,
        provider: &str,
        payment_ids: &[PaymentId],
        sink: &S,
        now: DateTime<Utc>,
    ) -> Result<ReconciliationCycleResult, ReconciliationCycleError> {
        let results = self
            .reconcile_provider(provider, payment_ids, now)
            .await
            .map_err(ReconciliationCycleError::EventStore)?;

        let discrepancies = Self::extract_discrepancies(&results);

        let auto_resolutions = self
            .auto_resolve_timing_delays(&discrepancies, now)
            .await
            .map_err(ReconciliationCycleError::EventStore)?;

        let escalation_resolutions = self
            .escalate_discrepancies(&discrepancies, sink, now)
            .await
            .map_err(ReconciliationCycleError::Escalation)?;

        let mut all_resolutions = auto_resolutions;
        all_resolutions.extend(escalation_resolutions);

        // Settle matched payments
        let settlements = self
            .settle_matched_payments(&results, now)
            .await
            .map_err(ReconciliationCycleError::EventStore)?;

        Ok(ReconciliationCycleResult {
            results,
            discrepancies,
            resolutions: all_resolutions,
            settlements,
        })
    }
}

/// Result of a full reconciliation cycle.
#[derive(Debug)]
pub struct ReconciliationCycleResult {
    /// Per-payment reconciliation results.
    pub results: Vec<ReconciliationResult>,
    /// Detected discrepancies.
    pub discrepancies: Vec<Discrepancy>,
    /// Resolutions applied this cycle (auto + escalations).
    pub resolutions: Vec<Resolution>,
    /// Number of payments settled this cycle.
    pub settlements: u64,
}

/// Errors from a full reconciliation cycle.
#[derive(Debug, thiserror::Error)]
pub enum ReconciliationCycleError {
    /// Event store error.
    #[error("RECONCILIATION_EVENT_STORE_ERROR: {0}")]
    EventStore(#[from] crate::event::store::EventStoreError),
    /// Escalation error.
    #[error("RECONCILIATION_ESCALATION_ERROR: {0}")]
    Escalation(super::discrepancy::EscalationError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::store::SqliteEventStore;
    use crate::event::types::{CanonicalEvent, EventType};
    use crate::id::EventId;
    use crate::payment::types::{Currency, Money};

    fn fixed_time() -> DateTime<Utc> {
        DateTime::from_timestamp_millis(1_700_000_000_000).unwrap()
    }

    fn later(base: DateTime<Utc>, secs: i64) -> DateTime<Utc> {
        base + chrono::Duration::seconds(secs)
    }

    fn test_payment_id() -> PaymentId {
        PaymentId::new()
    }

    fn make_event(
        payment_id: &PaymentId,
        provider: &str,
        state_before: PaymentState,
        state_after: PaymentState,
        timestamp: DateTime<Utc>,
    ) -> CanonicalEvent {
        CanonicalEvent {
            event_id: EventId::new(),
            event_type: EventType::new("payment.charge.captured").unwrap(),
            payment_id: payment_id.clone(),
            provider: provider.to_string(),
            timestamp,
            state_before,
            state_after,
            amount: Money::new(10000, Currency::ZAR),
            idempotency_key: format!("idem_{}", EventId::new()),
            raw_provider_payload: serde_json::json!({}),
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn reconcile_matching_states_returns_matched() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        // App event: Created → Captured
        let app_event = make_event(
            &pid,
            "app",
            PaymentState::Created,
            PaymentState::Captured,
            now,
        );
        store.append(&app_event).await.unwrap();

        // Provider event: Created → Captured
        let provider_event = make_event(
            &pid,
            "peach_payments",
            PaymentState::Created,
            PaymentState::Captured,
            later(now, 5),
        );
        store.append(&provider_event).await.unwrap();

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let result = engine
            .reconcile_payment(&pid, "peach_payments", later(now, 10))
            .await
            .unwrap();

        assert_eq!(result.status, ReconciliationStatus::Matched);
        assert_eq!(result.optimistic_state, PaymentState::Captured);
        assert_eq!(result.reconciled_state, Some(PaymentState::Captured));
    }

    #[tokio::test]
    async fn reconcile_material_mismatch_detected() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        // Provider confirms Failed first (e.g., webhook arrives)
        let provider_event = make_event(
            &pid,
            "peach_payments",
            PaymentState::Created,
            PaymentState::Failed,
            now,
        );
        store.append(&provider_event).await.unwrap();

        // App event is newer — app still thinks Captured
        // (e.g., app recorded its own state transition after the provider responded)
        let app_event = make_event(
            &pid,
            "app",
            PaymentState::Created,
            PaymentState::Captured,
            later(now, 5),
        );
        store.append(&app_event).await.unwrap();

        // optimistic_state = Captured (latest by timestamp = app event)
        // reconciled_state = Failed (latest provider event)
        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let result = engine
            .reconcile_payment(&pid, "peach_payments", later(now, 10))
            .await
            .unwrap();

        assert_eq!(
            result.status,
            ReconciliationStatus::Discrepancy(DiscrepancyCategory::MaterialMismatch)
        );
        assert_eq!(result.optimistic_state, PaymentState::Captured);
        assert_eq!(result.reconciled_state, Some(PaymentState::Failed));
    }

    #[tokio::test]
    async fn reconcile_timing_delay_within_window() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        // App event only — no provider confirmation yet
        let app_event = make_event(
            &pid,
            "app",
            PaymentState::Created,
            PaymentState::Captured,
            now,
        );
        store.append(&app_event).await.unwrap();

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        // Check within default 60-second window
        let result = engine
            .reconcile_payment(&pid, "peach_payments", later(now, 30))
            .await
            .unwrap();

        assert_eq!(
            result.status,
            ReconciliationStatus::Discrepancy(DiscrepancyCategory::TimingDelay)
        );
    }

    #[tokio::test]
    async fn reconcile_permanent_divergence_after_window() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        // App event only — no provider confirmation
        let app_event = make_event(
            &pid,
            "app",
            PaymentState::Created,
            PaymentState::Captured,
            now,
        );
        store.append(&app_event).await.unwrap();

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        // Check after 60-second window has elapsed
        let result = engine
            .reconcile_payment(&pid, "peach_payments", later(now, 120))
            .await
            .unwrap();

        assert_eq!(
            result.status,
            ReconciliationStatus::Discrepancy(DiscrepancyCategory::PermanentDivergence)
        );
    }

    #[tokio::test]
    async fn reconcile_no_events_returns_pending() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let result = engine
            .reconcile_payment(&pid, "peach_payments", now)
            .await
            .unwrap();

        assert_eq!(result.status, ReconciliationStatus::Pending);
    }

    #[tokio::test]
    async fn reconcile_provider_returns_all_results() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let now = fixed_time();

        let pid1 = test_payment_id();
        let pid2 = test_payment_id();

        // Both have matching states
        for pid in [&pid1, &pid2] {
            let app_event = make_event(
                pid,
                "app",
                PaymentState::Created,
                PaymentState::Captured,
                now,
            );
            store.append(&app_event).await.unwrap();

            let provider_event = make_event(
                pid,
                "peach_payments",
                PaymentState::Created,
                PaymentState::Captured,
                later(now, 5),
            );
            store.append(&provider_event).await.unwrap();
        }

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let results = engine
            .reconcile_provider("peach_payments", &[pid1, pid2], later(now, 10))
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|r| r.status == ReconciliationStatus::Matched)
        );
    }

    #[tokio::test]
    async fn reconcile_uses_provider_specific_window() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        // App event only
        let app_event = make_event(
            &pid,
            "app",
            PaymentState::Created,
            PaymentState::Captured,
            now,
        );
        store.append(&app_event).await.unwrap();

        // Configure 10-second window for peach
        let mut config = ReconciliationConfig::default();
        config.provider_confirmation_windows.insert(
            "peach_payments".to_string(),
            std::time::Duration::from_secs(10),
        );

        let engine = ReconciliationEngine::new(store, config);

        // At 5 seconds: still within 10-second window → timing delay
        let result = engine
            .reconcile_payment(&pid, "peach_payments", later(now, 5))
            .await
            .unwrap();
        assert_eq!(
            result.status,
            ReconciliationStatus::Discrepancy(DiscrepancyCategory::TimingDelay)
        );
    }

    // ========== Story 9.2: Discrepancy detection & auto-resolution ==========

    #[test]
    fn extract_discrepancies_from_results() {
        let now = Utc::now();
        let results = vec![
            ReconciliationResult {
                payment_id: PaymentId::new(),
                provider: "test".to_string(),
                optimistic_state: PaymentState::Captured,
                reconciled_state: Some(PaymentState::Captured),
                status: ReconciliationStatus::Matched,
                checked_at: now,
            },
            ReconciliationResult {
                payment_id: PaymentId::new(),
                provider: "test".to_string(),
                optimistic_state: PaymentState::Captured,
                reconciled_state: None,
                status: ReconciliationStatus::Discrepancy(DiscrepancyCategory::TimingDelay),
                checked_at: now,
            },
            ReconciliationResult {
                payment_id: PaymentId::new(),
                provider: "test".to_string(),
                optimistic_state: PaymentState::Captured,
                reconciled_state: Some(PaymentState::Failed),
                status: ReconciliationStatus::Discrepancy(DiscrepancyCategory::MaterialMismatch),
                checked_at: now,
            },
        ];

        let discrepancies =
            ReconciliationEngine::<SqliteEventStore>::extract_discrepancies(&results);
        assert_eq!(discrepancies.len(), 2);
        assert_eq!(discrepancies[0].category, DiscrepancyCategory::TimingDelay);
        assert_eq!(discrepancies[0].severity, DiscrepancySeverity::Low);
        assert_eq!(
            discrepancies[1].category,
            DiscrepancyCategory::MaterialMismatch
        );
        assert_eq!(discrepancies[1].severity, DiscrepancySeverity::Medium);
    }

    #[tokio::test]
    async fn auto_resolve_timing_delay_when_confirmation_arrives() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        // App event
        let app_event = make_event(
            &pid,
            "app",
            PaymentState::Created,
            PaymentState::Captured,
            now,
        );
        store.append(&app_event).await.unwrap();

        // Initially no provider event → timing delay
        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let result = engine
            .reconcile_payment(&pid, "peach", later(now, 5))
            .await
            .unwrap();
        assert_eq!(
            result.status,
            ReconciliationStatus::Discrepancy(DiscrepancyCategory::TimingDelay)
        );

        // Now provider confirmation arrives
        let provider_event = make_event(
            &pid,
            "peach",
            PaymentState::Created,
            PaymentState::Captured,
            later(now, 10),
        );
        engine.event_store.append(&provider_event).await.unwrap();

        // Create discrepancy from the earlier result
        let discrepancy = Discrepancy {
            payment_id: pid.clone(),
            provider: "peach".to_string(),
            optimistic_state: PaymentState::Captured,
            reconciled_state: None,
            category: DiscrepancyCategory::TimingDelay,
            severity: DiscrepancySeverity::Low,
            detected_at: later(now, 5),
            amount: None,
        };

        let resolutions = engine
            .auto_resolve_timing_delays(&[discrepancy], later(now, 15))
            .await
            .unwrap();
        assert_eq!(resolutions.len(), 1);
        assert_eq!(resolutions[0].resolution_type, ResolutionType::AutoResolved);
    }

    #[tokio::test]
    async fn timing_delay_stays_pending_without_confirmation() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        // App event only
        let app_event = make_event(
            &pid,
            "app",
            PaymentState::Created,
            PaymentState::Captured,
            now,
        );
        store.append(&app_event).await.unwrap();

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());

        let discrepancy = Discrepancy {
            payment_id: pid.clone(),
            provider: "peach".to_string(),
            optimistic_state: PaymentState::Captured,
            reconciled_state: None,
            category: DiscrepancyCategory::TimingDelay,
            severity: DiscrepancySeverity::Low,
            detected_at: now,
            amount: None,
        };

        let resolutions = engine
            .auto_resolve_timing_delays(&[discrepancy], later(now, 5))
            .await
            .unwrap();
        assert!(resolutions.is_empty());
    }

    #[tokio::test]
    async fn escalate_material_mismatch() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let sink = super::super::discrepancy::InMemoryEscalationSink::default();
        let now = fixed_time();

        let discrepancy = Discrepancy {
            payment_id: PaymentId::new(),
            provider: "test".to_string(),
            optimistic_state: PaymentState::Captured,
            reconciled_state: Some(PaymentState::Failed),
            category: DiscrepancyCategory::MaterialMismatch,
            severity: DiscrepancySeverity::Medium,
            detected_at: now,
            amount: None,
        };

        let resolutions = engine
            .escalate_discrepancies(&[discrepancy], &sink, now)
            .await
            .unwrap();

        assert_eq!(resolutions.len(), 1);
        assert_eq!(resolutions[0].resolution_type, ResolutionType::Escalated);
        assert_eq!(sink.escalations().len(), 1);
        assert_eq!(sink.escalations()[0].severity, DiscrepancySeverity::Medium);
        assert!(sink.escalations()[0].requires_investigation);
    }

    #[tokio::test]
    async fn escalate_permanent_divergence_as_high_priority() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let sink = super::super::discrepancy::InMemoryEscalationSink::default();
        let now = fixed_time();

        let discrepancy = Discrepancy {
            payment_id: PaymentId::new(),
            provider: "test".to_string(),
            optimistic_state: PaymentState::Captured,
            reconciled_state: None,
            category: DiscrepancyCategory::PermanentDivergence,
            severity: DiscrepancySeverity::High,
            detected_at: now,
            amount: None,
        };

        let resolutions = engine
            .escalate_discrepancies(&[discrepancy], &sink, now)
            .await
            .unwrap();

        assert_eq!(resolutions.len(), 1);
        assert_eq!(sink.escalations()[0].severity, DiscrepancySeverity::High);
    }

    #[tokio::test]
    async fn timing_delays_not_escalated() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let sink = super::super::discrepancy::InMemoryEscalationSink::default();
        let now = fixed_time();

        let discrepancy = Discrepancy {
            payment_id: PaymentId::new(),
            provider: "test".to_string(),
            optimistic_state: PaymentState::Captured,
            reconciled_state: None,
            category: DiscrepancyCategory::TimingDelay,
            severity: DiscrepancySeverity::Low,
            detected_at: now,
            amount: None,
        };

        let resolutions = engine
            .escalate_discrepancies(&[discrepancy], &sink, now)
            .await
            .unwrap();

        assert!(resolutions.is_empty());
        assert!(sink.escalations().is_empty());
    }

    #[tokio::test]
    async fn full_reconcile_and_handle_cycle() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let now = fixed_time();

        let pid_matched = test_payment_id();
        let pid_timing = test_payment_id();
        let pid_mismatch = test_payment_id();

        // Matched payment: both app and provider agree
        store
            .append(&make_event(
                &pid_matched,
                "app",
                PaymentState::Created,
                PaymentState::Captured,
                now,
            ))
            .await
            .unwrap();
        store
            .append(&make_event(
                &pid_matched,
                "peach",
                PaymentState::Created,
                PaymentState::Captured,
                later(now, 1),
            ))
            .await
            .unwrap();

        // Timing delay: app only, within window
        store
            .append(&make_event(
                &pid_timing,
                "app",
                PaymentState::Created,
                PaymentState::Captured,
                now,
            ))
            .await
            .unwrap();

        // Material mismatch: provider disagrees (provider first, then app newer)
        store
            .append(&make_event(
                &pid_mismatch,
                "peach",
                PaymentState::Created,
                PaymentState::Failed,
                now,
            ))
            .await
            .unwrap();
        store
            .append(&make_event(
                &pid_mismatch,
                "app",
                PaymentState::Created,
                PaymentState::Captured,
                later(now, 1),
            ))
            .await
            .unwrap();

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let sink = super::super::discrepancy::InMemoryEscalationSink::default();

        let cycle = engine
            .reconcile_and_handle(
                "peach",
                &[pid_matched.clone(), pid_timing, pid_mismatch],
                &sink,
                later(now, 5),
            )
            .await
            .unwrap();

        assert_eq!(cycle.results.len(), 3);
        assert_eq!(cycle.discrepancies.len(), 2); // timing + mismatch
        assert_eq!(cycle.settlements, 1); // only the matched payment settles
        // Material mismatch should be escalated
        assert_eq!(sink.escalations().len(), 1);
        assert_eq!(sink.escalations()[0].severity, DiscrepancySeverity::Medium);

        // Verify the matched payment is now settled
        let settled_state = engine
            .event_store
            .optimistic_state(&pid_matched)
            .await
            .unwrap();
        assert_eq!(settled_state, Some(PaymentState::Settled));
    }

    // ========== Story 9.3: Settlement transition & reports ==========

    #[tokio::test]
    async fn settle_matched_captured_payment() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        // Both agree on Captured
        store
            .append(&make_event(
                &pid,
                "app",
                PaymentState::Created,
                PaymentState::Captured,
                now,
            ))
            .await
            .unwrap();
        store
            .append(&make_event(
                &pid,
                "peach",
                PaymentState::Created,
                PaymentState::Captured,
                later(now, 1),
            ))
            .await
            .unwrap();

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let results = engine
            .reconcile_provider("peach", std::slice::from_ref(&pid), later(now, 5))
            .await
            .unwrap();

        let settled = engine
            .settle_matched_payments(&results, later(now, 10))
            .await
            .unwrap();

        assert_eq!(settled, 1);

        // Verify settlement event was recorded
        let events = engine.event_store.query_by_payment_id(&pid).await.unwrap();
        let settlement_event = events
            .iter()
            .find(|e| e.state_after == PaymentState::Settled)
            .expect("settlement event should exist");
        assert_eq!(
            settlement_event.event_type.as_str(),
            "payment.reconciliation.settled"
        );
        assert_eq!(settlement_event.state_before, PaymentState::Captured);

        // Verify metadata has provenance
        let metadata = &settlement_event.metadata;
        assert!(metadata.get("reconciliation_timestamp").is_some());
        assert!(metadata.get("confirmation_event_id").is_some());
        assert_eq!(metadata.get("match_type").unwrap(), "exact_state_match");
    }

    #[tokio::test]
    async fn settle_matched_refunded_payment() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        // Both agree on Refunded
        store
            .append(&make_event(
                &pid,
                "app",
                PaymentState::Captured,
                PaymentState::Refunded,
                now,
            ))
            .await
            .unwrap();
        store
            .append(&make_event(
                &pid,
                "peach",
                PaymentState::Captured,
                PaymentState::Refunded,
                later(now, 1),
            ))
            .await
            .unwrap();

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let results = engine
            .reconcile_provider("peach", std::slice::from_ref(&pid), later(now, 5))
            .await
            .unwrap();

        let settled = engine
            .settle_matched_payments(&results, later(now, 10))
            .await
            .unwrap();
        assert_eq!(settled, 1);

        let final_state = engine.event_store.optimistic_state(&pid).await.unwrap();
        assert_eq!(final_state, Some(PaymentState::Settled));
    }

    #[tokio::test]
    async fn settle_is_idempotent() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        store
            .append(&make_event(
                &pid,
                "app",
                PaymentState::Created,
                PaymentState::Captured,
                now,
            ))
            .await
            .unwrap();
        store
            .append(&make_event(
                &pid,
                "peach",
                PaymentState::Created,
                PaymentState::Captured,
                later(now, 1),
            ))
            .await
            .unwrap();

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let results = engine
            .reconcile_provider("peach", std::slice::from_ref(&pid), later(now, 5))
            .await
            .unwrap();

        // First settlement
        let settled1 = engine
            .settle_matched_payments(&results, later(now, 10))
            .await
            .unwrap();
        assert_eq!(settled1, 1);

        // Second settlement — should be 0 (already settled)
        let settled2 = engine
            .settle_matched_payments(&results, later(now, 20))
            .await
            .unwrap();
        assert_eq!(settled2, 0);
    }

    #[tokio::test]
    async fn settle_skips_non_captured_non_refunded() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = test_payment_id();
        let now = fixed_time();

        // Both agree on Authorized (not settleable)
        store
            .append(&make_event(
                &pid,
                "app",
                PaymentState::Created,
                PaymentState::Authorized,
                now,
            ))
            .await
            .unwrap();
        store
            .append(&make_event(
                &pid,
                "peach",
                PaymentState::Created,
                PaymentState::Authorized,
                later(now, 1),
            ))
            .await
            .unwrap();

        let engine = ReconciliationEngine::new(store, ReconciliationConfig::default());
        let results = engine
            .reconcile_provider("peach", &[pid], later(now, 5))
            .await
            .unwrap();

        let settled = engine
            .settle_matched_payments(&results, later(now, 10))
            .await
            .unwrap();
        assert_eq!(settled, 0); // Authorized can't be settled
    }

    #[test]
    fn generate_report_from_cycle() {
        let now = fixed_time();
        let results = vec![
            ReconciliationResult {
                payment_id: PaymentId::new(),
                provider: "peach".to_string(),
                optimistic_state: PaymentState::Captured,
                reconciled_state: Some(PaymentState::Captured),
                status: ReconciliationStatus::Matched,
                checked_at: now,
            },
            ReconciliationResult {
                payment_id: PaymentId::new(),
                provider: "peach".to_string(),
                optimistic_state: PaymentState::Captured,
                reconciled_state: None,
                status: ReconciliationStatus::Discrepancy(DiscrepancyCategory::TimingDelay),
                checked_at: now,
            },
        ];

        let report = ReconciliationEngine::<SqliteEventStore>::generate_report(
            "peach",
            now,
            later(now, 3600),
            &results,
            &[],
            1,
        );

        assert_eq!(report.provider, "peach");
        assert_eq!(report.total_payments, 2);
        assert_eq!(report.matched_count, 1);
        assert!((report.match_rate - 50.0).abs() < 0.01);
        assert_eq!(report.settlements, 1);
    }
}
