# mcp-server/ — @payrail/mcp-server

**Purpose:** TypeScript MCP server that registers AI tools for payment adapter generation, knowledge queries, and conformance validation.

## Depends on

- [payrail-mcp/CLAUDE.md](../../crates/payrail-mcp/CLAUDE.md) — Rust-side MCP bridge
- External: `@modelcontextprotocol/sdk@1.27.1`, `zod`

## Key files

- `src/index.ts`: MCP server entry — registers tools, starts stdio transport
- `src/tools/query-provider-pack.ts`: Query knowledge pack tool
- `src/tools/generate-adapter.ts`: Adapter generation tool
- `src/tools/validate-state-machine.ts`: State machine validation tool
- `src/tools/run-conformance.ts`: Conformance test runner tool
- `src/context/assembler.ts`: Three-layer context stack assembly (<14K tokens)
- `src/fingerprint/scanner.ts`: Codebase fingerprinting (language, framework, ORM)
- `src/knowledge/loader.ts`: Load compiled knowledge packs (JSON)
- `src/knowledge/schema.ts`: Zod schemas for knowledge pack validation

## Key models/types/interfaces

- MCP tool definitions (query_provider_pack, generate_adapter, validate_state_machine, run_conformance)
- `ContextStack`: Three-layer context assembly
- `CodebaseFingerprint`: Detected project conventions

## Key endpoints/functions/views

- Tool handlers registered via MCP SDK
- Context assembler with token budget enforcement

## Integration points

- MCP protocol (stdio transport to AI tools: Claude, Cursor, VS Code)
- Knowledge packs (JSON compiled from YAML)
- payrail-mcp Rust bridge

## Test

```bash
cd payrail && pnpm --filter @payrail/mcp-server test
```
