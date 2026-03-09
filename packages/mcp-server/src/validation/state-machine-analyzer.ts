import { readFileSync } from "node:fs";
import {
  CANONICAL_STATES,
  VALID_TRANSITIONS,
  TERMINAL_STATES,
  SELF_TRANSITION_STATES,
  validTargetsFrom,
  type CanonicalState,
} from "./canonical-states.js";

export type IssueSeverity = "error" | "warning" | "info";

export interface ValidationIssue {
  severity: IssueSeverity;
  description: string;
  location: string;
  fix: string;
  reference: string;
}

export interface ValidationResult {
  valid: boolean;
  mappedStates: CanonicalState[];
  mappedTransitions: Array<{ from: CanonicalState; to: CanonicalState }>;
  hasSelfTransitionHandling: boolean;
  issues: ValidationIssue[];
}

/**
 * Analyze code for state machine correctness.
 * Accepts either inline code or a file path.
 */
export function analyzeStateMachine(input: { code?: string; filePath?: string }): ValidationResult {
  let code: string;
  let sourceLabel: string;

  if (input.code) {
    code = input.code;
    sourceLabel = "inline code";
  } else if (input.filePath) {
    code = readFileSync(input.filePath, "utf-8");
    sourceLabel = input.filePath;
  } else {
    return {
      valid: false,
      mappedStates: [],
      mappedTransitions: [],
      hasSelfTransitionHandling: false,
      issues: [{
        severity: "error",
        description: "No code provided for analysis",
        location: "input",
        fix: "Provide either a code snippet or file path",
        reference: "validate_state_machine tool usage",
      }],
    };
  }

  let mappedStates = extractMappedStates(code);
  const mappedTransitions = extractTransitions(code, mappedStates);
  // Merge states discovered via transitions into mappedStates
  for (const t of mappedTransitions) {
    if (!mappedStates.includes(t.from)) mappedStates.push(t.from);
    if (!mappedStates.includes(t.to)) mappedStates.push(t.to);
  }
  const hasSelfTransitionHandling = detectSelfTransitionHandling(code);
  const issues: ValidationIssue[] = [];

  // Check for missing state mappings
  const mappedSet = new Set(mappedStates);
  const missingStates = CANONICAL_STATES.filter((s) => !mappedSet.has(s));
  for (const state of missingStates) {
    const isTerminal = TERMINAL_STATES.has(state);
    issues.push({
      severity: isTerminal ? "warning" : "error",
      description: `No mapping found for canonical state "${state}"`,
      location: sourceLabel,
      fix: `Add a status code or event type mapping that resolves to "${state}"`,
      reference: `Canonical state machine: ${state} is ${isTerminal ? "terminal" : "non-terminal"}`,
    });
  }

  // Check for missing transitions from non-terminal states
  for (const state of CANONICAL_STATES) {
    if (TERMINAL_STATES.has(state)) continue;
    const expectedTargets = validTargetsFrom(state);
    for (const target of expectedTargets) {
      const hasTransition = mappedTransitions.some(
        (t) => t.from === state && t.to === target,
      );
      if (!hasTransition && mappedSet.has(state) && mappedSet.has(target)) {
        // Only warn if both states are mapped but the transition path isn't covered
        issues.push({
          severity: target === "Failed" || target === "Expired" ? "warning" : "error",
          description: `Missing transition: ${state} -> ${target}`,
          location: sourceLabel,
          fix: `Add handling for the ${state} -> ${target} transition in your status mapping or event handler`,
          reference: `Canonical transitions from ${state}: ${expectedTargets.join(", ")}`,
        });
      }
    }
  }

  // Check for invalid transitions
  for (const t of mappedTransitions) {
    const valid = VALID_TRANSITIONS.some(([f, to]) => f === t.from && to === t.to);
    if (!valid && t.from !== t.to) {
      issues.push({
        severity: "error",
        description: `Invalid transition: ${t.from} -> ${t.to}`,
        location: sourceLabel,
        fix: `Remove or correct this transition. ${t.from} cannot transition to ${t.to} in the canonical state machine`,
        reference: `Valid targets from ${t.from}: ${validTargetsFrom(t.from).join(", ") || "(terminal state)"}`,
      });
    }
  }

  // Check for unreachable states
  const reachable = new Set<CanonicalState>(["Created"]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const t of mappedTransitions) {
      if (reachable.has(t.from) && !reachable.has(t.to)) {
        reachable.add(t.to);
        changed = true;
      }
    }
  }
  const unreachable = CANONICAL_STATES.filter(
    (s) => mappedSet.has(s) && !reachable.has(s) && s !== "Created",
  );
  for (const state of unreachable) {
    issues.push({
      severity: "warning",
      description: `State "${state}" is mapped but unreachable from Created`,
      location: sourceLabel,
      fix: `Ensure there is a transition path from Created to ${state}`,
      reference: "All states must be reachable from the Created initial state",
    });
  }

  // Check self-transition handling
  if (!hasSelfTransitionHandling) {
    issues.push({
      severity: "info",
      description: "No self-transition handling detected",
      location: sourceLabel,
      fix: `Add handling for duplicate events (same state -> same state). States needing self-transition: ${SELF_TRANSITION_STATES.join(", ")}`,
      reference: "Self-transitions should be logged and ignored to handle duplicate webhooks",
    });
  }

  // Sort: errors first, then warnings, then info
  const severityOrder: Record<IssueSeverity, number> = { error: 0, warning: 1, info: 2 };
  issues.sort((a, b) => severityOrder[a.severity] - severityOrder[b.severity]);

  return {
    valid: issues.filter((i) => i.severity === "error").length === 0,
    mappedStates: [...mappedSet] as CanonicalState[],
    mappedTransitions,
    hasSelfTransitionHandling,
    issues,
  };
}

/**
 * Extract states referenced in code via regex pattern matching.
 * Handles TypeScript and Rust patterns.
 */
function extractMappedStates(code: string): CanonicalState[] {
  const found = new Set<CanonicalState>();

  for (const state of CANONICAL_STATES) {
    // Match state in string literals: "Created", "Authorized", etc.
    const stringPattern = new RegExp(`["'\`]${state}["'\`]`, "g");
    if (stringPattern.test(code)) {
      found.add(state);
    }

    // Rust enum variants: PaymentState::Created
    const rustPattern = new RegExp(`PaymentState::${state}\\b`, "g");
    if (rustPattern.test(code)) {
      found.add(state);
    }

    // Rust uses TimedOut for what TS calls Expired
    if (state === "Expired" && /PaymentState::TimedOut\b/.test(code)) {
      found.add("Expired");
    }

    // TypeScript type references or assignments
    const assignPattern = new RegExp(`state\\s*[:=]\\s*["'\`]${state}["'\`]`, "gi");
    if (assignPattern.test(code)) {
      found.add(state);
    }
  }

  return [...found];
}

/**
 * Extract state transitions from code patterns.
 */
function extractTransitions(code: string, mappedStates: CanonicalState[]): Array<{ from: CanonicalState; to: CanonicalState }> {
  const transitions: Array<{ from: CanonicalState; to: CanonicalState }> = [];
  const seen = new Set<string>();

  // Pattern: explicit transition comments or arrow notation "Created -> Authorized"
  const arrowPattern = /(\w+)\s*->\s*(\w+)/g;
  let match;
  while ((match = arrowPattern.exec(code)) !== null) {
    const from = matchCanonicalState(match[1]);
    const to = matchCanonicalState(match[2]);
    if (from && to) {
      const key = `${from}->${to}`;
      if (!seen.has(key)) {
        seen.add(key);
        transitions.push({ from, to });
      }
    }
  }

  // Pattern: switch/case or if/else mapping event types to states
  // e.g., case "charge.succeeded": return "Captured"
  // Infer transitions from event semantics
  const eventToState = extractEventStateMap(code);
  for (const [eventType, state] of eventToState) {
    const inferredFrom = inferSourceState(eventType);
    if (inferredFrom && state) {
      const key = `${inferredFrom}->${state}`;
      if (!seen.has(key)) {
        seen.add(key);
        transitions.push({ from: inferredFrom, to: state });
      }
    }
  }

  // Pattern: status code map objects { "000.000": "Captured", ... }
  // These define state mappings but not transitions directly
  // Infer basic transitions from mapped non-terminal states
  for (const state of mappedStates) {
    if (TERMINAL_STATES.has(state)) continue;
    const targets = validTargetsFrom(state);
    for (const target of targets) {
      if (mappedStates.includes(target)) {
        const key = `${state}->${target}`;
        if (!seen.has(key)) {
          seen.add(key);
          transitions.push({ from: state, to: target });
        }
      }
    }
  }

  return transitions;
}

/**
 * Extract event type -> canonical state mappings from code.
 */
function extractEventStateMap(code: string): Array<[string, CanonicalState | null]> {
  const mappings: Array<[string, CanonicalState | null]> = [];

  // Pattern: case "event.type": ... "State" (limited to ~200 chars to avoid crossing case boundaries)
  const casePattern = /case\s+["'`]([^"'`]+)["'`]\s*:[^}]{0,200}?["'`](Created|Authorized|Captured|Refunded|Voided|Failed|Expired|Pending3ds)["'`]/g;
  let match;
  while ((match = casePattern.exec(code)) !== null) {
    const state = matchCanonicalState(match[2]);
    if (state) mappings.push([match[1], state]);
  }

  // Pattern: "eventType" => "State" or { event: "State" }
  const mapPattern = /["'`]([^"'`]+)["'`]\s*(?:=>|:)\s*["'`](Created|Authorized|Captured|Refunded|Voided|Failed|Expired|Pending3ds)["'`]/g;
  while ((match = mapPattern.exec(code)) !== null) {
    const state = matchCanonicalState(match[2]);
    if (state) mappings.push([match[1], state]);
  }

  return mappings;
}

/**
 * Infer source state from event type name.
 */
function inferSourceState(eventType: string): CanonicalState | null {
  const lower = eventType.toLowerCase();

  // 3DS pending = transition FROM Created TO Pending3ds (source is Created)
  if (lower.includes("3ds") && lower.includes("pending")) return "Created";
  if (lower.includes("3ds") || lower.includes("3d_secure")) return "Pending3ds";
  if (lower.includes("authorize") || lower.includes("auth")) return "Created";
  if (lower.includes("capture") || lower.includes("settle")) return "Authorized";
  if (lower.includes("refund")) return "Captured";
  if (lower.includes("void") || lower.includes("cancel")) return "Authorized";
  if (lower.includes("charge") && lower.includes("succeed")) return "Authorized";
  if (lower.includes("charge") && lower.includes("fail")) return "Created";
  if (lower.includes("payment") && lower.includes("succeed")) return "Authorized";
  if (lower.includes("expire") || lower.includes("timeout")) return "Created";

  return null;
}

function matchCanonicalState(input: string): CanonicalState | null {
  // Case-insensitive match, handling Pending3DS/Pending3ds variations
  const normalized = input.replace(/3DS/i, "3ds");
  for (const state of CANONICAL_STATES) {
    if (state.toLowerCase() === normalized.toLowerCase()) return state;
  }
  // Also match TimedOut -> Expired (Rust uses TimedOut)
  if (normalized.toLowerCase() === "timedout") return "Expired";
  return null;
}

function detectSelfTransitionHandling(code: string): boolean {
  const patterns = [
    /self[_-]?transition/i,
    /duplicate[_\s]?event/i,
    /same[_\s]?state/i,
    /idempoten/i,
    /already[_\s]in[_\s]state/i,
    /state_before\s*===?\s*state_after/i,
    /from\s*===?\s*to\b/i,
  ];
  return patterns.some((p) => p.test(code));
}

/**
 * Format validation result as structured markdown within ~500 token budget.
 */
export function formatValidationOutput(result: ValidationResult): string {
  const sections: string[] = [];

  if (result.valid) {
    sections.push("**State Machine Validation: PASSED**\n");
  } else {
    const errorCount = result.issues.filter((i) => i.severity === "error").length;
    sections.push(`**State Machine Validation: FAILED** (${errorCount} error${errorCount !== 1 ? "s" : ""})\n`);
  }

  // Summary table
  const coveredCount = result.mappedStates.length;
  const errorCount = result.issues.filter((i) => i.severity === "error").length;
  const warningCount = result.issues.filter((i) => i.severity === "warning").length;
  const infoCount = result.issues.filter((i) => i.severity === "info").length;

  sections.push("| Check | Result |");
  sections.push("|-------|--------|");
  sections.push(`| States covered | ${coveredCount}/${CANONICAL_STATES.length} |`);
  sections.push(`| Valid transitions | ${result.mappedTransitions.length} mapped |`);
  sections.push(`| Self-transition handling | ${result.hasSelfTransitionHandling ? "Present" : "Missing"} |`);
  sections.push(`| Issues | ${errorCount} errors, ${warningCount} warnings, ${infoCount} info |`);
  sections.push("");

  if (result.issues.length === 0) {
    sections.push("No issues found. Adapter correctly implements the canonical state machine.");
    return sections.join("\n");
  }

  // Issue details (truncate to fit ~500 tokens)
  sections.push("**Issues:**\n");
  const maxIssues = 8;
  const displayed = result.issues.slice(0, maxIssues);

  for (const issue of displayed) {
    const icon = issue.severity === "error" ? "ERROR" : issue.severity === "warning" ? "WARN" : "INFO";
    sections.push(`- **[${icon}]** ${issue.description}`);
    sections.push(`  - Fix: ${issue.fix}`);
  }

  if (result.issues.length > maxIssues) {
    sections.push(`\n...and ${result.issues.length - maxIssues} more issue(s).`);
  }

  return sections.join("\n");
}
