import { estimateTokens } from "../knowledge/compiler.js";

export const DEFAULT_TOTAL_BUDGET = 14000;

export const LAYER_BUDGETS = {
  universal: 2000,
  provider: 6000,
  codebase: 6000,
} as const;

export const estimateTokenCount = estimateTokens;

export function enforceTokenBudget(text: string, maxTokens: number): string {
  if (maxTokens <= 0) return "";
  const maxChars = maxTokens * 4;
  if (text.length <= maxChars) return text;
  return text.slice(0, maxChars);
}
