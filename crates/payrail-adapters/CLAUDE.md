# payrail-adapters/ — Provider Adapters

**Purpose:** Provider-specific payment adapters implementing the core PaymentAdapter trait for Peach Payments and Startbutton.

## Depends on

- [payrail-core/CLAUDE.md](../payrail-core/CLAUDE.md) — PaymentAdapter trait, canonical types, event types, error types

## Key files

- `src/lib.rs`: Re-exports adapter trait + provider modules
- `src/traits.rs`: PaymentAdapter trait, SignatureConfig, PaymentCommand enum
- `src/peach/mod.rs`: Peach Payments adapter module
- `src/peach/adapter.rs`: PeachPaymentsAdapter — execute() + translate_webhook()
- `src/peach/types.rs`: Peach-specific API types (request/response shapes)
- `src/peach/mappings.rs`: Peach status codes -> canonical states
- `src/startbutton/mod.rs`: Startbutton adapter module
- `src/startbutton/adapter.rs`: StartbuttonAdapter
- `src/startbutton/types.rs`: Startbutton-specific types
- `src/startbutton/mappings.rs`: Startbutton -> canonical mappings

## Key models/types/interfaces

- `PaymentAdapter`: Trait — execute(), translate_webhook(), signature_config()
- `PeachPaymentsAdapter`: Peach Payments implementation
- `StartbuttonAdapter`: Startbutton implementation

## Key endpoints/functions/views

- `execute()`: Send payment command to provider API
- `translate_webhook()`: Normalize provider webhook into CanonicalEvent

## Integration points

- Peach Payments sandbox/production API
- Startbutton sandbox/production API
- payrail-core (implements core traits)

## Test

```bash
cd payrail && PATH="$HOME/.cargo/bin:$PATH" cargo test -p payrail-adapters
```
