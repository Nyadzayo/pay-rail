import { describe, it, expect } from "vitest";
import {
  CANONICAL_STATES,
  VALID_TRANSITIONS,
  TERMINAL_STATES,
  SELF_TRANSITION_STATES,
  validTargetsFrom,
  isValidTransition,
} from "../canonical-states.js";

describe("canonical-states", () => {
  it("defines exactly 9 canonical states", () => {
    expect(CANONICAL_STATES).toHaveLength(9);
  });

  it("includes all required states", () => {
    expect(CANONICAL_STATES).toContain("Created");
    expect(CANONICAL_STATES).toContain("Authorized");
    expect(CANONICAL_STATES).toContain("Captured");
    expect(CANONICAL_STATES).toContain("Refunded");
    expect(CANONICAL_STATES).toContain("Voided");
    expect(CANONICAL_STATES).toContain("Failed");
    expect(CANONICAL_STATES).toContain("Expired");
    expect(CANONICAL_STATES).toContain("Pending3ds");
    expect(CANONICAL_STATES).toContain("Settled");
  });

  it("defines 4 terminal states", () => {
    expect(TERMINAL_STATES.size).toBe(4);
    expect(TERMINAL_STATES.has("Voided")).toBe(true);
    expect(TERMINAL_STATES.has("Failed")).toBe(true);
    expect(TERMINAL_STATES.has("Expired")).toBe(true);
    expect(TERMINAL_STATES.has("Settled")).toBe(true);
  });

  it("terminal states have no valid outgoing transitions", () => {
    for (const state of TERMINAL_STATES) {
      expect(validTargetsFrom(state)).toHaveLength(0);
    }
  });

  it("Created has 4 valid targets", () => {
    const targets = validTargetsFrom("Created");
    expect(targets).toContain("Authorized");
    expect(targets).toContain("Failed");
    expect(targets).toContain("Expired");
    expect(targets).toContain("Pending3ds");
  });

  it("isValidTransition checks correctly", () => {
    expect(isValidTransition("Created", "Authorized")).toBe(true);
    expect(isValidTransition("Created", "Refunded")).toBe(false);
    expect(isValidTransition("Captured", "Refunded")).toBe(true);
    expect(isValidTransition("Refunded", "Created")).toBe(false);
  });

  it("defines 5 self-transition states", () => {
    expect(SELF_TRANSITION_STATES).toHaveLength(5);
  });

  it("VALID_TRANSITIONS has 15 entries", () => {
    expect(VALID_TRANSITIONS.length).toBe(15);
  });
});
