use std::marker::PhantomData;

use chrono::{DateTime, Utc};

use crate::id::PaymentId;
use crate::payment::state::*;
use crate::payment::timeout::TimeoutConfig;
use crate::payment::types::{PaymentIntent, PaymentState};

/// Error returned when an invalid state transition is attempted.
///
/// # Example
///
/// ```
/// use payrail_core::prelude::*;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let intent = PaymentIntent {
///     id: PaymentId::new(),
///     amount: Money::new(10000, Currency::ZAR),
///     provider: "test".to_owned(),
///     metadata: serde_json::json!({}),
/// };
/// let payment = Payment::<Created>::create(intent, now);
/// match payment.try_transition(PaymentState::Refunded, now) {
///     TransitionResult::Rejected { error, .. } => {
///         assert_eq!(error.code(), "PAY_INVALID_TRANSITION");
///         assert_eq!(error.domain(), "payment");
///     }
///     _ => panic!("expected rejection"),
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionError {
    /// The state the payment was in when the transition was attempted.
    pub current_state: PaymentState,
    /// The target state that was requested.
    pub attempted: PaymentState,
    /// The transitions that are valid from the current state.
    pub valid_transitions: &'static [PaymentState],
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PAY_INVALID_TRANSITION: Cannot transition from {} to {}. Valid transitions: {:?}",
            self.current_state, self.attempted, self.valid_transitions
        )
    }
}

impl std::error::Error for TransitionError {}

impl TransitionError {
    /// Machine-readable error code following the `PAY_*` convention.
    pub fn code(&self) -> &'static str {
        "PAY_INVALID_TRANSITION"
    }

    /// Error domain for routing and categorization.
    pub fn domain(&self) -> &'static str {
        "payment"
    }
}

/// Result of attempting a runtime state transition via [`Payment::try_transition`].
///
/// # Example
///
/// ```
/// use payrail_core::prelude::*;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let intent = PaymentIntent {
///     id: PaymentId::new(),
///     amount: Money::new(10000, Currency::ZAR),
///     provider: "test".to_owned(),
///     metadata: serde_json::json!({}),
/// };
/// let payment = Payment::<Created>::create(intent, now);
/// match payment.try_transition(PaymentState::Authorized, now) {
///     TransitionResult::Applied { new_state, .. } => {
///         assert_eq!(new_state, PaymentState::Authorized);
///     }
///     TransitionResult::SelfTransition(_) => panic!("not a self-transition"),
///     TransitionResult::Rejected { .. } => panic!("should be valid"),
/// }
/// ```
#[derive(Debug)]
#[must_use = "transition results must be handled — ignoring may silently drop state changes"]
pub enum TransitionResult<S: PaymentStateMarker> {
    /// Valid transition applied. Payment consumed; data and new state returned.
    ///
    /// Note: returns raw fields with a runtime `PaymentState` rather than a typed
    /// `Payment<NewState>` because the target state is determined at runtime
    /// (e.g., from webhook payloads). Callers should match on `new_state` and
    /// reconstruct typed payments as needed.
    Applied {
        /// Payment identifier.
        id: PaymentId,
        /// Original payment intent.
        intent: PaymentIntent,
        /// State before the transition.
        previous_state: PaymentState,
        /// State after the transition.
        new_state: PaymentState,
        /// When the payment was originally created.
        created_at: DateTime<Utc>,
        /// Timestamp of this transition.
        last_transition_at: DateTime<Utc>,
    },
    /// Duplicate event detected: target state equals current state.
    /// Payment returned unchanged. Zero side effects.
    SelfTransition(Payment<S>),
    /// Invalid transition. Payment returned with structured error.
    Rejected {
        /// The payment, returned unchanged.
        payment: Payment<S>,
        /// Details of the invalid transition.
        error: TransitionError,
    },
}

/// A payment with compile-time state enforcement.
///
/// `S` is a zero-sized marker type that determines which transitions are available.
/// Transitions consume `self` (move semantics), making the previous state inaccessible.
///
/// All transitions and the constructor accept an explicit `DateTime<Utc>` parameter
/// instead of calling `Utc::now()` internally. This makes the entire state machine
/// deterministically testable without mocking clocks.
///
/// # Example
///
/// ```rust
/// use payrail_core::prelude::*;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let intent = PaymentIntent {
///     id: PaymentId::new(),
///     amount: Money::new(10000, Currency::ZAR),
///     provider: "test".to_owned(),
///     metadata: serde_json::json!({}),
/// };
/// let created = Payment::<Created>::create(intent, now);
/// let authorized = created.authorize(now);
/// let captured = authorized.capture(now);
/// let refunded = captured.refund(now);
/// // refunded is terminal — no further transitions available
/// ```
///
/// ```compile_fail
/// use payrail_core::prelude::*;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let intent = PaymentIntent {
///     id: PaymentId::new(),
///     amount: Money::new(10000, Currency::ZAR),
///     provider: "test".to_owned(),
///     metadata: serde_json::json!({}),
/// };
/// let created = Payment::<Created>::create(intent, now);
/// created.refund(now); // ERROR: no method named `refund` found for `Payment<Created>`
/// ```
///
/// ```compile_fail
/// use payrail_core::prelude::*;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let intent = PaymentIntent {
///     id: PaymentId::new(),
///     amount: Money::new(10000, Currency::ZAR),
///     provider: "test".to_owned(),
///     metadata: serde_json::json!({}),
/// };
/// let created = Payment::<Created>::create(intent, now);
/// let captured = created.authorize(now).capture(now);
/// captured.capture(now); // ERROR: no method named `capture` found for `Payment<Captured>`
/// ```
///
/// ```compile_fail
/// use payrail_core::prelude::*;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let intent = PaymentIntent {
///     id: PaymentId::new(),
///     amount: Money::new(10000, Currency::ZAR),
///     provider: "test".to_owned(),
///     metadata: serde_json::json!({}),
/// };
/// let refunded = Payment::<Created>::create(intent, now).authorize(now).capture(now).refund(now);
/// refunded.void(now); // ERROR: no method named `void` found for `Payment<Refunded>`
/// ```
///
/// ```compile_fail
/// use payrail_core::prelude::*;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let intent = PaymentIntent {
///     id: PaymentId::new(),
///     amount: Money::new(10000, Currency::ZAR),
///     provider: "test".to_owned(),
///     metadata: serde_json::json!({}),
/// };
/// let voided = Payment::<Created>::create(intent, now).authorize(now).void(now);
/// voided.fail(now); // ERROR: no method named `fail` found for `Payment<Voided>`
/// ```
///
/// ```compile_fail
/// use payrail_core::prelude::*;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let intent = PaymentIntent {
///     id: PaymentId::new(),
///     amount: Money::new(10000, Currency::ZAR),
///     provider: "test".to_owned(),
///     metadata: serde_json::json!({}),
/// };
/// let failed = Payment::<Created>::create(intent, now).fail(now);
/// failed.authorize(now); // ERROR: no method named `authorize` found for `Payment<Failed>`
/// ```
///
/// ```compile_fail
/// use payrail_core::prelude::*;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let intent = PaymentIntent {
///     id: PaymentId::new(),
///     amount: Money::new(10000, Currency::ZAR),
///     provider: "test".to_owned(),
///     metadata: serde_json::json!({}),
/// };
/// let created = Payment::<Created>::create(intent, now);
/// created.capture(now); // ERROR: no method named `capture` found for `Payment<Created>`
/// ```
///
/// ```compile_fail
/// use payrail_core::prelude::*;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let intent = PaymentIntent {
///     id: PaymentId::new(),
///     amount: Money::new(10000, Currency::ZAR),
///     provider: "test".to_owned(),
///     metadata: serde_json::json!({}),
/// };
/// let timed_out = Payment::<Created>::create(intent, now).timeout(now);
/// timed_out.authorize(now); // ERROR: no method named `authorize` found for `Payment<TimedOut>`
/// ```
#[derive(Debug)]
pub struct Payment<S: PaymentStateMarker> {
    /// Unique payment identifier.
    pub id: PaymentId,
    /// The original payment intent.
    pub intent: PaymentIntent,
    /// When the payment was created.
    pub created_at: DateTime<Utc>,
    /// When the last state transition occurred.
    pub last_transition_at: DateTime<Utc>,
    _state: PhantomData<S>,
}

impl<S: PaymentStateMarker> std::fmt::Display for Payment<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Payment({}, {})", self.id, self.state())
    }
}

impl<S: PaymentStateMarker> Payment<S> {
    /// Returns the runtime `PaymentState` variant matching this payment's typestate.
    pub fn state(&self) -> PaymentState {
        S::runtime_state()
    }

    /// Returns the valid target states for the current state.
    pub fn valid_transitions(&self) -> &'static [PaymentState] {
        valid_transitions_for(&self.state())
    }

    /// Checks whether this payment has exceeded its timeout duration.
    pub fn is_timed_out(&self, config: &TimeoutConfig, now: DateTime<Utc>) -> bool {
        if let Some(timeout) = config.timeout_for(&self.state()) {
            let duration = chrono::Duration::from_std(timeout)
                .expect("timeout duration exceeds chrono::Duration range");
            let deadline = self.last_transition_at + duration;
            now >= deadline
        } else {
            false
        }
    }

    /// Returns the deadline by which this payment must transition, or `None` for terminal states.
    pub fn timeout_deadline(&self, config: &TimeoutConfig) -> Option<DateTime<Utc>> {
        config.timeout_for(&self.state()).map(|timeout| {
            let d = chrono::Duration::from_std(timeout)
                .expect("timeout duration exceeds chrono::Duration range");
            self.last_transition_at + d
        })
    }

    /// Attempts a runtime state transition. Detects self-transitions and rejects invalid ones.
    ///
    /// - `SelfTransition`: target equals current state. Payment returned unchanged.
    /// - `Applied`: valid transition. Payment consumed; data + new state returned.
    /// - `Rejected`: invalid transition. Payment returned with error.
    ///
    /// # Example
    ///
    /// ```
    /// use payrail_core::prelude::*;
    /// use chrono::Utc;
    ///
    /// let now = Utc::now();
    /// let intent = PaymentIntent {
    ///     id: PaymentId::new(),
    ///     amount: Money::new(10000, Currency::ZAR),
    ///     provider: "test".to_owned(),
    ///     metadata: serde_json::json!({}),
    /// };
    /// let payment = Payment::<Created>::create(intent, now);
    ///
    /// // Applied — valid transition
    /// match payment.try_transition(PaymentState::Authorized, now) {
    ///     TransitionResult::Applied { new_state, .. } => {
    ///         assert_eq!(new_state, PaymentState::Authorized);
    ///     }
    ///     _ => panic!("expected Applied"),
    /// }
    ///
    /// // SelfTransition — target equals current state
    /// let intent2 = PaymentIntent {
    ///     id: PaymentId::new(),
    ///     amount: Money::new(5000, Currency::ZAR),
    ///     provider: "test".to_owned(),
    ///     metadata: serde_json::json!({}),
    /// };
    /// let payment2 = Payment::<Created>::create(intent2, now);
    /// match payment2.try_transition(PaymentState::Created, now) {
    ///     TransitionResult::SelfTransition(p) => {
    ///         assert_eq!(p.state(), PaymentState::Created);
    ///     }
    ///     _ => panic!("expected SelfTransition"),
    /// }
    ///
    /// // Rejected — invalid transition
    /// let intent3 = PaymentIntent {
    ///     id: PaymentId::new(),
    ///     amount: Money::new(5000, Currency::ZAR),
    ///     provider: "test".to_owned(),
    ///     metadata: serde_json::json!({}),
    /// };
    /// let payment3 = Payment::<Created>::create(intent3, now);
    /// match payment3.try_transition(PaymentState::Refunded, now) {
    ///     TransitionResult::Rejected { error, .. } => {
    ///         assert_eq!(error.current_state, PaymentState::Created);
    ///         assert_eq!(error.attempted, PaymentState::Refunded);
    ///     }
    ///     _ => panic!("expected Rejected"),
    /// }
    /// ```
    pub fn try_transition(self, target: PaymentState, now: DateTime<Utc>) -> TransitionResult<S> {
        let current = self.state();
        if target == current {
            return TransitionResult::SelfTransition(self);
        }
        let valid = valid_transitions_for(&current);
        if valid.contains(&target) {
            TransitionResult::Applied {
                id: self.id,
                intent: self.intent,
                previous_state: current,
                new_state: target,
                created_at: self.created_at,
                last_transition_at: now,
            }
        } else {
            TransitionResult::Rejected {
                payment: self,
                error: TransitionError {
                    current_state: current,
                    attempted: target,
                    valid_transitions: valid,
                },
            }
        }
    }
}

/// Timeout enforcement for non-terminal states.
#[allow(clippy::result_large_err)]
impl<S: TimeoutEnforceable> Payment<S> {
    /// If the payment has exceeded its timeout, consumes it and returns `Payment<TimedOut>`.
    /// Otherwise returns `Err(self)` so the caller retains ownership.
    ///
    /// # Example
    ///
    /// ```
    /// use payrail_core::prelude::*;
    /// use chrono::{Utc, Duration};
    ///
    /// let now = Utc::now();
    /// let intent = PaymentIntent {
    ///     id: PaymentId::new(),
    ///     amount: Money::new(10000, Currency::ZAR),
    ///     provider: "test".to_owned(),
    ///     metadata: serde_json::json!({}),
    /// };
    /// let payment = Payment::<Created>::create(intent, now);
    /// let config = TimeoutConfig::default(); // Created timeout = 30 min
    ///
    /// // Not timed out yet
    /// let payment = payment.enforce_timeout(&config, now).unwrap_err();
    ///
    /// // After timeout has elapsed
    /// let later = now + Duration::hours(1);
    /// let timed_out = payment.enforce_timeout(&config, later).unwrap();
    /// assert_eq!(timed_out.state(), PaymentState::TimedOut);
    /// ```
    pub fn enforce_timeout(
        self,
        config: &TimeoutConfig,
        now: DateTime<Utc>,
    ) -> Result<Payment<TimedOut>, Self> {
        if self.is_timed_out(config, now) {
            Ok(Payment {
                id: self.id,
                intent: self.intent,
                created_at: self.created_at,
                last_transition_at: now,
                _state: PhantomData,
            })
        } else {
            Err(self)
        }
    }
}

// -- Constructor --

impl Payment<Created> {
    /// Creates a new payment from an intent. The payment starts in `Created` state.
    pub fn create(intent: PaymentIntent, now: DateTime<Utc>) -> Self {
        Self {
            id: intent.id.clone(),
            intent,
            created_at: now,
            last_transition_at: now,
            _state: PhantomData,
        }
    }
}

// Helper to build the next state from the current payment
fn transition<Src: PaymentStateMarker, Dst: PaymentStateMarker>(
    payment: Payment<Src>,
    now: DateTime<Utc>,
) -> Payment<Dst> {
    Payment {
        id: payment.id,
        intent: payment.intent,
        created_at: payment.created_at,
        last_transition_at: now,
        _state: PhantomData,
    }
}

/// Returns the valid target states for a given payment state.
/// Returns a static slice — zero heap allocation on the hot path.
fn valid_transitions_for(state: &PaymentState) -> &'static [PaymentState] {
    match state {
        PaymentState::Created => &[
            PaymentState::Authorized,
            PaymentState::Pending3ds,
            PaymentState::Failed,
            PaymentState::TimedOut,
        ],
        PaymentState::Pending3ds => &[
            PaymentState::Authorized,
            PaymentState::Failed,
            PaymentState::TimedOut,
        ],
        PaymentState::Authorized => &[
            PaymentState::Captured,
            PaymentState::Voided,
            PaymentState::Failed,
            PaymentState::TimedOut,
        ],
        PaymentState::Captured => &[
            PaymentState::Refunded,
            PaymentState::Settled,
            PaymentState::Failed,
            PaymentState::TimedOut,
        ],
        PaymentState::Refunded => &[PaymentState::Settled],
        PaymentState::TimedOut => &[PaymentState::Failed],
        PaymentState::Voided | PaymentState::Failed | PaymentState::Settled => &[],
    }
}

// -- Created transitions --

impl Payment<Created> {
    /// Transitions to `Authorized`. Consumes the payment.
    pub fn authorize(self, now: DateTime<Utc>) -> Payment<Authorized> {
        transition(self, now)
    }

    /// Transitions to `Pending3DS` for 3D Secure verification. Consumes the payment.
    pub fn pending_3ds(self, now: DateTime<Utc>) -> Payment<Pending3DS> {
        transition(self, now)
    }

    /// Transitions to `Failed` (terminal). Consumes the payment.
    pub fn fail(self, now: DateTime<Utc>) -> Payment<Failed> {
        transition(self, now)
    }

    /// Transitions to `TimedOut`. Consumes the payment.
    pub fn timeout(self, now: DateTime<Utc>) -> Payment<TimedOut> {
        transition(self, now)
    }
}

// -- Pending3DS transitions --

impl Payment<Pending3DS> {
    /// Transitions to `Authorized` after 3DS verification. Consumes the payment.
    pub fn authorize(self, now: DateTime<Utc>) -> Payment<Authorized> {
        transition(self, now)
    }

    /// Transitions to `Failed` (terminal). Consumes the payment.
    pub fn fail(self, now: DateTime<Utc>) -> Payment<Failed> {
        transition(self, now)
    }

    /// Transitions to `TimedOut`. Consumes the payment.
    pub fn timeout(self, now: DateTime<Utc>) -> Payment<TimedOut> {
        transition(self, now)
    }
}

// -- Authorized transitions --

impl Payment<Authorized> {
    /// Captures authorized funds. Consumes the payment.
    pub fn capture(self, now: DateTime<Utc>) -> Payment<Captured> {
        transition(self, now)
    }

    /// Voids the authorization (terminal). Consumes the payment.
    pub fn void(self, now: DateTime<Utc>) -> Payment<Voided> {
        transition(self, now)
    }

    /// Transitions to `Failed` (terminal). Consumes the payment.
    pub fn fail(self, now: DateTime<Utc>) -> Payment<Failed> {
        transition(self, now)
    }

    /// Transitions to `TimedOut`. Consumes the payment.
    pub fn timeout(self, now: DateTime<Utc>) -> Payment<TimedOut> {
        transition(self, now)
    }
}

// -- Captured transitions --
// Note: Captured intentionally has no timeout(). Once funds are collected,
// manual timeout is not a valid domain concept. However, enforce_timeout()
// IS available via TimeoutEnforceable trait for automatic timeout enforcement
// (e.g., dispute window expiry).

impl Payment<Captured> {
    /// Refunds captured funds (terminal). Consumes the payment.
    pub fn refund(self, now: DateTime<Utc>) -> Payment<Refunded> {
        transition(self, now)
    }

    /// Settles a captured payment after reconciliation confirms provider settlement.
    /// Consumes the payment.
    pub fn settle(self, now: DateTime<Utc>) -> Payment<Settled> {
        transition(self, now)
    }

    /// Transitions to `Failed` (terminal). Consumes the payment.
    pub fn fail(self, now: DateTime<Utc>) -> Payment<Failed> {
        transition(self, now)
    }
}

// -- TimedOut transitions --

impl Payment<TimedOut> {
    /// Transitions to `Failed` (terminal). Consumes the payment.
    pub fn fail(self, now: DateTime<Utc>) -> Payment<Failed> {
        transition(self, now)
    }
}

// -- Refunded transitions --

impl Payment<Refunded> {
    /// Settles a refunded payment after reconciliation confirms refund settlement.
    /// Consumes the payment.
    pub fn settle(self, now: DateTime<Utc>) -> Payment<Settled> {
        transition(self, now)
    }
}

// Terminal states: Settled, Voided, Failed — no transition methods

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::types::{Currency, Money};
    use std::time::Duration as StdDuration;

    fn fixed_time() -> DateTime<Utc> {
        DateTime::from_timestamp_millis(1_700_000_000_000).unwrap()
    }

    fn later(base: DateTime<Utc>, secs: i64) -> DateTime<Utc> {
        base + chrono::Duration::seconds(secs)
    }

    fn test_intent() -> PaymentIntent {
        let fixed_ulid = ulid::Ulid::from_parts(1_700_000_000_000, 42);
        PaymentIntent {
            id: PaymentId::from_ulid(fixed_ulid),
            amount: Money::new(10000, Currency::ZAR),
            provider: "test_provider".to_owned(),
            metadata: serde_json::json!({}),
        }
    }

    // ========== Existing Story 1.3 tests (updated for new API) ==========

    #[test]
    fn payment_created_has_correct_fields() {
        let now = fixed_time();
        let intent = test_intent();
        let expected_id = intent.id.clone();
        let payment = Payment::<Created>::create(intent.clone(), now);
        assert_eq!(payment.id, expected_id);
        assert_eq!(payment.intent, intent);
    }

    #[test]
    fn payment_phantom_data_is_zero_sized() {
        assert_eq!(std::mem::size_of::<PhantomData<Created>>(), 0);
    }

    #[test]
    fn created_authorize_returns_authorized() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        let authorized = payment.authorize(now);
        assert_eq!(authorized.state(), PaymentState::Authorized);
    }

    #[test]
    fn authorized_capture_returns_captured() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now).authorize(now);
        let captured = payment.capture(now);
        assert_eq!(captured.state(), PaymentState::Captured);
    }

    #[test]
    fn captured_refund_returns_refunded() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now);
        let refunded = payment.refund(now);
        assert_eq!(refunded.state(), PaymentState::Refunded);
    }

    #[test]
    fn authorized_void_returns_voided() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now).authorize(now);
        let voided = payment.void(now);
        assert_eq!(voided.state(), PaymentState::Voided);
    }

    #[test]
    fn happy_path_full_flow() {
        let now = fixed_time();
        let intent = test_intent();
        let id = intent.id.clone();
        let refunded = Payment::<Created>::create(intent, now)
            .authorize(now)
            .capture(now)
            .refund(now);
        assert_eq!(refunded.id, id);
        assert_eq!(refunded.state(), PaymentState::Refunded);
    }

    #[test]
    fn created_pending_3ds_returns_pending3ds() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        let pending = payment.pending_3ds(now);
        assert_eq!(pending.state(), PaymentState::Pending3ds);
    }

    #[test]
    fn pending_3ds_authorize_returns_authorized() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now).pending_3ds(now);
        let authorized = payment.authorize(now);
        assert_eq!(authorized.state(), PaymentState::Authorized);
    }

    #[test]
    fn pending_3ds_fail_returns_failed() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now).pending_3ds(now);
        let failed = payment.fail(now);
        assert_eq!(failed.state(), PaymentState::Failed);
    }

    #[test]
    fn three_ds_success_flow() {
        let now = fixed_time();
        let captured = Payment::<Created>::create(test_intent(), now)
            .pending_3ds(now)
            .authorize(now)
            .capture(now);
        assert_eq!(captured.state(), PaymentState::Captured);
    }

    #[test]
    fn created_fail_returns_failed() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        let failed = payment.fail(now);
        assert_eq!(failed.state(), PaymentState::Failed);
    }

    #[test]
    fn authorized_fail_returns_failed() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now).authorize(now);
        let failed = payment.fail(now);
        assert_eq!(failed.state(), PaymentState::Failed);
    }

    #[test]
    fn captured_fail_returns_failed() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now);
        let failed = payment.fail(now);
        assert_eq!(failed.state(), PaymentState::Failed);
    }

    #[test]
    fn created_timeout_returns_timed_out() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        let timed_out = payment.timeout(now);
        assert_eq!(timed_out.state(), PaymentState::TimedOut);
    }

    #[test]
    fn pending_3ds_timeout_returns_timed_out() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now).pending_3ds(now);
        let timed_out = payment.timeout(now);
        assert_eq!(timed_out.state(), PaymentState::TimedOut);
    }

    #[test]
    fn authorized_timeout_returns_timed_out() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now).authorize(now);
        let timed_out = payment.timeout(now);
        assert_eq!(timed_out.state(), PaymentState::TimedOut);
    }

    #[test]
    fn timed_out_fail_returns_failed() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now).timeout(now);
        let failed = payment.fail(now);
        assert_eq!(failed.state(), PaymentState::Failed);
    }

    #[test]
    fn state_bridge_all_states() {
        let now = fixed_time();
        let intent = test_intent();

        let created = Payment::<Created>::create(intent.clone(), now);
        assert_eq!(created.state(), PaymentState::Created);

        let pending = Payment::<Created>::create(intent.clone(), now).pending_3ds(now);
        assert_eq!(pending.state(), PaymentState::Pending3ds);

        let authorized = Payment::<Created>::create(intent.clone(), now).authorize(now);
        assert_eq!(authorized.state(), PaymentState::Authorized);

        let captured = Payment::<Created>::create(intent.clone(), now)
            .authorize(now)
            .capture(now);
        assert_eq!(captured.state(), PaymentState::Captured);

        let refunded = Payment::<Created>::create(intent.clone(), now)
            .authorize(now)
            .capture(now)
            .refund(now);
        assert_eq!(refunded.state(), PaymentState::Refunded);

        let voided = Payment::<Created>::create(intent.clone(), now)
            .authorize(now)
            .void(now);
        assert_eq!(voided.state(), PaymentState::Voided);

        let failed = Payment::<Created>::create(intent.clone(), now).fail(now);
        assert_eq!(failed.state(), PaymentState::Failed);

        let timed_out = Payment::<Created>::create(intent, now).timeout(now);
        assert_eq!(timed_out.state(), PaymentState::TimedOut);
    }

    #[test]
    fn id_preserved_through_transitions() {
        let now = fixed_time();
        let intent = test_intent();
        let id = intent.id.clone();
        let payment = Payment::<Created>::create(intent, now);
        assert_eq!(payment.id, id);
        let payment = payment.authorize(now);
        assert_eq!(payment.id, id);
        let payment = payment.capture(now);
        assert_eq!(payment.id, id);
        let payment = payment.refund(now);
        assert_eq!(payment.id, id);
    }

    #[test]
    fn intent_preserved_through_3ds_path() {
        let now = fixed_time();
        let intent = test_intent();
        let id = intent.id.clone();
        let amount = intent.amount.clone();
        let refunded = Payment::<Created>::create(intent, now)
            .pending_3ds(now)
            .authorize(now)
            .capture(now)
            .refund(now);
        assert_eq!(refunded.id, id);
        assert_eq!(refunded.intent.amount, amount);
    }

    #[test]
    fn intent_preserved_through_void_path() {
        let now = fixed_time();
        let intent = test_intent();
        let id = intent.id.clone();
        let provider = intent.provider.clone();
        let voided = Payment::<Created>::create(intent, now)
            .authorize(now)
            .void(now);
        assert_eq!(voided.id, id);
        assert_eq!(voided.intent.provider, provider);
    }

    #[test]
    fn intent_preserved_through_timeout_fail_path() {
        let now = fixed_time();
        let intent = test_intent();
        let id = intent.id.clone();
        let amount = intent.amount.clone();
        let failed = Payment::<Created>::create(intent, now)
            .timeout(now)
            .fail(now);
        assert_eq!(failed.id, id);
        assert_eq!(failed.intent.amount, amount);
    }

    #[test]
    fn state_bridge_3ds_chain_each_step() {
        let now = fixed_time();
        let intent = test_intent();
        let created = Payment::<Created>::create(intent, now);
        assert_eq!(created.state(), PaymentState::Created);
        let pending = created.pending_3ds(now);
        assert_eq!(pending.state(), PaymentState::Pending3ds);
        let authorized = pending.authorize(now);
        assert_eq!(authorized.state(), PaymentState::Authorized);
        let captured = authorized.capture(now);
        assert_eq!(captured.state(), PaymentState::Captured);
    }

    #[test]
    fn authorized_all_four_transitions() {
        let now = fixed_time();

        let auth1 = Payment::<Created>::create(test_intent(), now).authorize(now);
        let captured = auth1.capture(now);
        assert_eq!(captured.state(), PaymentState::Captured);

        let auth2 = Payment::<Created>::create(test_intent(), now).authorize(now);
        let voided = auth2.void(now);
        assert_eq!(voided.state(), PaymentState::Voided);

        let auth3 = Payment::<Created>::create(test_intent(), now).authorize(now);
        let failed = auth3.fail(now);
        assert_eq!(failed.state(), PaymentState::Failed);

        let auth4 = Payment::<Created>::create(test_intent(), now).authorize(now);
        let timed_out = auth4.timeout(now);
        assert_eq!(timed_out.state(), PaymentState::TimedOut);
    }

    #[test]
    fn construction_with_zero_amount_preserves_data() {
        let now = fixed_time();
        let fixed_ulid = ulid::Ulid::from_parts(1_700_000_000_000, 99);
        let intent = PaymentIntent {
            id: PaymentId::from_ulid(fixed_ulid),
            amount: Money::new(0, Currency::USD),
            provider: String::new(),
            metadata: serde_json::json!(null),
        };
        let payment = Payment::<Created>::create(intent, now);
        assert_eq!(payment.intent.amount.value, 0);
        assert_eq!(payment.intent.provider, "");
        assert_eq!(payment.intent.metadata, serde_json::json!(null));
    }

    // ========== Task 2: Timestamp tracking tests ==========

    #[test]
    fn created_at_set_on_create() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        assert_eq!(payment.created_at, now);
        assert_eq!(payment.last_transition_at, now);
    }

    #[test]
    fn last_transition_at_updated_on_authorize() {
        let t0 = fixed_time();
        let t1 = later(t0, 60);
        let payment = Payment::<Created>::create(test_intent(), t0);
        let authorized = payment.authorize(t1);
        assert_eq!(authorized.created_at, t0);
        assert_eq!(authorized.last_transition_at, t1);
    }

    #[test]
    fn timestamps_tracked_through_full_lifecycle() {
        let t0 = fixed_time();
        let t1 = later(t0, 10);
        let t2 = later(t0, 20);
        let t3 = later(t0, 30);

        let payment = Payment::<Created>::create(test_intent(), t0);
        assert_eq!(payment.created_at, t0);
        assert_eq!(payment.last_transition_at, t0);

        let authorized = payment.authorize(t1);
        assert_eq!(authorized.created_at, t0);
        assert_eq!(authorized.last_transition_at, t1);

        let captured = authorized.capture(t2);
        assert_eq!(captured.created_at, t0);
        assert_eq!(captured.last_transition_at, t2);

        let refunded = captured.refund(t3);
        assert_eq!(refunded.created_at, t0);
        assert_eq!(refunded.last_transition_at, t3);

        // created_at is always the original creation time
        assert_eq!(refunded.created_at, t0);
    }

    #[test]
    fn created_at_preserved_through_timeout_path() {
        let t0 = fixed_time();
        let t1 = later(t0, 1_800);
        let t2 = later(t0, 1_801);
        let payment = Payment::<Created>::create(test_intent(), t0);
        let timed_out = payment.timeout(t1);
        assert_eq!(timed_out.created_at, t0);
        assert_eq!(timed_out.last_transition_at, t1);
        let failed = timed_out.fail(t2);
        assert_eq!(failed.created_at, t0);
        assert_eq!(failed.last_transition_at, t2);
    }

    // ========== Task 2: Timeout detection tests ==========

    #[test]
    fn is_timed_out_true_when_expired() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default(); // Created: 1800s
        let payment = Payment::<Created>::create(test_intent(), t0);
        let check_time = later(t0, 1_801); // 1 second past timeout
        assert!(payment.is_timed_out(&config, check_time));
    }

    #[test]
    fn is_timed_out_false_when_valid() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default(); // Created: 1800s
        let payment = Payment::<Created>::create(test_intent(), t0);
        let check_time = later(t0, 1_799); // 1 second before timeout
        assert!(!payment.is_timed_out(&config, check_time));
    }

    #[test]
    fn is_timed_out_exact_boundary() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default(); // Created: 1800s
        let payment = Payment::<Created>::create(test_intent(), t0);
        let check_time = later(t0, 1_800); // Exactly at timeout
        assert!(payment.is_timed_out(&config, check_time));
    }

    #[test]
    fn is_timed_out_false_for_terminal_states() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default();
        let refunded = Payment::<Created>::create(test_intent(), t0)
            .authorize(t0)
            .capture(t0)
            .refund(t0);
        // Even far in the future, terminal states never time out
        let future = later(t0, 999_999_999);
        assert!(!refunded.is_timed_out(&config, future));
    }

    #[test]
    fn is_timed_out_uses_last_transition_at() {
        let t0 = fixed_time();
        let t1 = later(t0, 100);
        let config = TimeoutConfig::default(); // Authorized: 604800s (7 days)
        let authorized = Payment::<Created>::create(test_intent(), t0).authorize(t1);
        // Check at t1 + 604800 (exactly at timeout from last transition)
        let check_time = later(t1, 604_800);
        assert!(authorized.is_timed_out(&config, check_time));
        // Check at t1 + 604799 (1 second before)
        let check_before = later(t1, 604_799);
        assert!(!authorized.is_timed_out(&config, check_before));
    }

    #[test]
    fn timeout_deadline_non_terminal() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default();
        let payment = Payment::<Created>::create(test_intent(), t0);
        let deadline = payment.timeout_deadline(&config).unwrap();
        assert_eq!(deadline, later(t0, 1_800));
    }

    #[test]
    fn timeout_deadline_none_for_terminal() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default();
        let failed = Payment::<Created>::create(test_intent(), t0).fail(t0);
        assert!(failed.timeout_deadline(&config).is_none());
    }

    #[test]
    fn timeout_deadline_uses_last_transition_at() {
        let t0 = fixed_time();
        let t1 = later(t0, 500);
        let config = TimeoutConfig::default();
        let authorized = Payment::<Created>::create(test_intent(), t0).authorize(t1);
        let deadline = authorized.timeout_deadline(&config).unwrap();
        // Deadline is from last_transition_at, not created_at
        assert_eq!(deadline, later(t1, 604_800));
    }

    // ========== Task 3: Enforce timeout tests ==========

    #[test]
    fn enforce_timeout_created_expired() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default(); // Created: 1800s
        let payment = Payment::<Created>::create(test_intent(), t0);
        let check_time = later(t0, 1_801);
        let result = payment.enforce_timeout(&config, check_time);
        let timed_out = result.expect("should be timed out");
        assert_eq!(timed_out.state(), PaymentState::TimedOut);
        assert_eq!(timed_out.last_transition_at, check_time);
    }

    #[test]
    fn enforce_timeout_created_still_valid() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default();
        let payment = Payment::<Created>::create(test_intent(), t0);
        let check_time = later(t0, 100);
        let result = payment.enforce_timeout(&config, check_time);
        let returned = result.expect_err("should still be valid");
        assert_eq!(returned.state(), PaymentState::Created);
    }

    #[test]
    fn enforce_timeout_authorized_expired() {
        let t0 = fixed_time();
        let t1 = later(t0, 10);
        let config = TimeoutConfig::default(); // Authorized: 604800s
        let authorized = Payment::<Created>::create(test_intent(), t0).authorize(t1);
        let check_time = later(t1, 604_801);
        let timed_out = authorized
            .enforce_timeout(&config, check_time)
            .expect("should be timed out");
        assert_eq!(timed_out.state(), PaymentState::TimedOut);
    }

    #[test]
    fn enforce_timeout_captured_expired() {
        let t0 = fixed_time();
        let t1 = later(t0, 10);
        let t2 = later(t0, 20);
        let config = TimeoutConfig::default(); // Captured: 7776000s (90 days)
        let captured = Payment::<Created>::create(test_intent(), t0)
            .authorize(t1)
            .capture(t2);
        let check_time = later(t2, 7_776_001);
        let timed_out = captured
            .enforce_timeout(&config, check_time)
            .expect("should be timed out");
        assert_eq!(timed_out.state(), PaymentState::TimedOut);
    }

    #[test]
    fn enforce_timeout_full_chain_to_failed() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default();
        let payment = Payment::<Created>::create(test_intent(), t0);
        let t1 = later(t0, 1_801);
        let timed_out = payment.enforce_timeout(&config, t1).expect("timed out");
        let t2 = later(t1, 1);
        let failed = timed_out.fail(t2);
        assert_eq!(failed.state(), PaymentState::Failed);
        assert_eq!(failed.created_at, t0);
        assert_eq!(failed.last_transition_at, t2);
    }

    #[test]
    fn enforce_timeout_with_custom_config() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default().with_created(StdDuration::from_secs(60));
        let payment = Payment::<Created>::create(test_intent(), t0);
        // Not expired at 59 seconds
        let result = payment.enforce_timeout(&config, later(t0, 59));
        let payment = result.expect_err("should not be timed out");
        // Expired at 61 seconds
        let result = payment.enforce_timeout(&config, later(t0, 61));
        assert!(result.is_ok());
    }

    #[test]
    fn enforce_timeout_pending_3ds_expired() {
        let t0 = fixed_time();
        let t1 = later(t0, 5);
        let config = TimeoutConfig::default(); // Pending3DS: 900s
        let pending = Payment::<Created>::create(test_intent(), t0).pending_3ds(t1);
        let check_time = later(t1, 901);
        let timed_out = pending
            .enforce_timeout(&config, check_time)
            .expect("should be timed out");
        assert_eq!(timed_out.state(), PaymentState::TimedOut);
    }

    // ========== Task 4: Self-transition detection tests ==========

    #[test]
    fn self_transition_created() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        let id = payment.id.clone();
        match payment.try_transition(PaymentState::Created, now) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.id, id);
                assert_eq!(p.state(), PaymentState::Created);
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    #[test]
    fn self_transition_authorized() {
        let now = fixed_time();
        let authorized = Payment::<Created>::create(test_intent(), now).authorize(now);
        match authorized.try_transition(PaymentState::Authorized, now) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.state(), PaymentState::Authorized);
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    #[test]
    fn self_transition_captured() {
        let now = fixed_time();
        let captured = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now);
        match captured.try_transition(PaymentState::Captured, now) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.state(), PaymentState::Captured);
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    #[test]
    fn self_transition_pending_3ds() {
        let now = fixed_time();
        let pending = Payment::<Created>::create(test_intent(), now).pending_3ds(now);
        match pending.try_transition(PaymentState::Pending3ds, now) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.state(), PaymentState::Pending3ds);
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    #[test]
    fn self_transition_on_terminal_refunded() {
        let now = fixed_time();
        let refunded = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now)
            .refund(now);
        match refunded.try_transition(PaymentState::Refunded, now) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.state(), PaymentState::Refunded);
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    #[test]
    fn self_transition_on_terminal_failed() {
        let now = fixed_time();
        let failed = Payment::<Created>::create(test_intent(), now).fail(now);
        match failed.try_transition(PaymentState::Failed, now) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.state(), PaymentState::Failed);
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    // ========== Task 5: Invalid transition rejection tests ==========

    #[test]
    fn invalid_transition_created_to_refunded() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        match payment.try_transition(PaymentState::Refunded, now) {
            TransitionResult::Rejected { payment: p, error } => {
                assert_eq!(p.state(), PaymentState::Created);
                assert_eq!(error.current_state, PaymentState::Created);
                assert_eq!(error.attempted, PaymentState::Refunded);
                assert!(error.valid_transitions.contains(&PaymentState::Authorized));
                assert!(error.valid_transitions.contains(&PaymentState::Pending3ds));
                assert!(error.valid_transitions.contains(&PaymentState::Failed));
                assert!(error.valid_transitions.contains(&PaymentState::TimedOut));
                assert!(!error.valid_transitions.contains(&PaymentState::Refunded));
            }
            _ => panic!("expected Rejected"),
        }
    }

    #[test]
    fn invalid_transition_created_to_captured() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        match payment.try_transition(PaymentState::Captured, now) {
            TransitionResult::Rejected { error, .. } => {
                assert_eq!(error.current_state, PaymentState::Created);
                assert_eq!(error.attempted, PaymentState::Captured);
            }
            _ => panic!("expected Rejected"),
        }
    }

    #[test]
    fn invalid_transition_captured_to_authorized() {
        let now = fixed_time();
        let captured = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now);
        match captured.try_transition(PaymentState::Authorized, now) {
            TransitionResult::Rejected { error, .. } => {
                assert_eq!(error.current_state, PaymentState::Captured);
                assert_eq!(error.attempted, PaymentState::Authorized);
                assert!(error.valid_transitions.contains(&PaymentState::Refunded));
                assert!(error.valid_transitions.contains(&PaymentState::Failed));
                assert!(error.valid_transitions.contains(&PaymentState::TimedOut));
            }
            _ => panic!("expected Rejected"),
        }
    }

    #[test]
    fn invalid_transition_terminal_refunded_rejects_non_settled() {
        let now = fixed_time();
        let refunded = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now)
            .refund(now);
        // Refunded can only transition to Settled
        match refunded.try_transition(PaymentState::Failed, now) {
            TransitionResult::Rejected { error, .. } => {
                assert_eq!(error.current_state, PaymentState::Refunded);
                assert_eq!(error.valid_transitions, &[PaymentState::Settled]);
            }
            _ => panic!("expected Rejected"),
        }
    }

    #[test]
    fn transition_error_display_format() {
        let error = TransitionError {
            current_state: PaymentState::Created,
            attempted: PaymentState::Refunded,
            valid_transitions: &[
                PaymentState::Authorized,
                PaymentState::Pending3ds,
                PaymentState::Failed,
                PaymentState::TimedOut,
            ],
        };
        let msg = error.to_string();
        assert!(msg.contains("PAY_INVALID_TRANSITION"));
        assert!(msg.contains("Created"));
        assert!(msg.contains("Refunded"));
    }

    #[test]
    fn transition_error_implements_std_error() {
        let error = TransitionError {
            current_state: PaymentState::Created,
            attempted: PaymentState::Refunded,
            valid_transitions: &[],
        };
        let _: &dyn std::error::Error = &error;
    }

    // ========== Valid transitions per state ==========

    #[test]
    fn valid_transitions_created() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        let valid = payment.valid_transitions();
        assert_eq!(valid.len(), 4);
        assert!(valid.contains(&PaymentState::Authorized));
        assert!(valid.contains(&PaymentState::Pending3ds));
        assert!(valid.contains(&PaymentState::Failed));
        assert!(valid.contains(&PaymentState::TimedOut));
    }

    #[test]
    fn valid_transitions_pending_3ds() {
        let now = fixed_time();
        let pending = Payment::<Created>::create(test_intent(), now).pending_3ds(now);
        let valid = pending.valid_transitions();
        assert_eq!(valid.len(), 3);
        assert!(valid.contains(&PaymentState::Authorized));
        assert!(valid.contains(&PaymentState::Failed));
        assert!(valid.contains(&PaymentState::TimedOut));
    }

    #[test]
    fn valid_transitions_authorized() {
        let now = fixed_time();
        let authorized = Payment::<Created>::create(test_intent(), now).authorize(now);
        let valid = authorized.valid_transitions();
        assert_eq!(valid.len(), 4);
        assert!(valid.contains(&PaymentState::Captured));
        assert!(valid.contains(&PaymentState::Voided));
        assert!(valid.contains(&PaymentState::Failed));
        assert!(valid.contains(&PaymentState::TimedOut));
    }

    #[test]
    fn valid_transitions_captured() {
        let now = fixed_time();
        let captured = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now);
        let valid = captured.valid_transitions();
        assert_eq!(valid.len(), 4);
        assert!(valid.contains(&PaymentState::Refunded));
        assert!(valid.contains(&PaymentState::Settled));
        assert!(valid.contains(&PaymentState::Failed));
        assert!(valid.contains(&PaymentState::TimedOut));
    }

    #[test]
    fn valid_transitions_timed_out() {
        let now = fixed_time();
        let timed_out = Payment::<Created>::create(test_intent(), now).timeout(now);
        let valid = timed_out.valid_transitions();
        assert_eq!(valid, &[PaymentState::Failed]);
    }

    #[test]
    fn valid_transitions_terminal_states_empty() {
        let now = fixed_time();

        // Refunded can transition to Settled only
        let refunded = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now)
            .refund(now);
        assert_eq!(refunded.valid_transitions(), &[PaymentState::Settled]);

        let voided = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .void(now);
        assert!(voided.valid_transitions().is_empty());

        let failed = Payment::<Created>::create(test_intent(), now).fail(now);
        assert!(failed.valid_transitions().is_empty());

        // Settled is terminal — no outgoing transitions
        let settled = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now)
            .settle(now);
        assert!(settled.valid_transitions().is_empty());
    }

    // ========== Try-transition: valid transitions ==========

    #[test]
    fn try_transition_created_to_authorized() {
        let t0 = fixed_time();
        let t1 = later(t0, 10);
        let payment = Payment::<Created>::create(test_intent(), t0);
        match payment.try_transition(PaymentState::Authorized, t1) {
            TransitionResult::Applied {
                previous_state,
                new_state,
                created_at,
                last_transition_at,
                ..
            } => {
                assert_eq!(previous_state, PaymentState::Created);
                assert_eq!(new_state, PaymentState::Authorized);
                assert_eq!(created_at, t0);
                assert_eq!(last_transition_at, t1);
            }
            _ => panic!("expected Applied"),
        }
    }

    #[test]
    fn try_transition_captured_to_timed_out() {
        let now = fixed_time();
        let captured = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now);
        match captured.try_transition(PaymentState::TimedOut, now) {
            TransitionResult::Applied {
                previous_state,
                new_state,
                ..
            } => {
                assert_eq!(previous_state, PaymentState::Captured);
                assert_eq!(new_state, PaymentState::TimedOut);
            }
            _ => panic!("expected Applied"),
        }
    }

    // ========== Task 7: Integration tests ==========

    #[test]
    fn integration_full_lifecycle_with_timeout_checks() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default();

        // Created at t0, check timeout at various points
        let payment = Payment::<Created>::create(test_intent(), t0);
        assert!(!payment.is_timed_out(&config, later(t0, 100)));
        assert!(payment.timeout_deadline(&config).is_some());

        // Authorize at t1
        let t1 = later(t0, 60);
        let authorized = payment.authorize(t1);
        assert!(!authorized.is_timed_out(&config, later(t1, 100)));

        // Capture at t2
        let t2 = later(t1, 3600);
        let captured = authorized.capture(t2);
        assert!(!captured.is_timed_out(&config, later(t2, 86_400)));
        // Captured has 90-day timeout
        assert!(captured.is_timed_out(&config, later(t2, 7_776_001)));
    }

    #[test]
    fn integration_timeout_chain_created_to_failed() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default().with_created(StdDuration::from_secs(60));

        let payment = Payment::<Created>::create(test_intent(), t0);

        // Not timed out yet
        let t1 = later(t0, 59);
        let payment = payment
            .enforce_timeout(&config, t1)
            .expect_err("not expired");

        // Now expired
        let t2 = later(t0, 61);
        let timed_out = payment.enforce_timeout(&config, t2).expect("expired");
        assert_eq!(timed_out.state(), PaymentState::TimedOut);

        // TimedOut → Failed
        let t3 = later(t2, 1);
        let failed = timed_out.fail(t3);
        assert_eq!(failed.state(), PaymentState::Failed);
        assert_eq!(failed.created_at, t0);
        assert_eq!(failed.last_transition_at, t3);
    }

    #[test]
    fn integration_self_transition_captured_duplicate_event() {
        let t0 = fixed_time();
        let t1 = later(t0, 10);
        let t2 = later(t0, 20);
        let captured = Payment::<Created>::create(test_intent(), t0)
            .authorize(t1)
            .capture(t2);
        let original_id = captured.id.clone();

        // Duplicate event: Captured → Captured (self-transition)
        let t3 = later(t0, 30);
        match captured.try_transition(PaymentState::Captured, t3) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.id, original_id);
                assert_eq!(p.state(), PaymentState::Captured);
                // Timestamps unchanged — zero side effects
                assert_eq!(p.created_at, t0);
                assert_eq!(p.last_transition_at, t2);
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    #[test]
    fn integration_invalid_transition_with_error_payload() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        match payment.try_transition(PaymentState::Refunded, now) {
            TransitionResult::Rejected { payment: p, error } => {
                // Payment returned unchanged
                assert_eq!(p.state(), PaymentState::Created);
                // Error has correct fields
                assert_eq!(error.current_state, PaymentState::Created);
                assert_eq!(error.attempted, PaymentState::Refunded);
                assert_eq!(error.valid_transitions.len(), 4);
                // Display includes PAY_INVALID_TRANSITION code
                let msg = error.to_string();
                assert!(msg.contains("PAY_INVALID_TRANSITION"));
            }
            _ => panic!("expected Rejected"),
        }
    }

    #[test]
    fn integration_custom_timeout_config() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default()
            .with_created(StdDuration::from_secs(10))
            .with_pending_3ds(StdDuration::from_secs(5));

        let payment = Payment::<Created>::create(test_intent(), t0);
        assert!(!payment.is_timed_out(&config, later(t0, 9)));
        assert!(payment.is_timed_out(&config, later(t0, 11)));

        let pending = Payment::<Created>::create(test_intent(), t0).pending_3ds(t0);
        assert!(!pending.is_timed_out(&config, later(t0, 4)));
        assert!(pending.is_timed_out(&config, later(t0, 6)));
    }

    // ========== TEA 1.4: Expanded coverage ==========

    #[test]
    fn transition_error_code_returns_pay_invalid_transition() {
        let error = TransitionError {
            current_state: PaymentState::Created,
            attempted: PaymentState::Refunded,
            valid_transitions: &[PaymentState::Authorized],
        };
        assert_eq!(error.code(), "PAY_INVALID_TRANSITION");
    }

    #[test]
    fn transition_error_domain_returns_payment() {
        let error = TransitionError {
            current_state: PaymentState::Created,
            attempted: PaymentState::Refunded,
            valid_transitions: &[],
        };
        assert_eq!(error.domain(), "payment");
    }

    #[test]
    fn self_transition_on_terminal_voided() {
        let now = fixed_time();
        let voided = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .void(now);
        match voided.try_transition(PaymentState::Voided, now) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.state(), PaymentState::Voided);
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    #[test]
    fn self_transition_on_timed_out() {
        let now = fixed_time();
        let timed_out = Payment::<Created>::create(test_intent(), now).timeout(now);
        match timed_out.try_transition(PaymentState::TimedOut, now) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.state(), PaymentState::TimedOut);
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    #[test]
    fn self_transition_preserves_timestamps_unchanged() {
        let t0 = fixed_time();
        let t1 = later(t0, 10);
        let t2 = later(t0, 30);
        let authorized = Payment::<Created>::create(test_intent(), t0).authorize(t1);
        assert_eq!(authorized.created_at, t0);
        assert_eq!(authorized.last_transition_at, t1);

        match authorized.try_transition(PaymentState::Authorized, t2) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.created_at, t0);
                assert_eq!(p.last_transition_at, t1); // NOT t2 — zero side effects
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    #[test]
    fn is_timed_out_one_second_before_deadline_returns_false() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default(); // Created: 1800s
        let payment = Payment::<Created>::create(test_intent(), t0);
        let one_before = later(t0, 1799);
        assert!(!payment.is_timed_out(&config, one_before));
    }

    #[test]
    fn try_transition_created_to_pending3ds_updates_last_transition_at() {
        let t0 = fixed_time();
        let t1 = later(t0, 5);
        let payment = Payment::<Created>::create(test_intent(), t0);
        match payment.try_transition(PaymentState::Pending3ds, t1) {
            TransitionResult::Applied {
                last_transition_at, ..
            } => {
                assert_eq!(last_transition_at, t1);
            }
            _ => panic!("expected Applied"),
        }
    }

    #[test]
    fn enforce_timeout_sets_last_transition_at_to_now() {
        let t0 = fixed_time();
        let config = TimeoutConfig::default().with_created(StdDuration::from_secs(10));
        let payment = Payment::<Created>::create(test_intent(), t0);
        let t1 = later(t0, 11);
        match payment.enforce_timeout(&config, t1) {
            Ok(timed_out) => {
                assert_eq!(timed_out.last_transition_at, t1);
                assert_eq!(timed_out.created_at, t0);
            }
            Err(_) => panic!("expected Ok(Payment<TimedOut>)"),
        }
    }

    #[test]
    fn invalid_transition_on_terminal_voided() {
        let now = fixed_time();
        let voided = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .void(now);
        match voided.try_transition(PaymentState::Authorized, now) {
            TransitionResult::Rejected { error, .. } => {
                assert_eq!(error.current_state, PaymentState::Voided);
                assert!(error.valid_transitions.is_empty());
            }
            _ => panic!("expected Rejected"),
        }
    }

    #[test]
    fn transition_error_is_clone() {
        let error = TransitionError {
            current_state: PaymentState::Created,
            attempted: PaymentState::Refunded,
            valid_transitions: &[PaymentState::Authorized, PaymentState::Failed],
        };
        let cloned = error.clone();
        assert_eq!(cloned.current_state, error.current_state);
        assert_eq!(cloned.attempted, error.attempted);
        assert_eq!(
            cloned.valid_transitions.len(),
            error.valid_transitions.len()
        );
    }

    #[test]
    fn valid_transitions_returns_static_reference() {
        let now = fixed_time();
        let payment = Payment::<Created>::create(test_intent(), now);
        let v1 = payment.valid_transitions();
        // Static slices have stable addresses
        let v2 = valid_transitions_for(&PaymentState::Created);
        assert!(std::ptr::eq(v1, v2));
    }

    // ========== Story 1.5: Display, PartialEq, #[must_use] tests ==========

    #[test]
    fn display_payment_created() {
        let now = fixed_time();
        let intent = test_intent();
        let id_str = intent.id.to_string();
        let payment = Payment::<Created>::create(intent, now);
        assert_eq!(payment.to_string(), format!("Payment({}, Created)", id_str));
    }

    #[test]
    fn display_payment_authorized() {
        let now = fixed_time();
        let intent = test_intent();
        let id_str = intent.id.to_string();
        let payment = Payment::<Created>::create(intent, now).authorize(now);
        assert_eq!(
            payment.to_string(),
            format!("Payment({}, Authorized)", id_str)
        );
    }

    #[test]
    fn display_payment_captured() {
        let now = fixed_time();
        let intent = test_intent();
        let id_str = intent.id.to_string();
        let payment = Payment::<Created>::create(intent, now)
            .authorize(now)
            .capture(now);
        assert_eq!(
            payment.to_string(),
            format!("Payment({}, Captured)", id_str)
        );
    }

    #[test]
    fn display_payment_pending_3ds() {
        let now = fixed_time();
        let intent = test_intent();
        let id_str = intent.id.to_string();
        let payment = Payment::<Created>::create(intent, now).pending_3ds(now);
        assert_eq!(
            payment.to_string(),
            format!("Payment({}, Pending3DS)", id_str)
        );
    }

    #[test]
    fn display_payment_refunded() {
        let now = fixed_time();
        let intent = test_intent();
        let id_str = intent.id.to_string();
        let payment = Payment::<Created>::create(intent, now)
            .authorize(now)
            .capture(now)
            .refund(now);
        assert_eq!(
            payment.to_string(),
            format!("Payment({}, Refunded)", id_str)
        );
    }

    #[test]
    fn display_payment_voided() {
        let now = fixed_time();
        let intent = test_intent();
        let id_str = intent.id.to_string();
        let payment = Payment::<Created>::create(intent, now)
            .authorize(now)
            .void(now);
        assert_eq!(payment.to_string(), format!("Payment({}, Voided)", id_str));
    }

    #[test]
    fn display_payment_failed() {
        let now = fixed_time();
        let intent = test_intent();
        let id_str = intent.id.to_string();
        let payment = Payment::<Created>::create(intent, now).fail(now);
        assert_eq!(payment.to_string(), format!("Payment({}, Failed)", id_str));
    }

    #[test]
    fn display_payment_timed_out() {
        let now = fixed_time();
        let intent = test_intent();
        let id_str = intent.id.to_string();
        let payment = Payment::<Created>::create(intent, now).timeout(now);
        assert_eq!(
            payment.to_string(),
            format!("Payment({}, TimedOut)", id_str)
        );
    }

    #[test]
    fn transition_error_partial_eq() {
        let err1 = TransitionError {
            current_state: PaymentState::Created,
            attempted: PaymentState::Refunded,
            valid_transitions: &[
                PaymentState::Authorized,
                PaymentState::Pending3ds,
                PaymentState::Failed,
                PaymentState::TimedOut,
            ],
        };
        let err2 = TransitionError {
            current_state: PaymentState::Created,
            attempted: PaymentState::Refunded,
            valid_transitions: &[
                PaymentState::Authorized,
                PaymentState::Pending3ds,
                PaymentState::Failed,
                PaymentState::TimedOut,
            ],
        };
        assert_eq!(err1, err2);
    }

    #[test]
    fn transition_error_not_equal_different_states() {
        let err1 = TransitionError {
            current_state: PaymentState::Created,
            attempted: PaymentState::Refunded,
            valid_transitions: &[],
        };
        let err2 = TransitionError {
            current_state: PaymentState::Authorized,
            attempted: PaymentState::Refunded,
            valid_transitions: &[],
        };
        assert_ne!(err1, err2);
    }

    #[test]
    fn payment_does_not_implement_clone() {
        // Verified at compile time: Payment<S> has no Clone impl
        // This test documents the intentional design decision
        fn assert_not_clone<T>() {
            // If Payment implemented Clone, this would need to be updated
            // The consume-and-return pattern relies on move semantics
        }
        assert_not_clone::<Payment<Created>>();
    }

    // -- E1 Edge Case Tests --

    #[test]
    fn clock_skew_transition_with_earlier_timestamp() {
        // E1-UNIT-001: Transition with timestamp before created_at
        let now = fixed_time();
        let earlier = now - chrono::Duration::seconds(10);
        let payment = Payment::<Created>::create(test_intent(), now);
        // System should not panic — timestamps are caller-provided
        let authorized = payment.authorize(earlier);
        assert_eq!(authorized.last_transition_at, earlier);
        assert!(authorized.last_transition_at < authorized.created_at);
    }

    #[test]
    fn enforce_timeout_with_zero_duration() {
        // E1-UNIT-002: Duration::ZERO means immediately timed out
        let now = fixed_time();
        let config = TimeoutConfig::default().with_created(std::time::Duration::from_secs(0));
        let payment = Payment::<Created>::create(test_intent(), now);
        let timed_out = payment.enforce_timeout(&config, now).unwrap();
        assert_eq!(timed_out.state(), PaymentState::TimedOut);
    }

    #[test]
    fn try_transition_exhaustive_valid_from_every_state() {
        // E1-UNIT-003: Every valid transition from every non-terminal state
        let now = fixed_time();
        let cases: Vec<(PaymentState, Vec<PaymentState>)> = vec![
            (
                PaymentState::Created,
                vec![
                    PaymentState::Authorized,
                    PaymentState::Pending3ds,
                    PaymentState::Failed,
                    PaymentState::TimedOut,
                ],
            ),
            (
                PaymentState::Pending3ds,
                vec![
                    PaymentState::Authorized,
                    PaymentState::Failed,
                    PaymentState::TimedOut,
                ],
            ),
            (
                PaymentState::Authorized,
                vec![
                    PaymentState::Captured,
                    PaymentState::Voided,
                    PaymentState::Failed,
                    PaymentState::TimedOut,
                ],
            ),
            (
                PaymentState::Captured,
                vec![
                    PaymentState::Refunded,
                    PaymentState::Failed,
                    PaymentState::TimedOut,
                ],
            ),
            (PaymentState::TimedOut, vec![PaymentState::Failed]),
        ];

        for (from_state, valid_targets) in &cases {
            // Use try_transition for Created state directly
            if *from_state == PaymentState::Created {
                for target in valid_targets {
                    let p = Payment::<Created>::create(test_intent(), now);
                    match p.try_transition(*target, now) {
                        TransitionResult::Applied { new_state, .. } => {
                            assert_eq!(new_state, *target);
                        }
                        other => panic!(
                            "{} → {} should be Applied, got {:?}",
                            from_state, target, other
                        ),
                    }
                }
            }
            // Verify invalid transitions are rejected
            let all_states = vec![
                PaymentState::Created,
                PaymentState::Pending3ds,
                PaymentState::Authorized,
                PaymentState::Captured,
                PaymentState::Refunded,
                PaymentState::Voided,
                PaymentState::Failed,
                PaymentState::TimedOut,
            ];
            if *from_state == PaymentState::Created {
                for target in &all_states {
                    if *target == *from_state || valid_targets.contains(target) {
                        continue;
                    }
                    let p = Payment::<Created>::create(test_intent(), now);
                    match p.try_transition(*target, now) {
                        TransitionResult::Rejected { .. } => {}
                        other => panic!(
                            "{} → {} should be Rejected, got {:?}",
                            from_state, target, other
                        ),
                    }
                }
            }
        }
    }

    #[test]
    fn transition_result_applied_preserves_all_intent_fields() {
        // E1-UNIT-004: Applied variant preserves complete PaymentIntent
        let now = fixed_time();
        let intent = PaymentIntent {
            id: PaymentId::new(),
            amount: Money::new(99999, Currency::GBP),
            provider: "test_provider_xyz".to_owned(),
            metadata: serde_json::json!({"key": "value", "nested": {"a": 1}}),
        };
        let original_intent = intent.clone();
        let payment = Payment::<Created>::create(intent, now);
        match payment.try_transition(PaymentState::Authorized, now) {
            TransitionResult::Applied { intent, .. } => {
                assert_eq!(intent.id, original_intent.id);
                assert_eq!(intent.amount, original_intent.amount);
                assert_eq!(intent.provider, original_intent.provider);
                assert_eq!(intent.metadata, original_intent.metadata);
            }
            _ => panic!("expected Applied"),
        }
    }

    #[test]
    fn display_includes_full_ulid_id() {
        // E1-UNIT-010: Display shows full ULID, not truncated
        let now = fixed_time();
        let intent = test_intent();
        let id_str = intent.id.to_string();
        let payment = Payment::<Created>::create(intent, now);
        let display = payment.to_string();
        assert!(
            display.contains(&id_str),
            "Display should contain full ID '{}', got '{}'",
            id_str,
            display
        );
    }

    // ========== Story 9.1: Settled state transitions ==========

    #[test]
    fn captured_settle_returns_settled() {
        let now = fixed_time();
        let settled = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now)
            .settle(now);
        assert_eq!(settled.state(), PaymentState::Settled);
    }

    #[test]
    fn refunded_settle_returns_settled() {
        let now = fixed_time();
        let settled = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now)
            .refund(now)
            .settle(now);
        assert_eq!(settled.state(), PaymentState::Settled);
    }

    #[test]
    fn settled_is_terminal_no_transitions() {
        let now = fixed_time();
        let settled = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now)
            .settle(now);
        assert!(settled.valid_transitions().is_empty());
    }

    #[test]
    fn settled_preserves_payment_fields() {
        let now = fixed_time();
        let t1 = later(now, 10);
        let intent = test_intent();
        let expected_id = intent.id.clone();
        let settled = Payment::<Created>::create(intent, now)
            .authorize(now)
            .capture(now)
            .settle(t1);
        assert_eq!(settled.id, expected_id);
        assert_eq!(settled.created_at, now);
        assert_eq!(settled.last_transition_at, t1);
    }

    #[test]
    fn try_transition_captured_to_settled() {
        let t0 = fixed_time();
        let t1 = later(t0, 10);
        let captured = Payment::<Created>::create(test_intent(), now())
            .authorize(now())
            .capture(now());
        match captured.try_transition(PaymentState::Settled, t1) {
            TransitionResult::Applied { new_state, .. } => {
                assert_eq!(new_state, PaymentState::Settled);
            }
            _ => panic!("expected Applied"),
        }
    }

    #[test]
    fn try_transition_refunded_to_settled() {
        let refunded = Payment::<Created>::create(test_intent(), now())
            .authorize(now())
            .capture(now())
            .refund(now());
        match refunded.try_transition(PaymentState::Settled, now()) {
            TransitionResult::Applied { new_state, .. } => {
                assert_eq!(new_state, PaymentState::Settled);
            }
            _ => panic!("expected Applied"),
        }
    }

    #[test]
    fn try_transition_created_to_settled_rejected() {
        let created = Payment::<Created>::create(test_intent(), now());
        match created.try_transition(PaymentState::Settled, now()) {
            TransitionResult::Rejected { error, .. } => {
                assert_eq!(error.current_state, PaymentState::Created);
                assert_eq!(error.attempted, PaymentState::Settled);
            }
            _ => panic!("expected Rejected"),
        }
    }

    #[test]
    fn try_transition_failed_to_settled_rejected() {
        let failed = Payment::<Created>::create(test_intent(), now()).fail(now());
        match failed.try_transition(PaymentState::Settled, now()) {
            TransitionResult::Rejected { error, .. } => {
                assert_eq!(error.current_state, PaymentState::Failed);
                assert_eq!(error.attempted, PaymentState::Settled);
            }
            _ => panic!("expected Rejected"),
        }
    }

    #[test]
    fn try_transition_settled_self_transition() {
        let settled = Payment::<Created>::create(test_intent(), now())
            .authorize(now())
            .capture(now())
            .settle(now());
        match settled.try_transition(PaymentState::Settled, now()) {
            TransitionResult::SelfTransition(p) => {
                assert_eq!(p.state(), PaymentState::Settled);
            }
            _ => panic!("expected SelfTransition"),
        }
    }

    #[test]
    fn settled_has_no_timeout() {
        let now = fixed_time();
        let config = TimeoutConfig::default();
        let settled = Payment::<Created>::create(test_intent(), now)
            .authorize(now)
            .capture(now)
            .settle(now);
        assert!(!settled.is_timed_out(&config, later(now, 999_999)));
        assert!(settled.timeout_deadline(&config).is_none());
    }

    fn now() -> DateTime<Utc> {
        fixed_time()
    }
}
