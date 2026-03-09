const UNIVERSAL_RULES = `## Universal Payment Rules

### Canonical State Machine (8 states)
States: pending, authorized, captured, partially_captured, failed, cancelled, refunded, partially_refunded
Valid transitions:
- pending -> authorized | failed | cancelled
- authorized -> captured | partially_captured | cancelled | failed
- captured -> refunded | partially_refunded
- partially_captured -> captured | refunded | partially_refunded | cancelled
- failed -> (terminal)
- cancelled -> (terminal)
- refunded -> (terminal)
- partially_refunded -> refunded

### Idempotency Rules
- Key format: \`{provider}:{operation}:{external_id}\`
- TTL: 24 hours minimum
- Fail-closed: defer without dedup protection, never process unsafely
- Every request must carry an idempotency key
- Duplicate requests return cached response, never re-process

### Naming Conventions
- Wire format (JSON/API): snake_case
- TypeScript code: camelCase for variables/functions
- Types/interfaces: PascalCase
- Constants: SCREAMING_SNAKE_CASE
- Canonical events: domain.entity.action (e.g., payment.charge.captured)

### Error Format
- Pattern: [WHAT] [WHY] [WHAT TO DO]
- Event naming: domain.entity.action
- Include doc links in error messages
- Structured error codes, not free-text

### PCI Boundary Rules
- Never store raw PAN (Primary Account Number)
- Never store CVV/CVC
- Never log card data
- Use tokenized references only
- PCI DSS enforced at type level — raw card types are unconstructable

### Financial Correctness
- Integer cents for all money values (never floating point)
- UTC timestamps always
- Every cent must be accounted for
- Currency codes: ISO 4217 (3-letter uppercase)

### Webhook Processing
- Verify signatures before processing
- Idempotent event handling
- Canonical event normalization from provider-specific events
- Append-only event store for audit trail`;

export function getUniversalRules(): string {
  return UNIVERSAL_RULES;
}
