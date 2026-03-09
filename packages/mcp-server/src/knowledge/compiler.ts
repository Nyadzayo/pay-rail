import type {
  KnowledgePack,
  FactCategory,
  FactSource,
} from "./schema.js";

// ── Types ──

export interface CompilationConfig {
  thresholds: {
    generate: number; // >= this: include directly (default 0.9)
    verify_min: number; // >= this and < generate: include with VERIFY marker (default 0.7)
    refuse_below: number; // < this: exclude entirely (default 0.7)
  };
  token_budget: number; // max tokens for compiled pack (default 4000-8000)
}

export interface CompiledFact {
  category: FactCategory;
  value: unknown;
  confidence_score: number;
  source: FactSource;
  verify_marker?: string;
}

export interface CompiledPack {
  version: string;
  metadata: KnowledgePack["metadata"];
  facts: CompiledFact[];
}

export interface CompilationMeta {
  version: string;
  token_count: number;
  coverage_pct: number;
  confidence_summary: {
    generate: number;
    verify: number;
    refuse_excluded: number;
  };
  compiled_at: string;
}

export interface CompileResult {
  compiledPack: CompiledPack;
  meta: CompilationMeta;
  warnings: string[];
  trimmed: CompiledFact[];
}

// ── Helpers ──

export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

const CHECK_GUIDANCE: Record<FactCategory, string> = {
  endpoints:
    "Verify this endpoints fact against provider documentation or sandbox",
  webhooks:
    "Verify this webhooks fact against provider documentation or sandbox",
  status_codes:
    "Verify this status_codes mapping against provider documentation or sandbox",
  errors:
    "Verify this errors fact against provider documentation or sandbox",
  flows:
    "Verify this flows fact against provider documentation or sandbox",
};

export function generateVerifyMarker(
  description: string,
  confidence: number,
  source: FactSource,
  category: FactCategory,
): string {
  return `// VERIFY: ${description} (confidence: ${confidence}, source: ${source}, check: ${CHECK_GUIDANCE[category]})`;
}

// Category priority for trimming (higher index = trimmed first)
const TRIM_PRIORITY: Record<FactCategory, number> = {
  endpoints: 0, // trim last
  webhooks: 1,
  status_codes: 2,
  errors: 3,
  flows: 4, // trim first
};

function getFactDescription(value: unknown): string {
  if (typeof value === "object" && value !== null) {
    const v = value as Record<string, unknown>;
    if ("description" in v && typeof v.description === "string") {
      return v.description;
    }
    if ("url" in v && typeof v.url === "string") {
      return `Endpoint at ${v.url}`;
    }
    if ("event_name" in v && typeof v.event_name === "string") {
      return `Webhook ${v.event_name}`;
    }
    if ("provider_code" in v && typeof v.provider_code === "string") {
      return `Status code ${v.provider_code}`;
    }
    if ("code" in v && typeof v.code === "string") {
      return `Error ${v.code}`;
    }
    if ("name" in v && typeof v.name === "string") {
      return `Flow ${v.name}`;
    }
  }
  return "Unknown fact";
}

// ── Compiler ──

export function compileKnowledgePack(
  pack: KnowledgePack,
  provider: string,
  config: CompilationConfig,
): CompileResult {
  // Validate threshold consistency
  if (config.thresholds.refuse_below > config.thresholds.generate) {
    throw new Error(
      "[KNOWLEDGE_COMPILE_CONFIG] Threshold inconsistency: refuse_below must be <= generate [Fix threshold configuration]",
    );
  }
  if (config.thresholds.verify_min > config.thresholds.generate) {
    throw new Error(
      "[KNOWLEDGE_COMPILE_CONFIG] Threshold inconsistency: verify_min must be <= generate [Fix threshold configuration]",
    );
  }

  const warnings: string[] = [];
  const today = new Date().toISOString().slice(0, 10);
  const version = `${provider}@${today}`;

  // Classify all facts
  let generateCount = 0;
  let verifyCount = 0;
  let refuseCount = 0;

  const allFacts: Array<{
    category: FactCategory;
    value: unknown;
    confidence_score: number;
    source: FactSource;
  }> = [];

  const categories: FactCategory[] = [
    "endpoints",
    "webhooks",
    "status_codes",
    "errors",
    "flows",
  ];

  for (const cat of categories) {
    const entries = pack[cat] as Array<{
      value: unknown;
      confidence_score: number;
      source: FactSource;
    }>;
    for (const entry of entries) {
      if (entry.confidence_score < config.thresholds.refuse_below) {
        refuseCount++;
      } else if (entry.confidence_score >= config.thresholds.generate) {
        generateCount++;
        allFacts.push({
          category: cat,
          value: entry.value,
          confidence_score: entry.confidence_score,
          source: entry.source,
        });
      } else {
        verifyCount++;
        allFacts.push({
          category: cat,
          value: entry.value,
          confidence_score: entry.confidence_score,
          source: entry.source,
        });
      }
    }
  }

  if (refuseCount > 0) {
    warnings.push(
      `Excluded ${refuseCount} facts below refuse threshold (<${config.thresholds.refuse_below}). These will not be used for code generation.`,
    );
  }

  // Build compiled facts with VERIFY markers
  let compiledFacts: CompiledFact[] = allFacts.map((f) => {
    const isVerifyBand = f.confidence_score < config.thresholds.generate;
    const description = getFactDescription(f.value);
    return {
      category: f.category,
      value: f.value,
      confidence_score: f.confidence_score,
      source: f.source,
      ...(isVerifyBand
        ? {
            verify_marker: generateVerifyMarker(
              description,
              f.confidence_score,
              f.source,
              f.category,
            ),
          }
        : {}),
    };
  });

  // Token budget enforcement
  const trimmed: CompiledFact[] = [];
  let json = JSON.stringify({
    version,
    metadata: pack.metadata,
    facts: compiledFacts,
  });
  let tokenCount = estimateTokens(json);

  if (tokenCount > config.token_budget) {
    // Sort by priority for trimming: lowest confidence first, then by category priority
    const sorted = [...compiledFacts].sort((a, b) => {
      if (a.confidence_score !== b.confidence_score) {
        return a.confidence_score - b.confidence_score;
      }
      return TRIM_PRIORITY[b.category] - TRIM_PRIORITY[a.category];
    });

    while (tokenCount > config.token_budget && sorted.length > 0) {
      const removed = sorted.shift()!;
      trimmed.push(removed);
      compiledFacts = sorted.slice();
      json = JSON.stringify({
        version,
        metadata: pack.metadata,
        facts: compiledFacts,
      });
      tokenCount = estimateTokens(json);
    }

    warnings.push(
      `Trimmed ${trimmed.length} lower-priority facts to fit token budget (${estimateTokens(JSON.stringify({ version, metadata: pack.metadata, facts: [...compiledFacts, ...trimmed] }))} -> ${tokenCount} tokens)`,
    );
  }

  // Calculate coverage
  const totalFacts =
    pack.endpoints.length +
    pack.webhooks.length +
    pack.status_codes.length +
    pack.errors.length +
    pack.flows.length;
  const coveragePct =
    totalFacts > 0
      ? Math.round((generateCount / totalFacts) * 100)
      : 0;

  const compiledPack: CompiledPack = {
    version,
    metadata: pack.metadata,
    facts: compiledFacts,
  };

  const meta: CompilationMeta = {
    version,
    token_count: tokenCount,
    coverage_pct: coveragePct,
    confidence_summary: {
      generate: generateCount,
      verify: verifyCount,
      refuse_excluded: refuseCount,
    },
    compiled_at: new Date().toISOString(),
  };

  return {
    compiledPack,
    meta,
    warnings,
    trimmed,
  };
}

export function formatCompileReport(result: CompileResult): string {
  const lines: string[] = [];
  lines.push("=== Compilation Report ===");
  lines.push(`Version: ${result.meta.version}`);
  lines.push(`Token count: ${result.meta.token_count}`);
  lines.push(`Coverage: ${result.meta.coverage_pct}%`);
  lines.push("");
  lines.push("Confidence summary:");
  lines.push(`  Generate (>= threshold): ${result.meta.confidence_summary.generate}`);
  lines.push(`  Verify (with markers): ${result.meta.confidence_summary.verify}`);
  lines.push(`  Refused (excluded): ${result.meta.confidence_summary.refuse_excluded}`);
  lines.push("");
  lines.push(`Facts compiled: ${result.compiledPack.facts.length}`);
  lines.push(`Facts trimmed: ${result.trimmed.length}`);

  if (result.warnings.length > 0) {
    lines.push("");
    lines.push("Warnings:");
    for (const w of result.warnings) {
      lines.push(`  - ${w}`);
    }
  }

  lines.push("");
  lines.push(`Compiled at: ${result.meta.compiled_at}`);
  return lines.join("\n");
}
