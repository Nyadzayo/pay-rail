import { describe, it, expect, beforeAll } from "vitest";
import { join } from "node:path";
import { loadCompiledPack, clearPackCache } from "../loader.js";
import { assembleContext } from "../../context/assembler.js";
import { DEFAULT_TOTAL_BUDGET } from "../../context/token-budget.js";
import { runPipeline } from "../../generation/pipeline.js";
import type { PayRailConfig } from "../../config/schema.js";

const KNOWLEDGE_PACKS_DIR = join(
  import.meta.dirname,
  "..",
  "..",
  "..",
  "..",
  "..",
  "knowledge-packs",
);

// -- Task 3: Multi-pack loading and routing --

describe("Multi-provider knowledge pack loading (Task 3)", () => {
  beforeAll(() => {
    clearPackCache();
  });

  it("loads both providers simultaneously without interference", () => {
    const peach = loadCompiledPack("peach-payments", KNOWLEDGE_PACKS_DIR);
    const sb = loadCompiledPack("startbutton", KNOWLEDGE_PACKS_DIR);

    expect(peach).not.toBeNull();
    expect(sb).not.toBeNull();

    // Each pack has its own identity
    expect(peach!.pack.metadata.name).toBe("peach_payments");
    expect(sb!.pack.metadata.name).toBe("startbutton");
  });

  it("returns correct pack for each provider query", () => {
    const peach = loadCompiledPack("peach-payments", KNOWLEDGE_PACKS_DIR);
    const sb = loadCompiledPack("startbutton", KNOWLEDGE_PACKS_DIR);

    // Facts are provider-specific — no cross-contamination
    const peachEndpoints = peach!.pack.facts.filter((f) => f.category === "endpoints");
    const sbEndpoints = sb!.pack.facts.filter((f) => f.category === "endpoints");

    // Peach endpoints reference oppwa URLs
    for (const ep of peachEndpoints) {
      const v = ep.value as Record<string, unknown>;
      expect(JSON.stringify(v)).not.toContain("startbutton");
    }

    // Startbutton endpoints don't reference Peach
    for (const ep of sbEndpoints) {
      const v = ep.value as Record<string, unknown>;
      expect(JSON.stringify(v)).not.toContain("oppwa");
      expect(JSON.stringify(v)).not.toContain("peach");
    }
  });

  it("cache isolates providers correctly", () => {
    clearPackCache();

    // Load peach first
    const peach1 = loadCompiledPack("peach-payments", KNOWLEDGE_PACKS_DIR);
    // Load startbutton — should not affect peach cache
    const sb = loadCompiledPack("startbutton", KNOWLEDGE_PACKS_DIR);
    // Re-load peach — should get cached version
    const peach2 = loadCompiledPack("peach-payments", KNOWLEDGE_PACKS_DIR);

    expect(peach1).toBe(peach2); // Same reference (cached)
    expect(peach1!.pack.metadata.name).toBe("peach_payments");
    expect(sb!.pack.metadata.name).toBe("startbutton");
  });

  it("unknown provider returns null without affecting loaded packs", () => {
    const result = loadCompiledPack("nonexistent", KNOWLEDGE_PACKS_DIR);
    expect(result).toBeNull();

    // Other packs still load fine
    const peach = loadCompiledPack("peach-payments", KNOWLEDGE_PACKS_DIR);
    expect(peach).not.toBeNull();
  });
});

// -- Task 3.4-3.5: Context assembly per provider --

describe("Multi-provider context assembly (Task 3.4-3.5)", () => {
  beforeAll(() => {
    clearPackCache();
  });

  it("assembles context for peach-payments within 14K budget", () => {
    const ctx = assembleContext("peach-payments", KNOWLEDGE_PACKS_DIR);

    expect(ctx.loaded).toBe(true);
    expect(ctx.totalTokens).toBeLessThanOrEqual(DEFAULT_TOTAL_BUDGET);
    expect(ctx.universalTokens).toBeGreaterThan(0);
    expect(ctx.providerTokens).toBeGreaterThan(0);
    expect(ctx.providerContext.length).toBeGreaterThan(0);
  });

  it("assembles context for startbutton within 14K budget", () => {
    const ctx = assembleContext("startbutton", KNOWLEDGE_PACKS_DIR);

    expect(ctx.loaded).toBe(true);
    expect(ctx.totalTokens).toBeLessThanOrEqual(DEFAULT_TOTAL_BUDGET);
    expect(ctx.universalTokens).toBeGreaterThan(0);
    expect(ctx.providerTokens).toBeGreaterThan(0);
    expect(ctx.providerContext.length).toBeGreaterThan(0);
  });

  it("provider contexts are distinct (no cross-contamination)", () => {
    const peachCtx = assembleContext("peach-payments", KNOWLEDGE_PACKS_DIR);
    const sbCtx = assembleContext("startbutton", KNOWLEDGE_PACKS_DIR);

    // Universal rules are the same
    expect(peachCtx.universalRules).toBe(sbCtx.universalRules);

    // Provider contexts differ
    expect(peachCtx.providerContext).not.toBe(sbCtx.providerContext);
  });

  it("both providers fit within token budget independently", () => {
    const peachCtx = assembleContext("peach-payments", KNOWLEDGE_PACKS_DIR);
    const sbCtx = assembleContext("startbutton", KNOWLEDGE_PACKS_DIR);

    expect(peachCtx.totalTokens).toBeLessThanOrEqual(DEFAULT_TOTAL_BUDGET);
    expect(sbCtx.totalTokens).toBeLessThanOrEqual(DEFAULT_TOTAL_BUDGET);

    // Token breakdown: universal (~2K) + provider (~4-6K) + codebase (0 — no projectPath)
    expect(peachCtx.universalTokens).toBeLessThanOrEqual(2000);
    expect(sbCtx.universalTokens).toBeLessThanOrEqual(2000);
    expect(peachCtx.providerTokens).toBeLessThanOrEqual(6000);
    expect(sbCtx.providerTokens).toBeLessThanOrEqual(6000);
  });
});

// -- Task 3.3: Generation pipeline uses correct pack per provider --

describe("Generation pipeline multi-provider routing (Task 3.3)", () => {
  const config: PayRailConfig = {
    confidence: { generate: 0.9, verify_min: 0.7 },
    token_budget: 14000,
    knowledge_packs_path: KNOWLEDGE_PACKS_DIR,
  };

  beforeAll(() => {
    clearPackCache();
  });

  it("generates with peach-payments pack for peach provider", () => {
    const result = runPipeline("peach-payments", "typescript", config);
    expect(result.provider).toBe("peach-payments");
    expect(result.confidenceStats.totalFacts).toBeGreaterThan(0);
    // Peach has higher confidence — more direct generation, fewer VERIFY markers
    expect(result.confidenceStats.generatedDirectly).toBeGreaterThan(0);
  });

  it("generates with startbutton pack for startbutton provider", () => {
    const result = runPipeline("startbutton", "typescript", config);
    expect(result.provider).toBe("startbutton");
    expect(result.confidenceStats.totalFacts).toBeGreaterThan(0);
    // Startbutton has more VERIFY markers (sparser docs)
    expect(result.confidenceStats.withVerifyMarkers).toBeGreaterThan(0);
  });

  it("different providers produce different generation results", () => {
    const peachResult = runPipeline("peach-payments", "typescript", config);
    const sbResult = runPipeline("startbutton", "typescript", config);

    // Different providers, different fact counts
    expect(peachResult.provider).not.toBe(sbResult.provider);
    // Both have files generated
    expect(peachResult.files.length).toBeGreaterThan(0);
    expect(sbResult.files.length).toBeGreaterThan(0);
  });
});

// -- Task 5: End-to-end pipeline validation --

describe("End-to-end pipeline validation (Task 5)", () => {
  it("both packs have correct version format", () => {
    const peach = loadCompiledPack("peach-payments", KNOWLEDGE_PACKS_DIR);
    const sb = loadCompiledPack("startbutton", KNOWLEDGE_PACKS_DIR);

    expect(peach!.pack.version).toMatch(/^peach-payments@\d{4}-\d{2}-\d{2}$/);
    expect(sb!.pack.version).toMatch(/^startbutton@\d{4}-\d{2}-\d{2}$/);
  });

  it("both packs have complete fact categories", () => {
    const peach = loadCompiledPack("peach-payments", KNOWLEDGE_PACKS_DIR);
    const sb = loadCompiledPack("startbutton", KNOWLEDGE_PACKS_DIR);

    const categories = ["endpoints", "webhooks", "status_codes", "errors", "flows"];

    for (const cat of categories) {
      const peachFacts = peach!.pack.facts.filter((f) => f.category === cat);
      const sbFacts = sb!.pack.facts.filter((f) => f.category === cat);
      expect(peachFacts.length).toBeGreaterThan(0);
      expect(sbFacts.length).toBeGreaterThan(0);
    }
  });

  it("startbutton workflow mirrors peach workflow (same schema)", () => {
    const peach = loadCompiledPack("peach-payments", KNOWLEDGE_PACKS_DIR);
    const sb = loadCompiledPack("startbutton", KNOWLEDGE_PACKS_DIR);

    // Same top-level structure
    const peachKeys = Object.keys(peach!.pack).sort();
    const sbKeys = Object.keys(sb!.pack).sort();
    expect(peachKeys).toEqual(sbKeys);

    // Same meta structure
    const peachMetaKeys = Object.keys(peach!.meta).sort();
    const sbMetaKeys = Object.keys(sb!.meta).sort();
    expect(peachMetaKeys).toEqual(sbMetaKeys);
  });

  it("adding startbutton required zero architecture changes", () => {
    // Startbutton pack uses the exact same schema as Peach
    const peach = loadCompiledPack("peach-payments", KNOWLEDGE_PACKS_DIR);
    const sb = loadCompiledPack("startbutton", KNOWLEDGE_PACKS_DIR);

    // Both have identical fact field schemas
    if (peach!.pack.facts.length > 0 && sb!.pack.facts.length > 0) {
      const peachFactKeys = Object.keys(peach!.pack.facts[0])
        .filter((k) => k !== "verify_marker")
        .sort();
      const sbFactKeys = Object.keys(sb!.pack.facts[0])
        .filter((k) => k !== "verify_marker")
        .sort();
      expect(peachFactKeys).toEqual(sbFactKeys);
    }

    // Both packs are self-contained
    const peachJson = JSON.stringify(peach!.pack);
    const sbJson = JSON.stringify(sb!.pack);
    expect(peachJson).not.toContain("startbutton");
    expect(sbJson).not.toContain("oppwa");
  });
});
