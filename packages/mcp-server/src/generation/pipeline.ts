import { loadCompiledPack, type LoadedPack } from "../knowledge/loader.js";
import { scanCodebase } from "../fingerprint/scanner.js";
import type { CodebaseFingerprint } from "../fingerprint/conventions.js";
import type { CompiledFact } from "../knowledge/compiler.js";
import type { PayRailConfig } from "../config/schema.js";
import { generateAdapterFiles } from "./templates.js";
import { applyConventionsToAll } from "./convention-matcher.js";
import { CANONICAL_STATES } from "../validation/canonical-states.js";

export interface GeneratedFile {
  name: string;
  path: string;
  content: string;
  lineCount: number;
}

export interface GenerationResult {
  provider: string;
  targetLanguage: "typescript" | "rust";
  files: GeneratedFile[];
  confidenceStats: {
    totalFacts: number;
    generatedDirectly: number;
    withVerifyMarkers: number;
    refused: number;
    overallPct: number;
  };
  verifyCount: number;
  conventionMatch: {
    language: string;
    framework: string;
    naming: string;
    testFramework: string;
    moduleStructure: string;
  };
  narration: string[];
  warnings: string[];
}

export type PipelineStep = "analyze" | "generate" | "validate" | "fit-check";

interface AnalyzeResult {
  pack: LoadedPack | null;
  fingerprint: CodebaseFingerprint | null;
  facts: {
    generate: CompiledFact[];
    verify: CompiledFact[];
    refused: CompiledFact[];
  };
}

// Re-export for consumers that imported from pipeline
export { CANONICAL_STATES } from "../validation/canonical-states.js";

export function classifyFacts(
  facts: CompiledFact[],
  generateThreshold: number,
  verifyMin: number,
): { generate: CompiledFact[]; verify: CompiledFact[]; refused: CompiledFact[] } {
  const generate: CompiledFact[] = [];
  const verify: CompiledFact[] = [];
  const refused: CompiledFact[] = [];

  for (const fact of facts) {
    if (fact.confidence_score >= generateThreshold) {
      generate.push(fact);
    } else if (fact.confidence_score >= verifyMin) {
      verify.push(fact);
    } else {
      refused.push(fact);
    }
  }

  return { generate, verify, refused };
}

function analyzeStep(
  provider: string,
  config: PayRailConfig,
  projectPath?: string,
): AnalyzeResult {
  const knowledgePacksPath = config.knowledge_packs_path ?? "knowledge-packs";
  const pack = loadCompiledPack(provider, knowledgePacksPath);
  const fingerprint = projectPath ? scanCodebase(projectPath) : null;

  const allFacts = pack?.pack.facts ?? [];
  const facts = classifyFacts(
    allFacts,
    config.confidence.generate,
    config.confidence.verify_min,
  );

  return { pack, fingerprint, facts };
}

function validateStep(
  files: GeneratedFile[],
  narration: string[],
): string[] {
  const warnings: string[] = [];
  narration.push("Validating state machine correctness...");

  for (const file of files) {
    if (file.name.includes("adapter") && !file.name.includes(".test.")) {
      const missingStates = CANONICAL_STATES.filter(
        (state) => !file.content.includes(state),
      );
      if (missingStates.length > 0) {
        warnings.push(
          `Adapter may be missing state mappings for: ${missingStates.join(", ")}`,
        );
      }
    }
  }

  return warnings;
}

function fitCheckStep(
  fingerprint: CodebaseFingerprint | null,
  files: GeneratedFile[],
  narration: string[],
): string[] {
  const warnings: string[] = [];
  narration.push("Checking convention match...");

  if (!fingerprint) {
    warnings.push("No codebase fingerprint available — convention match skipped");
    return warnings;
  }

  for (const file of files) {
    if (file.name.endsWith(".ts") && fingerprint.language.value !== "TypeScript") {
      warnings.push(
        `Generated TypeScript file but detected language is ${fingerprint.language.value}`,
      );
    }
  }

  return warnings;
}

export function runPipeline(
  provider: string,
  targetLanguage: "typescript" | "rust",
  config: PayRailConfig,
  projectPath?: string,
): GenerationResult {
  const narration: string[] = [];
  const warnings: string[] = [];

  // Step 1: Analyze
  narration.push(`Loading ${provider} knowledge pack...`);
  narration.push("Scanning codebase conventions...");
  const analysis = analyzeStep(provider, config, projectPath);

  if (!analysis.pack) {
    narration.push(`No knowledge pack found for "${provider}" — generating with VERIFY markers on all mappings`);
    warnings.push(`No knowledge pack for "${provider}". All generated mappings will have VERIFY markers.`);
  }

  // Step 2: Generate
  narration.push("Generating adapter...");
  const rawFiles = generateAdapterFiles(
    provider,
    targetLanguage,
    analysis.facts,
    analysis.fingerprint,
    config,
  );

  // Step 3: Apply conventions (includes barrel exports when detected)
  const files = analysis.fingerprint
    ? applyConventionsToAll(rawFiles, analysis.fingerprint!)
    : rawFiles;

  // Step 4: Validate
  const validateWarnings = validateStep(files, narration);
  warnings.push(...validateWarnings);

  // Step 5: Fit-check
  const fitCheckWarnings = fitCheckStep(analysis.fingerprint, files, narration);
  warnings.push(...fitCheckWarnings);

  const totalFacts = analysis.facts.generate.length + analysis.facts.verify.length + analysis.facts.refused.length;
  const generatedDirectly = analysis.facts.generate.length;
  const withVerifyMarkers = analysis.facts.verify.length;
  const refused = analysis.facts.refused.length;

  const verifyCount = files.reduce(
    (count, f) => count + (f.content.match(/\/\/ VERIFY:/g) ?? []).length,
    0,
  );

  const fp = analysis.fingerprint;

  return {
    provider,
    targetLanguage,
    files,
    confidenceStats: {
      totalFacts,
      generatedDirectly,
      withVerifyMarkers,
      refused,
      overallPct: totalFacts > 0 ? Math.round((generatedDirectly / totalFacts) * 100) : 0,
    },
    verifyCount,
    conventionMatch: {
      language: fp?.language.value ?? "unknown",
      framework: fp?.framework.value ?? "unknown",
      naming: fp?.namingConvention.value ?? "unknown",
      testFramework: fp?.testFramework.value ?? "unknown",
      moduleStructure: fp?.moduleStructure.value ?? "unknown",
    },
    narration,
    warnings,
  };
}

function escapeCell(text: string | number): string {
  return String(text ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
}

export function formatGenerationOutput(result: GenerationResult): string {
  const sections: string[] = [];

  // Narration
  for (const msg of result.narration) {
    sections.push(`> ${msg}`);
  }
  sections.push("");

  // File table
  sections.push("## Generated Files\n");
  sections.push("| File | Path | Lines |");
  sections.push("|------|------|-------|");
  for (const f of result.files) {
    sections.push(`| ${escapeCell(f.name)} | ${escapeCell(f.path)} | ${escapeCell(f.lineCount)} |`);
  }
  sections.push("");

  // Stats
  sections.push("## Generation Stats\n");
  sections.push(`| Metric | Value |`);
  sections.push(`|--------|-------|`);
  sections.push(`| Confidence | ${result.confidenceStats.overallPct}% facts generated directly |`);
  sections.push(`| VERIFY markers | ${result.verifyCount} |`);
  sections.push(`| Facts refused (low confidence) | ${result.confidenceStats.refused} |`);
  sections.push(`| Convention match | ${result.conventionMatch.naming} naming, ${result.conventionMatch.testFramework} tests |`);
  sections.push("");

  // Warnings
  if (result.warnings.length > 0) {
    sections.push("## Warnings\n");
    for (const w of result.warnings) {
      sections.push(`- ${w}`);
    }
    sections.push("");
  }

  // File contents
  sections.push("## File Contents\n");
  for (const f of result.files) {
    const ext = f.name.endsWith(".rs") ? "rust" : "typescript";
    sections.push(`### ${f.path}\n`);
    sections.push(`\`\`\`${ext}`);
    sections.push(f.content);
    sections.push("```\n");
  }

  // Next step
  sections.push('> **Next step:** Run `run_conformance` to validate all state transitions.');

  return sections.join("\n");
}
