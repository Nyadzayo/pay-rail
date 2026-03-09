//! PayRail Core — Type-safe payment state machine for African payment providers.
//!
//! This crate provides a compile-time enforced payment lifecycle using Rust's
//! typestate pattern. Invalid state transitions are caught at compile time, not
//! runtime.
//!
//! # Quick Start
//!
//! ```rust
//! use payrail_core::prelude::*;
//! use chrono::Utc;
//!
//! let now = Utc::now();
//! let intent = PaymentIntent {
//!     id: PaymentId::new(),
//!     amount: Money::new(15000, Currency::ZAR), // R150.00
//!     provider: "peach_payments".to_owned(),
//!     metadata: serde_json::json!({}),
//! };
//!
//! let payment = Payment::<Created>::create(intent, now);
//! let authorized = payment.authorize(now);
//! let captured = authorized.capture(now);
//! // captured.authorize(now); // Compile error! Can't re-authorize a captured payment.
//! ```
//!
//! # Module Map
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`payment`] | Typestate machine, state markers, transitions |
//! | [`event`] | Canonical events, webhook payloads, event store |
//! | [`error`] | Structured errors with domain codes |
//! | [`id`] | ULID-based prefixed identifiers (`pay_`, `evt_`) |
//! | [`config`] | Configuration loading (stub) |
//! | [`knowledge`] | Knowledge pack schema for provider intelligence |
//! | [`idempotency`] | Deduplication engine (stub) |
//! | [`webhook`] | Webhook signature verification and receiver |
//! | [`prelude`] | Convenient re-exports for common usage |
//!
//! # Design Decisions
//!
//! - **Integer cents**: All money values use `i64` cents — never floating point.
//! - **Explicit time**: All transitions accept `DateTime<Utc>` for deterministic testing.
//! - **Consume-and-return**: Transitions take `self` by move. Previous states are inaccessible.
//! - **Sealed traits**: `PaymentStateMarker` and `TimeoutEnforceable` cannot be implemented externally.

#![warn(missing_docs)]

/// Configuration loading for PayRail.
///
/// Loads settings from `payrail.config.yaml`. Implementation deferred to a future story.
#[allow(missing_docs)]
pub mod config;

/// Structured error types with domain-prefixed codes.
///
/// Errors follow the `[WHAT] [WHY] [WHAT TO DO]` format with machine-readable
/// codes like `PAY_INVALID_TRANSITION`, `ADAPTER_TIMEOUT`, etc.
pub mod error;

/// Canonical event system for provider-agnostic event processing.
///
/// Normalizes events from any payment provider into a common `CanonicalEvent`
/// format with `domain.entity.action` event types.
pub mod event;

/// ULID-based identifiers with type-safe prefixes.
///
/// - [`PaymentId`]: `pay_` prefix (e.g., `pay_01HX...`)
/// - [`EventId`]: `evt_` prefix (e.g., `evt_01HX...`)
pub mod id;

/// Knowledge pack schema for structured provider intelligence.
///
/// Defines the canonical types for knowledge packs: provider metadata,
/// API endpoints, webhook events, status codes, error codes, and payment flows.
/// Each fact carries confidence metadata with source attribution and decay rate.
pub mod knowledge;

/// Idempotency engine for deduplicating payment operations.
///
/// Prevents double-processing of webhooks and commands via deterministic
/// key generation and pluggable storage backends.
pub mod idempotency;

/// Typestate payment engine with compile-time transition safety.
///
/// The core of PayRail: a state machine where invalid transitions are caught
/// by the Rust compiler. See [`Payment`] for the main entry point.
pub mod payment;

/// Commonly-used types for PayRail payment processing.
///
/// Import with `use payrail_core::prelude::*` for convenient access
/// to payment types, state markers, money, and transition types.
pub mod prelude;

/// Webhook signature verification and processing.
///
/// Two-phase webhook handling: verify signature, deduplicate, transition, ACK.
pub mod webhook;

pub use error::{ErrorCode, PayRailError};
pub use event::{
    CanonicalEvent, EventEnvelope, EventStore, EventStoreError, EventType, RawWebhook,
    SqliteEventStore,
};
pub use id::{EventId, PaymentId};
pub use idempotency::{
    IdempotencyError, IdempotencyKey, IdempotencyOutcome, IdempotencyRecord, IdempotencyStore,
    KeyError, SqliteIdempotencyStore,
};
pub use knowledge::{
    ConfidenceScore, EndpointFact, ErrorCodeFact, FactEntry, FactSource, KnowledgePack,
    PaymentFlowSequence, ProviderMetadata, StatusCodeMapping, VERIFY_THRESHOLD, WebhookEventFact,
    decayed_score, needs_reverification,
};
pub use payment::{
    Authorized, Captured, Created, Currency, Failed, Money, Payment, PaymentCommand, PaymentIntent,
    PaymentState, PaymentStateMarker, Pending3DS, Refunded, TimedOut, TimeoutConfig,
    TimeoutEnforceable, TransitionError, TransitionResult, Voided,
};
pub use webhook::{
    EnvSecretStore, ReceiverError, SecretStore, SignatureConfig, SignatureError, SignatureMethod,
    WebhookNormalizer, WebhookOutcome, WebhookReceiver, verify_signature,
};
