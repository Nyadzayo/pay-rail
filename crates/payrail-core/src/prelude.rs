/// Commonly-used types for PayRail payment processing.
///
/// Import everything with `use payrail_core::prelude::*` to get the types
/// most developers need for working with the payment state machine.
///
/// # Included
///
/// - **Payment types**: [`Payment`], all state markers ([`Created`], [`Authorized`], etc.)
/// - **Money types**: [`Money`], [`Currency`]
/// - **Intent & state**: [`PaymentIntent`], [`PaymentState`]
/// - **IDs**: [`PaymentId`]
/// - **Timeouts**: [`TimeoutConfig`]
/// - **Transitions**: [`TransitionResult`], [`TransitionError`]
///
/// # Not included
///
/// Advanced or internal types live at `payrail_core::` directly:
/// - Event store: `EventStore`, `SqliteEventStore`, `EventStoreError`
/// - Event types: `EventId`, `EventType`, `CanonicalEvent`, `RawWebhook`, `EventEnvelope`
/// - Error infrastructure: `ErrorCode`, `PayRailError`
/// - Internal markers: `PaymentCommand`, `PaymentStateMarker`, `TimeoutEnforceable`
pub use crate::id::PaymentId;
pub use crate::payment::{
    Authorized, Captured, Created, Currency, Failed, Money, Payment, PaymentIntent, PaymentState,
    Pending3DS, Refunded, Settled, TimedOut, TimeoutConfig, TransitionError, TransitionResult,
    Voided,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prelude_provides_payment_type() {
        fn assert_type<T>() {}
        assert_type::<Payment<Created>>();
        assert_type::<Payment<Authorized>>();
        assert_type::<Payment<Captured>>();
        assert_type::<Payment<Pending3DS>>();
        assert_type::<Payment<Refunded>>();
        assert_type::<Payment<Voided>>();
        assert_type::<Payment<Failed>>();
        assert_type::<Payment<TimedOut>>();
        assert_type::<Payment<Settled>>();
    }

    #[test]
    fn prelude_provides_money_types() {
        let _m = Money::new(100, Currency::ZAR);
    }

    #[test]
    fn prelude_provides_intent_and_state() {
        fn assert_type<T>() {}
        assert_type::<PaymentIntent>();
        assert_type::<PaymentState>();
    }

    #[test]
    fn prelude_provides_ids() {
        let _id = PaymentId::new();
    }

    #[test]
    fn prelude_provides_timeout_config() {
        let _cfg = TimeoutConfig::default();
    }

    #[test]
    fn prelude_provides_transition_types() {
        fn assert_type<T>() {}
        assert_type::<TransitionResult<Created>>();
        assert_type::<TransitionError>();
    }

    #[test]
    fn root_reexports_all_public_types() {
        // Verify types accessible from payrail_core:: root
        fn assert_type<T>() {}
        assert_type::<crate::Payment<crate::Created>>();
        assert_type::<crate::Money>();
        assert_type::<crate::Currency>();
        assert_type::<crate::PaymentIntent>();
        assert_type::<crate::PaymentState>();
        assert_type::<crate::PaymentId>();
        assert_type::<crate::EventId>();
        assert_type::<crate::EventType>();
        assert_type::<crate::CanonicalEvent>();
        assert_type::<crate::RawWebhook>();
        assert_type::<crate::EventEnvelope>();
        assert_type::<crate::ErrorCode>();
        assert_type::<crate::PayRailError>();
        assert_type::<crate::PaymentCommand>();
        assert_type::<crate::TimeoutConfig>();
        assert_type::<crate::TransitionError>();
        assert_type::<crate::TransitionResult<crate::Created>>();
    }
}
