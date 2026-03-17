# payrail-output/ — Shared CLI Output Formatting

**Purpose:** Shared output formatting, color management, symbol rendering, and terminal adaptation for PayRail CLI and MCP tooling.

## Depends on

Nothing (leaf crate).

## Used by

- [payrail-cli/CLAUDE.md](../payrail-cli/CLAUDE.md) — CLI tool
- [payrail-mcp/CLAUDE.md](../payrail-mcp/CLAUDE.md) — MCP bridge

## Key files

- `src/config.rs`: `OutputConfig`, `OutputMode`, `Verbosity`, `ColorMode`, terminal width detection
- `src/writer.rs`: `OutputWriter` trait, `StdWriter` implementation with JSON/text dual output
- `src/format.rs`: Line formatting utilities (`summary_line`, `section_header`, `detail_line`, `help_line`, `error_message`) with terminal width truncation
- `src/colors.rs`: Color application functions using `owo-colors`, gated by `OutputConfig`
- `src/symbols.rs`: Unicode/ASCII symbol selection for pass/fail/verify indicators

## Key types

- **`OutputConfig`** — Central config: mode (text/json), color (auto/always/never), verbosity (quiet/normal/verbose), terminal_width
- **`OutputWriter`** (trait) — Structured output interface: `write_success`, `write_warning`, `write_error`, `write_result<T: Serialize>`
- **`StdWriter`** — Default `OutputWriter` that prints to stdout/stderr; also exposes `format_*` methods for testable string output

## Test

```bash
cd payrail && PATH="$HOME/.cargo/bin:$PATH" cargo test -p payrail-output
```
