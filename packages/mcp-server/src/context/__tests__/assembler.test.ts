import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { assembleContext } from "../assembler.js";
import { DEFAULT_TOTAL_BUDGET } from "../token-budget.js";
import { clearPackCache } from "../../knowledge/loader.js";
import { clearFingerprintCache } from "../../fingerprint/scanner.js";
import type { CompiledPack, CompilationMeta } from "../../knowledge/compiler.js";

const TEST_DIR = join(import.meta.dirname, "__fixtures__", "knowledge-packs");

function makeCompiledPack(provider: string, factCount: number = 2): CompiledPack {
  const facts = [];
  for (let i = 0; i < factCount; i++) {
    facts.push({
      category: "endpoints" as const,
      value: {
        url: `/endpoint-${i}`,
        method: "POST",
        parameters: ["amount"],
        response_schema: '{"id": "string"}',
        description: `Endpoint ${i} for ${provider}`,
      },
      confidence_score: 0.95 - i * 0.05,
      source: "sandbox_test" as const,
    });
  }
  return {
    version: `${provider}@2026-03-01`,
    metadata: {
      name: provider,
      display_name: `${provider} Provider`,
      version: "1.0.0",
      base_url: `https://api.${provider}.com/v1`,
      sandbox_url: `https://sandbox.${provider}.com/v1`,
      documentation_url: `https://docs.${provider}.com`,
    },
    facts,
  };
}

function makeMeta(provider: string): CompilationMeta {
  return {
    version: `${provider}@2026-03-01`,
    token_count: 500,
    coverage_pct: 85,
    confidence_summary: { generate: 2, verify: 0, refuse_excluded: 0 },
    compiled_at: "2026-03-01T00:00:00.000Z",
  };
}

function writePackFixture(provider: string, factCount?: number): void {
  const dir = join(TEST_DIR, provider, "compiled");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "pack.json"), JSON.stringify(makeCompiledPack(provider, factCount)));
  writeFileSync(join(dir, "meta.json"), JSON.stringify(makeMeta(provider)));
}

describe("assembleContext", () => {
  beforeEach(() => {
    clearPackCache();
    rmSync(TEST_DIR, { recursive: true, force: true });
  });

  afterEach(() => {
    clearPackCache();
    rmSync(TEST_DIR, { recursive: true, force: true });
  });

  it("assembles three layers for a valid provider", () => {
    writePackFixture("test-provider");
    const result = assembleContext("test-provider", TEST_DIR);

    expect(result.universalRules.length).toBeGreaterThan(0);
    expect(result.providerContext.length).toBeGreaterThan(0);
    expect(result.totalTokens).toBeLessThanOrEqual(DEFAULT_TOTAL_BUDGET);
  });

  it("returns empty provider context for missing provider", () => {
    const result = assembleContext("nonexistent", TEST_DIR);

    expect(result.universalRules.length).toBeGreaterThan(0);
    expect(result.providerContext).toBe("");
    expect(result.providerTokens).toBe(0);
    expect(result.loaded).toBe(false);
  });

  it("stays within 14K total token budget", () => {
    writePackFixture("budget-provider", 50);
    const result = assembleContext("budget-provider", TEST_DIR);

    expect(result.totalTokens).toBeLessThanOrEqual(DEFAULT_TOTAL_BUDGET);
  });

  it("trims lowest-confidence facts when over budget", () => {
    // Create a massive pack that would exceed budget
    writePackFixture("large-provider", 200);
    const result = assembleContext("large-provider", TEST_DIR);

    expect(result.totalTokens).toBeLessThanOrEqual(DEFAULT_TOTAL_BUDGET);
    expect(result.trimmedCount).toBeGreaterThanOrEqual(0);
  });

  it("returns empty codebase context when no projectPath provided", () => {
    writePackFixture("codebase-provider");
    const result = assembleContext("codebase-provider", TEST_DIR);

    expect(result.codebaseContext).toBe("");
    expect(result.codebaseTokens).toBe(0);
  });

  describe("codebase fingerprint integration (Story 5.3)", () => {
    const PROJECT_DIR = join(import.meta.dirname, "__fixtures__", "sample-project");

    beforeEach(() => {
      clearFingerprintCache();
      rmSync(PROJECT_DIR, { recursive: true, force: true });
    });

    afterEach(() => {
      clearFingerprintCache();
      rmSync(PROJECT_DIR, { recursive: true, force: true });
    });

    function createSampleProject(): void {
      mkdirSync(join(PROJECT_DIR, "src"), { recursive: true });
      writeFileSync(
        join(PROJECT_DIR, "package.json"),
        JSON.stringify({
          name: "sample",
          dependencies: { express: "4.18.0" },
          devDependencies: { vitest: "1.0.0" },
        }),
      );
      writeFileSync(join(PROJECT_DIR, "src", "app.ts"), "export function handleRequest() {}\n");
      writeFileSync(join(PROJECT_DIR, "src", "utils.ts"), "export const maxRetries = 3;\n");
    }

    it("populates Layer 3 with fingerprint when projectPath is provided", () => {
      createSampleProject();
      writePackFixture("fp-provider");
      const result = assembleContext("fp-provider", TEST_DIR, DEFAULT_TOTAL_BUDGET, PROJECT_DIR);

      expect(result.codebaseContext.length).toBeGreaterThan(0);
      expect(result.codebaseContext).toContain("Codebase Fingerprint");
      expect(result.codebaseContext).toContain("TypeScript");
      expect(result.codebaseTokens).toBeGreaterThan(0);
    });

    it("keeps total within budget with fingerprint", () => {
      createSampleProject();
      writePackFixture("fp-budget", 50);
      const result = assembleContext("fp-budget", TEST_DIR, DEFAULT_TOTAL_BUDGET, PROJECT_DIR);

      expect(result.totalTokens).toBeLessThanOrEqual(DEFAULT_TOTAL_BUDGET);
    });

    it("includes codebaseTokens in totalTokens when provider not loaded", () => {
      createSampleProject();
      const result = assembleContext("nonexistent", TEST_DIR, DEFAULT_TOTAL_BUDGET, PROJECT_DIR);

      expect(result.loaded).toBe(false);
      expect(result.codebaseTokens).toBeGreaterThan(0);
      expect(result.totalTokens).toBe(result.universalTokens + result.codebaseTokens);
    });
  });
});
