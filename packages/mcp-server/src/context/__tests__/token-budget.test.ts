import { describe, it, expect } from "vitest";
import {
  estimateTokenCount,
  enforceTokenBudget,
  DEFAULT_TOTAL_BUDGET,
  LAYER_BUDGETS,
} from "../token-budget.js";
import { estimateTokens } from "../../knowledge/compiler.js";

describe("estimateTokenCount", () => {
  it("estimates tokens as chars/4 rounded up", () => {
    expect(estimateTokenCount("abcd")).toBe(1);
    expect(estimateTokenCount("abcde")).toBe(2);
    expect(estimateTokenCount("")).toBe(0);
    expect(estimateTokenCount("a".repeat(100))).toBe(25);
  });

  it("is the same function as compiler.estimateTokens (no duplication)", () => {
    expect(estimateTokenCount).toBe(estimateTokens);
  });
});

describe("enforceTokenBudget", () => {
  it("returns text unchanged when within budget", () => {
    const text = "short text";
    const result = enforceTokenBudget(text, 100);
    expect(result).toBe(text);
  });

  it("truncates text when over budget", () => {
    const text = "a".repeat(1000); // 250 tokens
    const result = enforceTokenBudget(text, 50); // 50 tokens = 200 chars
    expect(result.length).toBeLessThanOrEqual(200);
    expect(estimateTokenCount(result)).toBeLessThanOrEqual(50);
  });

  it("returns empty string for zero budget", () => {
    expect(enforceTokenBudget("hello", 0)).toBe("");
  });
});

describe("budget constants", () => {
  it("total budget is 14K", () => {
    expect(DEFAULT_TOTAL_BUDGET).toBe(14000);
  });

  it("layer budgets sum to less than total", () => {
    const sum = LAYER_BUDGETS.universal + LAYER_BUDGETS.provider + LAYER_BUDGETS.codebase;
    expect(sum).toBeLessThanOrEqual(DEFAULT_TOTAL_BUDGET);
  });

  it("universal layer is ~2K", () => {
    expect(LAYER_BUDGETS.universal).toBe(2000);
  });

  it("provider layer is 4-8K range", () => {
    expect(LAYER_BUDGETS.provider).toBeGreaterThanOrEqual(4000);
    expect(LAYER_BUDGETS.provider).toBeLessThanOrEqual(8000);
  });
});
