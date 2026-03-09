//! Canonical event system for provider-agnostic payment event processing.
//!
//! All provider webhooks and events are normalized into [`CanonicalEvent`] with
//! validated [`EventType`] strings in `domain.entity.action` format.
//!
//! # Key Types
//!
//! - [`CanonicalEvent`]: Normalized event from any provider
//! - [`EventType`]: Validated `domain.entity.action` string
//! - [`EventEnvelope`]: Wraps events with routing metadata
//! - [`RawWebhook`]: Raw webhook payload before normalization

/// Append-only event store for immutable payment audit trails.
/// Public (not `pub(crate)`) because `EventStore` trait is part of the public API
/// consumed by adapters and external backends.
pub mod store;
/// Event types: [`CanonicalEvent`], [`EventType`], [`RawWebhook`], [`EventEnvelope`].
pub(crate) mod types;

pub use store::{EventStore, EventStoreError, SqliteEventStore};
pub use types::{CanonicalEvent, EventEnvelope, EventType, RawWebhook};
