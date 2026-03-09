# payrail-core/ — Core Engine

**Purpose:** Typestate payment engine with compile-time safety, event store, idempotency, and webhook processing.

## Depends on

None — this is the foundation crate. Zero external deps beyond serde, thiserror, chrono, tokio.

## Key files

- `src/lib.rs`: Public API re-exports all modules
- `src/payment/state.rs`: Typestate definitions (Created, Pending3DS, Authorized, Captured, Refunded, Voided, Failed, TimedOut)
- `src/payment/machine.rs`: State machine — transition validation, timeout rules, self-transitions
- `src/payment/types.rs`: Money, Currency, PaymentIntent, PaymentCommand, PaymentEvent
- `src/event/store.rs`: EventStore trait + SqliteEventStore impl
- `src/event/types.rs`: CanonicalEvent, RawWebhook, EventEnvelope
- `src/idempotency/store.rs`: IdempotencyStore trait + SqliteIdempotencyStore impl
- `src/idempotency/key.rs`: Deterministic key generation per provider pattern
- `src/webhook/receiver.rs`: Two-phase handler (hot path: verify -> dedup -> transition -> ACK)
- `src/webhook/signature.rs`: Framework-level HMAC verification, timing-safe comparison
- `src/error.rs`: PayRailError enum with domain codes (PAY_*, MCP_*, etc.)
- `src/id.rs`: ULID generation with prefixes (pay_, evt_)
- `src/config.rs`: PayRailConfig — loads payrail.config.yaml

## Key models/types/interfaces

- `Payment<S>`: Typestate payment — `S` is one of 8 state types
- `Money`: i64 value + currency code (integer cents, never float)
- `CanonicalEvent`: Normalized event from any provider
- `EventStore`: Trait for append-only event persistence
- `IdempotencyStore`: Trait for dedup key management
- `PayRailError`: Unified error type with domain codes

## Key endpoints/functions/views

- `Payment::create()`: Entry point — returns `Payment<Created>`
- `.authorize()`, `.capture()`, `.refund()`, `.void()`: State transitions (compile-time enforced)
- `WebhookReceiver::handle()`: Two-phase webhook processing

## Integration points

- SQLite (event store, idempotency store)
- payrail-adapters (consumers of core traits)

## Test

```bash
cd payrail && PATH="$HOME/.cargo/bin:$PATH" cargo test -p payrail-core
```
