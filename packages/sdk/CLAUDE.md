# sdk/ — @payrail/sdk

**Purpose:** TypeScript SDK providing canonical payment types, Zod validation schemas, and error types that mirror the Rust core.

## Depends on

- External: `zod`
- Conceptual: mirrors types from payrail-core (not a build dependency)

## Key files

- `src/index.ts`: Public API surface
- `src/types.ts`: Canonical types for TypeScript consumers
- `src/validation.ts`: Zod schemas + snake->camel boundary transform
- `src/errors.ts`: PayRailError type + domain codes

## Key models/types/interfaces

- Canonical payment types (TypeScript mirrors of Rust types)
- Zod validation schemas for runtime type checking
- PayRailError with domain codes

## Key endpoints/functions/views

- Type exports for consumer usage
- Validation functions for boundary transforms

## Integration points

- Consumer TypeScript projects
- @payrail/mcp-server (uses SDK types)

## Test

```bash
cd payrail && pnpm --filter @payrail/sdk test
```
