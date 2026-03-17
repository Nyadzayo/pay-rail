//! Continuous reconciliation engine for comparing optimistic vs provider-confirmed state.
//!
//! Compares each payment's optimistic state (app-facing) against its reconciled state
//! (provider-confirmed) to detect discrepancies and drive settlement transitions.

mod discrepancy;
mod engine;
mod loop_runner;
mod report;
mod types;

pub use discrepancy::{
    Discrepancy, DiscrepancySeverity, Escalation, EscalationError, EscalationSink,
    InMemoryEscalationSink, LogEscalationSink, Resolution, ResolutionType,
};
pub use engine::{ReconciliationCycleError, ReconciliationCycleResult, ReconciliationEngine};
pub use loop_runner::{LoopIterationResult, PaymentIdSource, ReconciliationLoop};
pub use report::{DiscrepancyBreakdown, ReconciliationReport, ResolutionSummary};
pub use types::{
    DiscrepancyCategory, ReconciliationConfig, ReconciliationResult, ReconciliationStatus,
};
