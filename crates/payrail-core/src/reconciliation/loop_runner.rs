use std::time::Instant;

use chrono::Utc;
use tokio::sync::watch;

use crate::event::store::EventStore;
use crate::id::PaymentId;

use super::engine::ReconciliationEngine;
use super::types::ReconciliationResult;

/// Outcome of a single reconciliation loop iteration.
#[derive(Debug)]
pub struct LoopIterationResult {
    /// Results from this iteration.
    pub results: Vec<ReconciliationResult>,
    /// How long the iteration took.
    pub duration: std::time::Duration,
    /// The provider that was reconciled.
    pub provider: String,
}

/// A payment ID source that the loop queries each iteration to get active payments.
pub trait PaymentIdSource: Send + Sync {
    /// Returns payment IDs to reconcile for a given provider.
    fn payment_ids_for_provider(
        &self,
        provider: &str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Vec<PaymentId>, crate::event::store::EventStoreError>,
                > + Send
                + '_,
        >,
    >;
}

/// Runs the reconciliation engine at configured intervals.
pub struct ReconciliationLoop<E: EventStore, P: PaymentIdSource> {
    engine: ReconciliationEngine<E>,
    payment_source: P,
    providers: Vec<String>,
    shutdown_rx: watch::Receiver<bool>,
}

impl<E: EventStore, P: PaymentIdSource> ReconciliationLoop<E, P> {
    /// Creates a new reconciliation loop.
    pub fn new(
        engine: ReconciliationEngine<E>,
        payment_source: P,
        providers: Vec<String>,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            engine,
            payment_source,
            providers,
            shutdown_rx,
        }
    }

    /// Runs a single iteration of reconciliation for all providers.
    pub async fn run_once(
        &self,
    ) -> Result<Vec<LoopIterationResult>, crate::event::store::EventStoreError> {
        let mut all_results = Vec::new();
        let now = Utc::now();

        for provider in &self.providers {
            let start = Instant::now();
            let payment_ids = self
                .payment_source
                .payment_ids_for_provider(provider)
                .await?;

            let results = self
                .engine
                .reconcile_provider(provider, &payment_ids, now)
                .await?;

            let duration = start.elapsed();
            all_results.push(LoopIterationResult {
                results,
                duration,
                provider: provider.clone(),
            });
        }

        Ok(all_results)
    }

    /// Runs the reconciliation loop continuously until shutdown signal.
    pub async fn run(&mut self) -> Result<(), crate::event::store::EventStoreError> {
        let interval = self.engine.config().interval;

        loop {
            self.run_once().await?;

            // Wait for interval or shutdown signal
            let sleep = tokio::time::sleep(interval);
            tokio::pin!(sleep);

            tokio::select! {
                _ = &mut sleep => {
                    // Interval elapsed, continue to next iteration
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::store::SqliteEventStore;
    use crate::event::types::{CanonicalEvent, EventType};
    use crate::id::EventId;
    use crate::payment::types::{Currency, Money, PaymentState};
    use crate::reconciliation::types::ReconciliationConfig;

    struct StaticPaymentSource {
        ids: Vec<PaymentId>,
    }

    impl PaymentIdSource for StaticPaymentSource {
        fn payment_ids_for_provider(
            &self,
            _provider: &str,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Vec<PaymentId>, crate::event::store::EventStoreError>,
                    > + Send
                    + '_,
            >,
        > {
            let ids = self.ids.clone();
            Box::pin(async move { Ok(ids) })
        }
    }

    fn fixed_time() -> chrono::DateTime<Utc> {
        chrono::DateTime::from_timestamp_millis(1_700_000_000_000).unwrap()
    }

    fn make_event(
        payment_id: &PaymentId,
        provider: &str,
        state_after: PaymentState,
        timestamp: chrono::DateTime<Utc>,
    ) -> CanonicalEvent {
        CanonicalEvent {
            event_id: EventId::new(),
            event_type: EventType::new("payment.charge.captured").unwrap(),
            payment_id: payment_id.clone(),
            provider: provider.to_string(),
            timestamp,
            state_before: PaymentState::Created,
            state_after,
            amount: Money::new(10000, Currency::ZAR),
            idempotency_key: format!("idem_{}", EventId::new()),
            raw_provider_payload: serde_json::json!({}),
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn run_once_returns_results_for_all_providers() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = PaymentId::new();
        let now = fixed_time();

        // Add matching events
        let app = make_event(&pid, "app", PaymentState::Captured, now);
        store.append(&app).await.unwrap();
        let prov = make_event(
            &pid,
            "peach_payments",
            PaymentState::Captured,
            now + chrono::Duration::seconds(5),
        );
        store.append(&prov).await.unwrap();

        let config = ReconciliationConfig::default();
        let engine = ReconciliationEngine::new(store, config);
        let source = StaticPaymentSource { ids: vec![pid] };
        let (_tx, rx) = watch::channel(false);
        let loop_runner =
            ReconciliationLoop::new(engine, source, vec!["peach_payments".to_string()], rx);

        let results = loop_runner.run_once().await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "peach_payments");
        assert_eq!(results[0].results.len(), 1);
    }

    #[tokio::test]
    async fn run_once_tracks_per_provider_duration() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let config = ReconciliationConfig::default();
        let engine = ReconciliationEngine::new(store, config);
        let source = StaticPaymentSource { ids: vec![] };
        let (_tx, rx) = watch::channel(false);
        let loop_runner = ReconciliationLoop::new(
            engine,
            source,
            vec!["provider_a".to_string(), "provider_b".to_string()],
            rx,
        );

        let results = loop_runner.run_once().await.unwrap();
        assert_eq!(results.len(), 2);
        // Duration should be very small for empty sets
        for r in &results {
            assert!(r.duration.as_secs() < 1);
        }
    }

    #[tokio::test]
    async fn shutdown_signal_stops_loop() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let config = ReconciliationConfig {
            interval: std::time::Duration::from_millis(50),
            ..Default::default()
        };
        let engine = ReconciliationEngine::new(store, config);
        let source = StaticPaymentSource { ids: vec![] };
        let (tx, rx) = watch::channel(false);
        let mut loop_runner = ReconciliationLoop::new(engine, source, vec!["test".to_string()], rx);

        // Send shutdown after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let _ = tx.send(true);
        });

        // This should complete (not hang forever)
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(2), loop_runner.run()).await;

        assert!(result.is_ok(), "Loop should have stopped due to shutdown");
        assert!(result.unwrap().is_ok());
    }
}
