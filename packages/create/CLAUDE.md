# create/ — @payrail/create

**Purpose:** Project scaffolding tool for quickstart setup via `npx @payrail/create`.

## Depends on

None — minimal scaffolding package.

## Key files

- `src/index.ts`: Scaffolding entry point
- `src/templates/`: Project templates (future)

## Key models/types/interfaces

None yet — scaffolding templates to be added.

## Key endpoints/functions/views

- CLI entry point for `npx @payrail/create`

## Integration points

- npm registry (distribution)
- End-user projects (generates starter code)

## Test

```bash
cd payrail && pnpm --filter @payrail/create test
```
