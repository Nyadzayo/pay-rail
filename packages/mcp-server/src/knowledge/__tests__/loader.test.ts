import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import {
  loadCompiledPack,
  clearPackCache,
  type LoadedPack,
} from "../loader.js";
import type { CompiledPack, CompilationMeta } from "../compiler.js";

const TEST_DIR = join(import.meta.dirname, "__fixtures__", "knowledge-packs");

function makeCompiledPack(provider: string): CompiledPack {
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
    facts: [
      {
        category: "endpoints",
        value: {
          url: "/charges",
          method: "POST",
          parameters: ["amount", "currency"],
          response_schema: '{"id": "string"}',
          description: "Create a charge",
        },
        confidence_score: 0.95,
        source: "sandbox_test",
      },
      {
        category: "webhooks",
        value: {
          event_name: "charge.succeeded",
          payload_schema: '{"id": "string"}',
          trigger_conditions: "When charge succeeds",
          description: "Charge succeeded webhook",
        },
        confidence_score: 0.8,
        source: "official_docs",
        verify_marker:
          "// VERIFY: Charge succeeded webhook (confidence: 0.8, source: official_docs, check: Verify this webhooks fact against provider documentation or sandbox)",
      },
    ],
  };
}

function makeMeta(provider: string): CompilationMeta {
  return {
    version: `${provider}@2026-03-01`,
    token_count: 500,
    coverage_pct: 85,
    confidence_summary: {
      generate: 1,
      verify: 1,
      refuse_excluded: 0,
    },
    compiled_at: "2026-03-01T00:00:00.000Z",
  };
}

function writePackFixture(provider: string): void {
  const dir = join(TEST_DIR, provider, "compiled");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "pack.json"), JSON.stringify(makeCompiledPack(provider)));
  writeFileSync(join(dir, "meta.json"), JSON.stringify(makeMeta(provider)));
}

describe("loadCompiledPack", () => {
  beforeEach(() => {
    clearPackCache();
    rmSync(TEST_DIR, { recursive: true, force: true });
  });

  afterEach(() => {
    clearPackCache();
    rmSync(TEST_DIR, { recursive: true, force: true });
  });

  it("loads a valid compiled pack and meta", () => {
    writePackFixture("test-provider");
    const result = loadCompiledPack("test-provider", TEST_DIR);

    expect(result).not.toBeNull();
    const loaded = result as LoadedPack;
    expect(loaded.pack.version).toBe("test-provider@2026-03-01");
    expect(loaded.pack.facts).toHaveLength(2);
    expect(loaded.meta.token_count).toBe(500);
    expect(loaded.meta.coverage_pct).toBe(85);
  });

  it("returns null for missing provider directory", () => {
    const result = loadCompiledPack("nonexistent", TEST_DIR);
    expect(result).toBeNull();
  });

  it("returns null for missing pack.json", () => {
    const dir = join(TEST_DIR, "empty-provider", "compiled");
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "meta.json"), JSON.stringify(makeMeta("empty-provider")));
    // No pack.json
    const result = loadCompiledPack("empty-provider", TEST_DIR);
    expect(result).toBeNull();
  });

  it("returns null for corrupted JSON", () => {
    const dir = join(TEST_DIR, "bad-provider", "compiled");
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "pack.json"), "not valid json {{{");
    writeFileSync(join(dir, "meta.json"), JSON.stringify(makeMeta("bad-provider")));
    const result = loadCompiledPack("bad-provider", TEST_DIR);
    expect(result).toBeNull();
  });

  it("caches loaded packs for subsequent calls", () => {
    writePackFixture("cached-provider");
    const first = loadCompiledPack("cached-provider", TEST_DIR);
    const second = loadCompiledPack("cached-provider", TEST_DIR);

    expect(first).not.toBeNull();
    expect(second).not.toBeNull();
    // Same object reference means cache hit
    expect(first).toBe(second);
  });

  it("returns fresh data after cache clear", () => {
    writePackFixture("refresh-provider");
    const first = loadCompiledPack("refresh-provider", TEST_DIR);
    clearPackCache();
    const second = loadCompiledPack("refresh-provider", TEST_DIR);

    expect(first).not.toBeNull();
    expect(second).not.toBeNull();
    // Different references after cache clear
    expect(first).not.toBe(second);
    // But same data
    expect((first as LoadedPack).pack.version).toBe((second as LoadedPack).pack.version);
  });

  it("loads meta with confidence summary", () => {
    writePackFixture("meta-provider");
    const result = loadCompiledPack("meta-provider", TEST_DIR) as LoadedPack;

    expect(result.meta.confidence_summary.generate).toBe(1);
    expect(result.meta.confidence_summary.verify).toBe(1);
    expect(result.meta.confidence_summary.refuse_excluded).toBe(0);
  });
});
