# payrail-cli/ — CLI Tool

**Purpose:** Command-line interface for PayRail operations including knowledge pack management, adapter generation, conformance testing, project initialization, and health checks.

## Depends on

- [payrail-core/CLAUDE.md](../payrail-core/CLAUDE.md) — core types and engine
- [payrail-adapters/CLAUDE.md](../payrail-adapters/CLAUDE.md) — provider adapters
- [payrail-output/CLAUDE.md](../payrail-output/CLAUDE.md) — shared output formatting, colors, symbols

## Key files

- `src/main.rs`: CLI entry point, clap argument parsing, subcommand dispatch
- `src/knowledge.rs`: Knowledge pack init, ingest, validate, compile, publish
- `src/generate.rs`: Adapter code generation from knowledge packs
- `src/conformance.rs`: Conformance test runner (delegates to TypeScript pipeline)
- `src/init.rs`: Project scaffold initialization
- `src/doctor.rs`: Health checks (env vars, knowledge packs, adapters, connectivity)
- `src/status.rs`: Project status and provider summary display
- `src/reconciliation.rs`: Reconciliation report display (human-readable and JSON)

## Key types

- **`Cli`** — Top-level clap struct with global flags: `--json`, `--verbose`, `--quiet`, `--no-color`
- **`Commands`** — Subcommand enum: Knowledge, Generate, Conformance, Init, Doctor, Status, Reconciliation, Completions
- **`CheckResult`** — Doctor check result with category, pass/fail, message, optional help

## Subcommands

| Command | Description |
|---------|-------------|
| `knowledge init/ingest/validate/compile/publish` | Knowledge pack lifecycle |
| `generate <provider>` | Generate adapter from knowledge pack |
| `conformance run <provider> [--sandbox]` | Run conformance tests |
| `init [--provider] [--lang] [--framework]` | Scaffold a new project |
| `doctor` | Check project health |
| `status [--period]` | Show project status |
| `reconciliation [--provider] [--period]` | Show reconciliation report |
| `completions <shell>` | Generate shell completions |

## Test

```bash
cd payrail && PATH="$HOME/.cargo/bin:$PATH" cargo test -p payrail-cli
```
