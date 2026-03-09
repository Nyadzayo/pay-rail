import { describe, it, expect } from "vitest";
import { getUniversalRules } from "../universal-rules.js";
import { estimateTokenCount } from "../token-budget.js";

describe("getUniversalRules", () => {
  it("returns a non-empty string", () => {
    const rules = getUniversalRules();
    expect(rules.length).toBeGreaterThan(0);
  });

  it("fits within ~2K token budget", () => {
    const rules = getUniversalRules();
    const tokens = estimateTokenCount(rules);
    expect(tokens).toBeLessThanOrEqual(2000);
  });

  it("includes canonical state machine states", () => {
    const rules = getUniversalRules();
    expect(rules).toContain("pending");
    expect(rules).toContain("authorized");
    expect(rules).toContain("captured");
    expect(rules).toContain("failed");
    expect(rules).toContain("refunded");
  });

  it("includes idempotency rules", () => {
    const rules = getUniversalRules();
    expect(rules).toContain("idempotency");
  });

  it("includes naming conventions", () => {
    const rules = getUniversalRules();
    expect(rules).toContain("snake_case");
    expect(rules).toContain("camelCase");
  });

  it("includes PCI boundary rules", () => {
    const rules = getUniversalRules();
    expect(rules).toContain("PCI");
    expect(rules).toContain("PAN");
  });

  it("includes error format convention", () => {
    const rules = getUniversalRules();
    expect(rules).toContain("domain.entity.action");
  });
});
