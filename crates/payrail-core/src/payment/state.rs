use crate::payment::types::PaymentState;

mod sealed {
    pub trait Sealed {}
}

/// Marker trait for payment states. Sealed to prevent external implementations.
pub trait PaymentStateMarker: sealed::Sealed {
    /// Returns the corresponding runtime `PaymentState` variant.
    fn runtime_state() -> PaymentState;
}

/// Payment has been created but not yet authorized.
#[derive(Debug, Clone)]
pub struct Created;

/// Payment is awaiting 3D Secure verification.
#[derive(Debug, Clone)]
pub struct Pending3DS;

/// Payment has been authorized by the provider.
#[derive(Debug, Clone)]
pub struct Authorized;

/// Payment has been captured (funds collected).
#[derive(Debug, Clone)]
pub struct Captured;

/// Payment has been refunded (terminal).
#[derive(Debug, Clone)]
pub struct Refunded;

/// Payment has been voided (terminal).
#[derive(Debug, Clone)]
pub struct Voided;

/// Payment has failed (terminal).
#[derive(Debug, Clone)]
pub struct Failed;

/// Payment has timed out.
#[derive(Debug, Clone)]
pub struct TimedOut;

// Sealed trait implementations
impl sealed::Sealed for Created {}
impl sealed::Sealed for Pending3DS {}
impl sealed::Sealed for Authorized {}
impl sealed::Sealed for Captured {}
impl sealed::Sealed for Refunded {}
impl sealed::Sealed for Voided {}
impl sealed::Sealed for Failed {}
impl sealed::Sealed for TimedOut {}

// PaymentStateMarker implementations
impl PaymentStateMarker for Created {
    fn runtime_state() -> PaymentState {
        PaymentState::Created
    }
}

impl PaymentStateMarker for Pending3DS {
    fn runtime_state() -> PaymentState {
        PaymentState::Pending3ds
    }
}

impl PaymentStateMarker for Authorized {
    fn runtime_state() -> PaymentState {
        PaymentState::Authorized
    }
}

impl PaymentStateMarker for Captured {
    fn runtime_state() -> PaymentState {
        PaymentState::Captured
    }
}

impl PaymentStateMarker for Refunded {
    fn runtime_state() -> PaymentState {
        PaymentState::Refunded
    }
}

impl PaymentStateMarker for Voided {
    fn runtime_state() -> PaymentState {
        PaymentState::Voided
    }
}

impl PaymentStateMarker for Failed {
    fn runtime_state() -> PaymentState {
        PaymentState::Failed
    }
}

impl PaymentStateMarker for TimedOut {
    fn runtime_state() -> PaymentState {
        PaymentState::TimedOut
    }
}

/// Marker trait for states that support automatic timeout enforcement.
/// Implemented for all non-terminal states that can auto-transition to TimedOut.
pub trait TimeoutEnforceable: PaymentStateMarker {}

impl TimeoutEnforceable for Created {}
impl TimeoutEnforceable for Pending3DS {}
impl TimeoutEnforceable for Authorized {}
impl TimeoutEnforceable for Captured {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_marker_types_are_zero_sized() {
        assert_eq!(std::mem::size_of::<Created>(), 0);
        assert_eq!(std::mem::size_of::<Pending3DS>(), 0);
        assert_eq!(std::mem::size_of::<Authorized>(), 0);
        assert_eq!(std::mem::size_of::<Captured>(), 0);
        assert_eq!(std::mem::size_of::<Refunded>(), 0);
        assert_eq!(std::mem::size_of::<Voided>(), 0);
        assert_eq!(std::mem::size_of::<Failed>(), 0);
        assert_eq!(std::mem::size_of::<TimedOut>(), 0);
    }

    #[test]
    fn all_markers_implement_payment_state_marker() {
        // If these compile, the trait is implemented
        fn assert_marker<T: PaymentStateMarker>() {}
        assert_marker::<Created>();
        assert_marker::<Pending3DS>();
        assert_marker::<Authorized>();
        assert_marker::<Captured>();
        assert_marker::<Refunded>();
        assert_marker::<Voided>();
        assert_marker::<Failed>();
        assert_marker::<TimedOut>();
    }

    #[test]
    fn all_markers_are_debug_clone() {
        fn assert_debug_clone<T: std::fmt::Debug + Clone>() {}
        assert_debug_clone::<Created>();
        assert_debug_clone::<Pending3DS>();
        assert_debug_clone::<Authorized>();
        assert_debug_clone::<Captured>();
        assert_debug_clone::<Refunded>();
        assert_debug_clone::<Voided>();
        assert_debug_clone::<Failed>();
        assert_debug_clone::<TimedOut>();
    }

    #[test]
    fn exactly_eight_states_exist() {
        // Verify runtime_state() returns the correct variant for each
        let states = [
            Created::runtime_state(),
            Pending3DS::runtime_state(),
            Authorized::runtime_state(),
            Captured::runtime_state(),
            Refunded::runtime_state(),
            Voided::runtime_state(),
            Failed::runtime_state(),
            TimedOut::runtime_state(),
        ];
        assert_eq!(states.len(), 8);
        // All unique
        for (i, a) in states.iter().enumerate() {
            for (j, b) in states.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "States at index {} and {} are equal", i, j);
                }
            }
        }
    }

    #[test]
    fn runtime_state_mapping_is_correct() {
        assert_eq!(Created::runtime_state(), PaymentState::Created);
        assert_eq!(Pending3DS::runtime_state(), PaymentState::Pending3ds);
        assert_eq!(Authorized::runtime_state(), PaymentState::Authorized);
        assert_eq!(Captured::runtime_state(), PaymentState::Captured);
        assert_eq!(Refunded::runtime_state(), PaymentState::Refunded);
        assert_eq!(Voided::runtime_state(), PaymentState::Voided);
        assert_eq!(Failed::runtime_state(), PaymentState::Failed);
        assert_eq!(TimedOut::runtime_state(), PaymentState::TimedOut);
    }

    #[test]
    fn all_markers_have_unit_alignment() {
        assert_eq!(std::mem::align_of::<Created>(), 1);
        assert_eq!(std::mem::align_of::<Pending3DS>(), 1);
        assert_eq!(std::mem::align_of::<Authorized>(), 1);
        assert_eq!(std::mem::align_of::<Captured>(), 1);
        assert_eq!(std::mem::align_of::<Refunded>(), 1);
        assert_eq!(std::mem::align_of::<Voided>(), 1);
        assert_eq!(std::mem::align_of::<Failed>(), 1);
        assert_eq!(std::mem::align_of::<TimedOut>(), 1);
    }
}
