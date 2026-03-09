import { loadCompiledPack } from "../knowledge/loader.js";
import type { CompiledFact } from "../knowledge/compiler.js";
import { getUniversalRules } from "./universal-rules.js";
import {
  estimateTokenCount,
  enforceTokenBudget,
  DEFAULT_TOTAL_BUDGET,
  LAYER_BUDGETS,
} from "./token-budget.js";
import { scanCodebase } from "../fingerprint/scanner.js";
import { formatFingerprintAsMarkdown } from "../fingerprint/conventions.js";

export interface ContextStack {
  universalRules: string;
  universalTokens: number;
  providerContext: string;
  providerTokens: number;
  codebaseContext: string;
  codebaseTokens: number;
  totalTokens: number;
  loaded: boolean;
  trimmedCount: number;
}

function formatFactsAsContext(facts: CompiledFact[]): string {
  if (facts.length === 0) return "";

  const byCategory = new Map<string, CompiledFact[]>();
  for (const fact of facts) {
    const existing = byCategory.get(fact.category) ?? [];
    existing.push(fact);
    byCategory.set(fact.category, existing);
  }

  const sections: string[] = [];
  for (const [category, catFacts] of byCategory) {
    sections.push(`### ${category}`);
    for (const f of catFacts) {
      const value = f.value as Record<string, unknown>;
      const desc = (value.description ?? value.name ?? value.event_name ?? value.url ?? "unknown") as string;
      const line = `- ${desc} (confidence: ${f.confidence_score}, source: ${f.source})`;
      sections.push(line);
      if (f.verify_marker) {
        sections.push(`  ${f.verify_marker}`);
      }
    }
    sections.push("");
  }
  return sections.join("\n");
}

export function assembleContext(
  provider: string,
  knowledgePacksPath: string,
  totalBudget: number = DEFAULT_TOTAL_BUDGET,
  projectPath?: string,
): ContextStack {
  // Layer 1: Universal rules
  const universalRules = enforceTokenBudget(getUniversalRules(), LAYER_BUDGETS.universal);
  const universalTokens = estimateTokenCount(universalRules);

  // Layer 3: Codebase context
  let codebaseContext = "";
  if (projectPath) {
    const fingerprint = scanCodebase(projectPath);
    codebaseContext = enforceTokenBudget(
      formatFingerprintAsMarkdown(fingerprint),
      LAYER_BUDGETS.codebase,
    );
  }
  const codebaseTokens = estimateTokenCount(codebaseContext);

  // Layer 2: Provider knowledge pack
  const remainingBudget = totalBudget - universalTokens - codebaseTokens;
  const providerBudget = Math.min(remainingBudget, LAYER_BUDGETS.provider);

  const loaded = loadCompiledPack(provider, knowledgePacksPath);
  if (!loaded) {
    return {
      universalRules,
      universalTokens,
      providerContext: "",
      providerTokens: 0,
      codebaseContext,
      codebaseTokens,
      totalTokens: universalTokens + codebaseTokens,
      loaded: false,
      trimmedCount: 0,
    };
  }

  // Format facts and trim if over budget
  // Sort once by confidence ascending (lowest first for trimming)
  let facts = [...loaded.pack.facts].sort((a, b) => a.confidence_score - b.confidence_score);
  let providerText = formatFactsAsContext(facts);
  let providerTokens = estimateTokenCount(providerText);
  let trimmedCount = 0;

  // Trim lowest-confidence facts first to fit budget
  while (providerTokens > providerBudget && facts.length > 0) {
    facts.shift();
    trimmedCount++;
    providerText = formatFactsAsContext(facts);
    providerTokens = estimateTokenCount(providerText);
  }

  // Final total budget enforcement — guard against rounding drift
  let total = universalTokens + providerTokens + codebaseTokens;
  while (total > totalBudget && facts.length > 0) {
    facts.shift();
    trimmedCount++;
    providerText = formatFactsAsContext(facts);
    providerTokens = estimateTokenCount(providerText);
    total = universalTokens + providerTokens + codebaseTokens;
  }

  return {
    universalRules,
    universalTokens,
    providerContext: providerText,
    providerTokens,
    codebaseContext,
    codebaseTokens,
    totalTokens: total,
    loaded: true,
    trimmedCount,
  };
}
