/// Deterministic idempotency key generation.
pub mod key;
/// IdempotencyStore trait and SQLite backend.
pub mod store;

pub use key::{IdempotencyKey, KeyError};
pub use store::{
    IdempotencyError, IdempotencyOutcome, IdempotencyRecord, IdempotencyStore,
    SqliteIdempotencyStore,
};
