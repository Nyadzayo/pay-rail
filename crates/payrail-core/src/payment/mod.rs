//! Typestate payment engine with compile-time transition enforcement.
//!
//! This module implements the payment lifecycle as a state machine where each
//! state is a distinct Rust type. Invalid transitions fail at compile time.
//!
//! # State Diagram
//!
//! ```text
//!                    ┌─────────┐
//!                    │ Created │
//!                    └────┬────┘
//!                   ╱     │     ╲
//!          Pending3DS  Authorized  Failed/TimedOut
//!               │    ╱    │    ╲
//!          Authorized  Captured  Voided  Failed/TimedOut
//!                        │
//!                    Refunded / Failed / TimedOut
//! ```
//!
//! # Key Types
//!
//! - [`Payment`]: The payment with compile-time state `S`
//! - [`TransitionResult`]: Runtime transition outcome
//! - [`TimeoutConfig`]: Per-state timeout durations

/// State machine implementation: [`Payment`], transitions, and [`TransitionResult`].
pub(crate) mod machine;
/// Typestate marker types for each payment state (e.g., [`Created`], [`Authorized`]).
pub(crate) mod state;
/// Per-state timeout configuration and enforcement.
pub(crate) mod timeout;
/// Core value types: [`Money`], [`Currency`], [`PaymentIntent`], [`PaymentState`].
pub(crate) mod types;

pub use machine::{Payment, TransitionError, TransitionResult};
pub use state::{
    Authorized, Captured, Created, Failed, PaymentStateMarker, Pending3DS, Refunded, TimedOut,
    TimeoutEnforceable, Voided,
};
pub use timeout::TimeoutConfig;
pub use types::{Currency, Money, PaymentCommand, PaymentIntent, PaymentState};
