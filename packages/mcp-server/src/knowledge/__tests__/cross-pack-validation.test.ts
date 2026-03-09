import { describe, it, expect, beforeAll } from "vitest";
import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { parse as parseYaml } from "yaml";
import { KnowledgePackSchema } from "../schema.js";
import type { KnowledgePack } from "../schema.js";
import {
  compileKnowledgePack,
  type CompilationConfig,
  type CompileResult,
} from "../compiler.js";
import { loadCompiledPack, clearPackCache } from "../loader.js";

const KNOWLEDGE_PACKS_DIR = join(
  import.meta.dirname,
  "..",
  "..",
  "..",
  "..",
  "..",
  "knowledge-packs",
);
const STARTBUTTON_PACK = join(KNOWLEDGE_PACKS_DIR, "startbutton", "pack.yaml");
const PEACH_PACK = join(KNOWLEDGE_PACKS_DIR, "peach-payments", "pack.yaml");

const defaultConfig: CompilationConfig = {
  thresholds: { generate: 0.9, verify_min: 0.7, refuse_below: 0.7 },
  token_budget: 8000,
};

// ── Task 4: Compilation validation ──

describe("Startbutton compilation", () => {
  let pack: KnowledgePack;
  let result: CompileResult;

  beforeAll(() => {
    const yaml = readFileSync(STARTBUTTON_PACK, "utf-8");
    pack = KnowledgePackSchema.parse(parseYaml(yaml));
    result = compileKnowledgePack(pack, "startbutton", defaultConfig);
  });

  it("produces versioned artifact in provider@YYYY-MM-DD format", () => {
    expect(result.compiledPack.version).toMatch(
      /^startbutton@\d{4}-\d{2}-\d{2}$/,
    );
  });

  it("meta reports token count", () => {
    expect(result.meta.token_count).toBeGreaterThan(0);
  });

  it("meta reports coverage percentage (generate-band facts / total)", () => {
    expect(result.meta.coverage_pct).toBeGreaterThanOrEqual(0);
    expect(result.meta.coverage_pct).toBeLessThanOrEqual(100);
    // Coverage = generate-band facts (>= 0.9 confidence) / total facts
    // Startbutton has lower coverage than 100% because some facts are in verify band
    const generateCount = result.meta.confidence_summary.generate;
    const totalFacts = generateCount + result.meta.confidence_summary.verify + result.meta.confidence_summary.refuse_excluded;
    const expectedCoverage = Math.round((generateCount / totalFacts) * 100);
    expect(result.meta.coverage_pct).toBe(expectedCoverage);
  });

  it("meta reports confidence summary with band counts", () => {
    expect(result.meta.confidence_summary.generate).toBeGreaterThanOrEqual(0);
    expect(result.meta.confidence_summary.verify).toBeGreaterThanOrEqual(0);
    expect(result.meta.confidence_summary.refuse_excluded).toBeGreaterThanOrEqual(0);
  });

  it("excludes facts below 0.7 refuse threshold", () => {
    for (const fact of result.compiledPack.facts) {
      expect(fact.confidence_score).toBeGreaterThanOrEqual(0.7);
    }
  });

  it("adds VERIFY markers to facts in 0.7-0.89 band", () => {
    const verifyFacts = result.compiledPack.facts.filter(
      (f) => f.confidence_score < 0.9,
    );
    for (const fact of verifyFacts) {
      expect(fact.verify_marker).toBeDefined();
      expect(fact.verify_marker).toContain("VERIFY:");
    }
  });

  it("does not add VERIFY markers to generate band facts", () => {
    const generateFacts = result.compiledPack.facts.filter(
      (f) => f.confidence_score >= 0.9,
    );
    for (const fact of generateFacts) {
      expect(fact.verify_marker).toBeUndefined();
    }
  });

  it("has verify-band facts (sparse docs)", () => {
    expect(result.meta.confidence_summary.verify).toBeGreaterThan(0);
  });

  it("fits within token budget", () => {
    expect(result.meta.token_count).toBeLessThanOrEqual(defaultConfig.token_budget);
  });

  it("compilation timestamp is ISO 8601 UTC", () => {
    expect(result.meta.compiled_at).toMatch(/Z$/);
  });
});

// ── Task 5: Cross-pack schema validation ──

describe("Cross-pack schema validation (Startbutton vs Peach)", () => {
  let startbuttonPack: KnowledgePack;
  let peachPack: KnowledgePack;
  let startbuttonResult: CompileResult;
  let peachResult: CompileResult;

  beforeAll(() => {
    const sbYaml = readFileSync(STARTBUTTON_PACK, "utf-8");
    startbuttonPack = KnowledgePackSchema.parse(parseYaml(sbYaml));
    startbuttonResult = compileKnowledgePack(
      startbuttonPack,
      "startbutton",
      defaultConfig,
    );

    const peachYaml = readFileSync(PEACH_PACK, "utf-8");
    peachPack = KnowledgePackSchema.parse(parseYaml(peachYaml));
    peachResult = compileKnowledgePack(
      peachPack,
      "peach-payments",
      defaultConfig,
    );
  });

  it("both packs validate against the same KnowledgePackSchema", () => {
    expect(startbuttonPack).toBeDefined();
    expect(peachPack).toBeDefined();
  });

  it("compiled packs share identical top-level structure", () => {
    const sbKeys = Object.keys(startbuttonResult.compiledPack).sort();
    const peachKeys = Object.keys(peachResult.compiledPack).sort();
    expect(sbKeys).toEqual(peachKeys);
  });

  it("compiled facts have identical field schemas", () => {
    if (startbuttonResult.compiledPack.facts.length > 0 && peachResult.compiledPack.facts.length > 0) {
      const sbFactKeys = Object.keys(startbuttonResult.compiledPack.facts[0]).sort();
      const peachFactKeys = Object.keys(peachResult.compiledPack.facts[0]).sort();
      // Both should have: category, confidence_score, source, value (verify_marker is optional)
      const sbRequired = sbFactKeys.filter((k) => k !== "verify_marker");
      const peachRequired = peachFactKeys.filter((k) => k !== "verify_marker");
      expect(sbRequired).toEqual(peachRequired);
    }
  });

  it("meta objects have identical structure", () => {
    const sbMetaKeys = Object.keys(startbuttonResult.meta).sort();
    const peachMetaKeys = Object.keys(peachResult.meta).sort();
    expect(sbMetaKeys).toEqual(peachMetaKeys);
  });

  it("Startbutton has lower average confidence than Peach (expected)", () => {
    const sbFacts = [
      ...startbuttonPack.endpoints,
      ...startbuttonPack.webhooks,
      ...startbuttonPack.status_codes,
      ...startbuttonPack.errors,
      ...startbuttonPack.flows,
    ];
    const peachFacts = [
      ...peachPack.endpoints,
      ...peachPack.webhooks,
      ...peachPack.status_codes,
      ...peachPack.errors,
      ...peachPack.flows,
    ];
    const sbAvg =
      sbFacts.reduce((s, f) => s + f.confidence_score, 0) / sbFacts.length;
    const peachAvg =
      peachFacts.reduce((s, f) => s + f.confidence_score, 0) /
      peachFacts.length;
    expect(sbAvg).toBeLessThan(peachAvg);
  });

  it("Startbutton has more VERIFY markers than Peach (expected)", () => {
    expect(startbuttonResult.meta.confidence_summary.verify).toBeGreaterThan(
      peachResult.meta.confidence_summary.verify,
    );
  });

  it("both packs are self-contained (no cross-references)", () => {
    const sbJson = JSON.stringify(startbuttonResult.compiledPack);
    const peachJson = JSON.stringify(peachResult.compiledPack);

    // Startbutton pack should not reference Peach
    expect(sbJson).not.toContain("peach");
    expect(sbJson).not.toContain("oppwa");

    // Peach pack should not reference Startbutton
    expect(peachJson).not.toContain("startbutton");
  });

  it("both packs are independently distributable (have metadata)", () => {
    expect(startbuttonResult.compiledPack.metadata.name).toBe("startbutton");
    expect(peachResult.compiledPack.metadata.name).toBe("peach_payments");
  });
});

// ── Compiled artifacts on disk ──

describe("Compiled artifacts on disk", () => {
  beforeAll(() => {
    clearPackCache();
  });

  it("loader can load compiled Startbutton pack", () => {
    const loaded = loadCompiledPack("startbutton", KNOWLEDGE_PACKS_DIR);
    expect(loaded).toBeDefined();
    expect(loaded!.pack.version).toMatch(/^startbutton@/);
    expect(loaded!.meta.token_count).toBeGreaterThan(0);
  });

  it("loader can load compiled Peach Payments pack", () => {
    const loaded = loadCompiledPack("peach-payments", KNOWLEDGE_PACKS_DIR);
    expect(loaded).toBeDefined();
    expect(loaded!.pack.version).toMatch(/^peach-payments@/);
    expect(loaded!.meta.token_count).toBeGreaterThan(0);
  });
});
