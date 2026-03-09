# payrail-cli/ — CLI Tool

**Purpose:** Command-line interface for PayRail operations (Phase 2, currently stubbed).

## Depends on

- [payrail-core/CLAUDE.md](../payrail-core/CLAUDE.md) — core types and engine
- [payrail-adapters/CLAUDE.md](../payrail-adapters/CLAUDE.md) — provider adapters

## Key files

- `src/main.rs`: CLI entry point (stubbed)

## Key models/types/interfaces

None yet — Phase 2.

## Key endpoints/functions/views

None yet — Phase 2.

## Integration points

- payrail-core (payment engine)
- payrail-adapters (provider operations)

## Test

```bash
cd payrail && PATH="$HOME/.cargo/bin:$PATH" cargo test -p payrail-cli
```
