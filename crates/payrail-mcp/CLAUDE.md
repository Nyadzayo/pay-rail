# payrail-mcp/ — Rust MCP Bridge

**Purpose:** Rust-side logic for MCP server integration, bridging core engine functionality to the TypeScript MCP server.

## Depends on

- [payrail-core/CLAUDE.md](../payrail-core/CLAUDE.md) — core types, state machine, event store
- [payrail-adapters/CLAUDE.md](../payrail-adapters/CLAUDE.md) — provider adapters for generation/validation

## Key files

- `src/lib.rs`: MCP bridge logic (stubbed)

## Key models/types/interfaces

None yet — implementation in Story 5.1.

## Key endpoints/functions/views

None yet — will expose core functionality as MCP-compatible operations.

## Integration points

- payrail-core (payment engine)
- payrail-adapters (provider operations)
- @payrail/mcp-server (TypeScript MCP server consumes this)

## Test

```bash
cd payrail && PATH="$HOME/.cargo/bin:$PATH" cargo test -p payrail-mcp
```
