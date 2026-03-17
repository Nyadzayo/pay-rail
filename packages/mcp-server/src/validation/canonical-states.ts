/**
 * Canonical payment state machine reference.
 * 9 states with defined valid transitions.
 * Source: payrail-core state.rs + architecture.md
 */

export const CANONICAL_STATES = [
  "Created", "Authorized", "Captured", "Refunded",
  "Voided", "Failed", "Expired", "Pending3ds", "Settled",
] as const;

export type CanonicalState = (typeof CANONICAL_STATES)[number];

export const TERMINAL_STATES: ReadonlySet<CanonicalState> = new Set([
  "Refunded", "Voided", "Failed", "Expired", "Settled",
]);

/** Valid transitions: [from, to] pairs */
export const VALID_TRANSITIONS: ReadonlyArray<[CanonicalState, CanonicalState]> = [
  ["Created", "Authorized"],
  ["Created", "Failed"],
  ["Created", "Expired"],
  ["Created", "Pending3ds"],
  ["Pending3ds", "Authorized"],
  ["Pending3ds", "Failed"],
  ["Pending3ds", "Expired"],
  ["Authorized", "Captured"],
  ["Authorized", "Voided"],
  ["Authorized", "Failed"],
  ["Authorized", "Expired"],
  ["Captured", "Refunded"],
  ["Captured", "Settled"],
  ["Captured", "Failed"],
  ["Refunded", "Settled"],
];

/** States that should handle self-transitions (duplicate events) */
export const SELF_TRANSITION_STATES: ReadonlyArray<CanonicalState> = [
  "Authorized", "Captured", "Refunded", "Voided", "Failed",
];

/** Get valid target states for a given source state */
export function validTargetsFrom(state: CanonicalState): CanonicalState[] {
  return VALID_TRANSITIONS
    .filter(([from]) => from === state)
    .map(([, to]) => to);
}

/** Check if a specific transition is valid */
export function isValidTransition(from: CanonicalState, to: CanonicalState): boolean {
  return VALID_TRANSITIONS.some(([f, t]) => f === from && t === to);
}
