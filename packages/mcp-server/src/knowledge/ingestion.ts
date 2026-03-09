import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { parse as parseYaml, stringify as stringifyYaml } from "yaml";
import { KnowledgePackSchema } from "./schema.js";
import type { KnowledgePack, FactCategory } from "./schema.js";
import { parseDocumentation, type ParsedFact } from "./parser.js";
import type { FactSource } from "./schema.js";

export interface IngestResult {
  pack: KnowledgePack;
  factsPerCategory: Record<FactCategory, number>;
  totalFacts: number;
  gaps: GapReport[];
  averageConfidence: number;
  newFacts: number;
  updatedFacts: number;
  unchangedFacts: number;
}

export interface GapReport {
  category: FactCategory;
  suggestion: string;
}

const GAP_SUGGESTIONS: Record<FactCategory, string> = {
  endpoints:
    "Add API endpoint documentation or reference the provider's REST API guide",
  webhooks:
    "Document webhook event types from the provider's webhook/notification documentation",
  status_codes:
    "Add status/result code mappings from the provider's response code reference",
  errors:
    "Document error codes from the provider's error handling documentation",
  flows:
    "Add payment flow sequences describing typical transaction lifecycles",
};

function detectGaps(pack: KnowledgePack): GapReport[] {
  const gaps: GapReport[] = [];
  const categories: FactCategory[] = [
    "endpoints",
    "webhooks",
    "status_codes",
    "errors",
    "flows",
  ];
  for (const cat of categories) {
    if (pack[cat].length === 0) {
      gaps.push({ category: cat, suggestion: GAP_SUGGESTIONS[cat] });
    }
  }
  return gaps;
}

function factsPerCategory(
  pack: KnowledgePack,
): Record<FactCategory, number> {
  return {
    endpoints: pack.endpoints.length,
    webhooks: pack.webhooks.length,
    status_codes: pack.status_codes.length,
    errors: pack.errors.length,
    flows: pack.flows.length,
  };
}

function totalFacts(counts: Record<FactCategory, number>): number {
  return Object.values(counts).reduce((a, b) => a + b, 0);
}

function averageConfidence(pack: KnowledgePack): number {
  const allScores: number[] = [
    ...pack.endpoints.map((e) => e.confidence_score),
    ...pack.webhooks.map((e) => e.confidence_score),
    ...pack.status_codes.map((e) => e.confidence_score),
    ...pack.errors.map((e) => e.confidence_score),
    ...pack.flows.map((e) => e.confidence_score),
  ];
  if (allScores.length === 0) return 0;
  return allScores.reduce((a, b) => a + b, 0) / allScores.length;
}

export function ingestDocumentation(
  sourceText: string,
  sourceType: FactSource,
  existingPack?: KnowledgePack,
): IngestResult {
  const parsed = parseDocumentation(sourceText, sourceType);

  const pack: KnowledgePack = existingPack
    ? structuredClone(existingPack)
    : {
        metadata: {
          name: "",
          display_name: "",
          version: "",
          base_url: "",
          sandbox_url: "",
          documentation_url: "",
        },
        endpoints: [],
        webhooks: [],
        status_codes: [],
        errors: [],
        flows: [],
      };

  let newCount = 0;
  let updatedCount = 0;
  let unchangedCount = 0;

  // Merge endpoints
  const endpointResult = mergeFactsByIdentity(
    pack.endpoints,
    parsed.endpoints,
    (f) => `${f.value.method}:${f.value.url}`,
  );
  pack.endpoints = endpointResult.merged;
  newCount += endpointResult.newCount;
  updatedCount += endpointResult.updatedCount;
  unchangedCount += endpointResult.unchangedCount;

  // Merge webhooks
  const webhookResult = mergeFactsByIdentity(
    pack.webhooks,
    parsed.webhooks,
    (f) => f.value.event_name,
  );
  pack.webhooks = webhookResult.merged;
  newCount += webhookResult.newCount;
  updatedCount += webhookResult.updatedCount;
  unchangedCount += webhookResult.unchangedCount;

  // Merge status codes
  const statusResult = mergeFactsByIdentity(
    pack.status_codes,
    parsed.status_codes,
    (f) => f.value.provider_code,
  );
  pack.status_codes = statusResult.merged;
  newCount += statusResult.newCount;
  updatedCount += statusResult.updatedCount;
  unchangedCount += statusResult.unchangedCount;

  // Merge errors
  const errorResult = mergeFactsByIdentity(
    pack.errors,
    parsed.errors,
    (f) => f.value.code,
  );
  pack.errors = errorResult.merged;
  newCount += errorResult.newCount;
  updatedCount += errorResult.updatedCount;
  unchangedCount += errorResult.unchangedCount;

  // Merge flows
  const flowResult = mergeFactsByIdentity(
    pack.flows,
    parsed.flows,
    (f) => f.value.name,
  );
  pack.flows = flowResult.merged;
  newCount += flowResult.newCount;
  updatedCount += flowResult.updatedCount;
  unchangedCount += flowResult.unchangedCount;

  const counts = factsPerCategory(pack);
  return {
    pack,
    factsPerCategory: counts,
    totalFacts: totalFacts(counts),
    gaps: detectGaps(pack),
    averageConfidence: averageConfidence(pack),
    newFacts: newCount,
    updatedFacts: updatedCount,
    unchangedFacts: unchangedCount,
  };
}

interface MergeResult<T> {
  merged: T[];
  newCount: number;
  updatedCount: number;
  unchangedCount: number;
}

function mergeFactsByIdentity<
  T extends { confidence_score: number; value: unknown },
>(
  existing: T[],
  incoming: T[],
  identityFn: (fact: T) => string,
): MergeResult<T> {
  const map = new Map<string, T>();
  let newCount = 0;
  let updatedCount = 0;
  let unchangedCount = 0;

  for (const fact of existing) {
    map.set(identityFn(fact), fact);
  }

  for (const fact of incoming) {
    const key = identityFn(fact);
    const existingFact = map.get(key);

    if (!existingFact) {
      map.set(key, fact);
      newCount++;
    } else if (fact.confidence_score > existingFact.confidence_score) {
      // Higher confidence source supersedes
      map.set(key, fact);
      updatedCount++;
    } else if (
      fact.confidence_score === existingFact.confidence_score &&
      JSON.stringify(fact.value) !== JSON.stringify(existingFact.value)
    ) {
      // Same confidence but different value — take the newer (incoming) version
      map.set(key, fact);
      updatedCount++;
    } else {
      unchangedCount++;
    }
  }

  return {
    merged: [...map.values()],
    newCount,
    updatedCount,
    unchangedCount,
  };
}

export function ingestFromFile(
  sourcePath: string,
  sourceType: FactSource,
  packYamlPath?: string,
): IngestResult {
  const sourceText = readFileSync(sourcePath, "utf-8");

  let existingPack: KnowledgePack | undefined;
  if (packYamlPath && existsSync(packYamlPath)) {
    const yamlText = readFileSync(packYamlPath, "utf-8");
    const parsed = parseYaml(yamlText);
    existingPack = KnowledgePackSchema.parse(parsed);
  }

  const result = ingestDocumentation(sourceText, sourceType, existingPack);

  // Persist merged pack back to disk
  if (packYamlPath) {
    writeFileSync(packYamlPath, stringifyYaml(result.pack), "utf-8");
  }

  return result;
}

export function adjustFactConfidence<
  T extends { confidence_score: number },
>(facts: T[], identityFn: (fact: T) => string, key: string, newScore: number): T[] {
  if (newScore < 0 || newScore > 1) {
    throw new Error(`Confidence score must be between 0 and 1, got ${newScore}`);
  }
  return facts.map((f) =>
    identityFn(f) === key ? { ...f, confidence_score: newScore } : f,
  );
}

export function formatIngestReport(result: IngestResult): string {
  const lines: string[] = [];
  lines.push("=== Ingestion Report ===");
  lines.push("");
  lines.push("Facts per category:");
  for (const [cat, count] of Object.entries(result.factsPerCategory)) {
    lines.push(`  ${cat}: ${count}`);
  }
  lines.push("");
  lines.push(`Total facts: ${result.totalFacts}`);
  lines.push(`Average confidence: ${result.averageConfidence.toFixed(2)}`);
  lines.push(`New facts: ${result.newFacts}`);
  lines.push(`Updated facts: ${result.updatedFacts}`);
  lines.push(`Unchanged facts: ${result.unchangedFacts}`);

  if (result.gaps.length > 0) {
    lines.push("");
    lines.push("Gaps detected:");
    for (const gap of result.gaps) {
      lines.push(`  [${gap.category}] ${gap.suggestion}`);
    }
  }

  return lines.join("\n");
}
