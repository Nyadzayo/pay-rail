use std::time::Duration;

use crate::payment::types::PaymentState;

/// Per-state timeout configuration for the payment state machine.
///
/// Each non-terminal state has a configurable timeout duration.
/// Terminal states (Refunded, Voided, Failed, TimedOut) return `None`.
///
/// # Example
///
/// ```
/// use payrail_core::TimeoutConfig;
/// use std::time::Duration;
///
/// let config = TimeoutConfig::default()
///     .with_created(Duration::from_secs(60))
///     .with_authorized(Duration::from_secs(86_400));
///
/// assert_eq!(config.created, Duration::from_secs(60));
/// assert_eq!(config.authorized, Duration::from_secs(86_400));
/// ```
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Timeout for `Created` state (default: 30 minutes).
    pub created: Duration,
    /// Timeout for `Pending3DS` state (default: 15 minutes).
    pub pending_3ds: Duration,
    /// Timeout for `Authorized` state (default: 7 days).
    pub authorized: Duration,
    /// Timeout for `Captured` state (default: 90 days).
    pub captured: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            created: Duration::from_secs(1_800),      // 30 minutes
            pending_3ds: Duration::from_secs(900),    // 15 minutes
            authorized: Duration::from_secs(604_800), // 7 days
            captured: Duration::from_secs(7_776_000), // 90 days
        }
    }
}

impl TimeoutConfig {
    /// Returns the timeout duration for a given payment state, or `None` for terminal states.
    pub fn timeout_for(&self, state: &PaymentState) -> Option<Duration> {
        match state {
            PaymentState::Created => Some(self.created),
            PaymentState::Pending3ds => Some(self.pending_3ds),
            PaymentState::Authorized => Some(self.authorized),
            PaymentState::Captured => Some(self.captured),
            PaymentState::Refunded
            | PaymentState::Voided
            | PaymentState::Failed
            | PaymentState::TimedOut
            | PaymentState::Settled => None,
        }
    }

    /// Sets the timeout for the `Created` state.
    pub fn with_created(mut self, duration: Duration) -> Self {
        self.created = duration;
        self
    }

    /// Sets the timeout for the `Pending3DS` state.
    pub fn with_pending_3ds(mut self, duration: Duration) -> Self {
        self.pending_3ds = duration;
        self
    }

    /// Sets the timeout for the `Authorized` state.
    pub fn with_authorized(mut self, duration: Duration) -> Self {
        self.authorized = duration;
        self
    }

    /// Sets the timeout for the `Captured` state.
    pub fn with_captured(mut self, duration: Duration) -> Self {
        self.captured = duration;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_created_30_minutes() {
        let config = TimeoutConfig::default();
        assert_eq!(config.created, Duration::from_secs(1_800));
    }

    #[test]
    fn default_pending_3ds_15_minutes() {
        let config = TimeoutConfig::default();
        assert_eq!(config.pending_3ds, Duration::from_secs(900));
    }

    #[test]
    fn default_authorized_7_days() {
        let config = TimeoutConfig::default();
        assert_eq!(config.authorized, Duration::from_secs(604_800));
    }

    #[test]
    fn default_captured_90_days() {
        let config = TimeoutConfig::default();
        assert_eq!(config.captured, Duration::from_secs(7_776_000));
    }

    #[test]
    fn timeout_for_non_terminal_states() {
        let config = TimeoutConfig::default();
        assert_eq!(
            config.timeout_for(&PaymentState::Created),
            Some(Duration::from_secs(1_800))
        );
        assert_eq!(
            config.timeout_for(&PaymentState::Pending3ds),
            Some(Duration::from_secs(900))
        );
        assert_eq!(
            config.timeout_for(&PaymentState::Authorized),
            Some(Duration::from_secs(604_800))
        );
        assert_eq!(
            config.timeout_for(&PaymentState::Captured),
            Some(Duration::from_secs(7_776_000))
        );
    }

    #[test]
    fn timeout_for_terminal_states_returns_none() {
        let config = TimeoutConfig::default();
        assert_eq!(config.timeout_for(&PaymentState::Refunded), None);
        assert_eq!(config.timeout_for(&PaymentState::Voided), None);
        assert_eq!(config.timeout_for(&PaymentState::Failed), None);
        assert_eq!(config.timeout_for(&PaymentState::TimedOut), None);
    }

    #[test]
    fn with_created_overrides_default() {
        let config = TimeoutConfig::default().with_created(Duration::from_secs(300));
        assert_eq!(config.created, Duration::from_secs(300));
        assert_eq!(config.pending_3ds, Duration::from_secs(900));
    }

    #[test]
    fn with_pending_3ds_overrides_default() {
        let config = TimeoutConfig::default().with_pending_3ds(Duration::from_secs(60));
        assert_eq!(config.pending_3ds, Duration::from_secs(60));
    }

    #[test]
    fn with_authorized_overrides_default() {
        let config = TimeoutConfig::default().with_authorized(Duration::from_secs(86_400));
        assert_eq!(config.authorized, Duration::from_secs(86_400));
    }

    #[test]
    fn with_captured_overrides_default() {
        let config = TimeoutConfig::default().with_captured(Duration::from_secs(2_592_000));
        assert_eq!(config.captured, Duration::from_secs(2_592_000));
    }

    #[test]
    fn builder_chaining() {
        let config = TimeoutConfig::default()
            .with_created(Duration::from_secs(60))
            .with_pending_3ds(Duration::from_secs(30))
            .with_authorized(Duration::from_secs(3600))
            .with_captured(Duration::from_secs(86_400));
        assert_eq!(config.created, Duration::from_secs(60));
        assert_eq!(config.pending_3ds, Duration::from_secs(30));
        assert_eq!(config.authorized, Duration::from_secs(3600));
        assert_eq!(config.captured, Duration::from_secs(86_400));
    }

    #[test]
    fn timeout_config_is_clone() {
        let config = TimeoutConfig::default().with_created(Duration::from_secs(42));
        let cloned = config.clone();
        assert_eq!(cloned.created, Duration::from_secs(42));
        assert_eq!(cloned.pending_3ds, config.pending_3ds);
        assert_eq!(cloned.authorized, config.authorized);
        assert_eq!(cloned.captured, config.captured);
    }
}
