import { describe, it, expect } from "vitest";
import {
  compileKnowledgePack,
  estimateTokens,
  generateVerifyMarker,
  type CompileResult,
  type CompilationConfig,
  type CompiledFact,
} from "../compiler.js";
import type { KnowledgePack } from "../schema.js";

function makePack(overrides?: Partial<KnowledgePack>): KnowledgePack {
  return {
    metadata: {
      name: "test-provider",
      display_name: "Test Provider",
      version: "1.0.0",
      base_url: "https://api.test.com/v1",
      sandbox_url: "https://sandbox.test.com/v1",
      documentation_url: "https://docs.test.com",
    },
    endpoints: [],
    webhooks: [],
    status_codes: [],
    errors: [],
    flows: [],
    ...overrides,
  };
}

function makeEndpoint(
  url: string,
  confidence: number,
  source: "sandbox_test" | "official_docs" | "community_report" = "official_docs",
) {
  return {
    value: {
      url,
      method: "POST",
      parameters: ["amount", "currency"],
      response_schema: '{"id": "string"}',
      description: `Endpoint at ${url}`,
    },
    confidence_score: confidence,
    source,
    verification_date: "2026-03-01T00:00:00Z",
    decay_rate: 0.05,
  };
}

function makeStatusCode(code: string, confidence: number) {
  return {
    value: {
      provider_code: code,
      canonical_state: "captured",
      description: `Status ${code}`,
    },
    confidence_score: confidence,
    source: "official_docs" as const,
    verification_date: "2026-03-01T00:00:00Z",
    decay_rate: 0.05,
  };
}

function makeError(code: string, confidence: number) {
  return {
    value: {
      code,
      description: `Error ${code}`,
      recovery_action: "Retry",
    },
    confidence_score: confidence,
    source: "official_docs" as const,
    verification_date: "2026-03-01T00:00:00Z",
    decay_rate: 0.05,
  };
}

const defaultConfig: CompilationConfig = {
  thresholds: { generate: 0.9, verify_min: 0.7, refuse_below: 0.7 },
  token_budget: 8000,
};

// ── Task 1: YAML-to-JSON compiler ──

describe("compileKnowledgePack", () => {
  it("compiles empty pack into valid CompileResult", () => {
    const pack = makePack();
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);

    expect(result.compiledPack).toBeDefined();
    expect(result.compiledPack.metadata).toEqual(pack.metadata);
    expect(result.compiledPack.facts).toEqual([]);
    expect(result.meta.version).toMatch(/^test-provider@\d{4}-\d{2}-\d{2}$/);
    expect(result.meta.compiled_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it("includes generate-band facts directly without markers", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.95)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);

    expect(result.compiledPack.facts).toHaveLength(1);
    expect(result.compiledPack.facts[0].verify_marker).toBeUndefined();
    expect(result.compiledPack.facts[0].category).toBe("endpoints");
    expect(result.compiledPack.facts[0].confidence_score).toBe(0.95);
  });

  it("validates pack against schema", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.95)],
      status_codes: [makeStatusCode("000.000.000", 0.92)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);

    expect(result.compiledPack.facts).toHaveLength(2);
    // All facts should have required fields
    for (const fact of result.compiledPack.facts) {
      expect(fact.category).toBeDefined();
      expect(fact.confidence_score).toBeDefined();
      expect(fact.source).toBeDefined();
      expect(fact.value).toBeDefined();
    }
  });
});

// ── Task 2: meta.json generation ──

describe("meta generation", () => {
  it("generates version in provider@YYYY-MM-DD format", () => {
    const pack = makePack();
    const result = compileKnowledgePack(pack, "peach-payments", defaultConfig);
    expect(result.meta.version).toMatch(/^peach-payments@\d{4}-\d{2}-\d{2}$/);
  });

  it("counts tokens in compiled pack", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.95)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.meta.token_count).toBeGreaterThan(0);
  });

  it("calculates coverage percentage", () => {
    const pack = makePack({
      endpoints: [
        makeEndpoint("/v1/payments", 0.95),
        makeEndpoint("/v1/refunds", 0.5), // refuse band, excluded
      ],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    // 1 generate-band out of 2 total => 50% coverage
    expect(result.meta.coverage_pct).toBe(50);
  });

  it("generates confidence summary with band counts", () => {
    const pack = makePack({
      endpoints: [
        makeEndpoint("/v1/a", 0.95), // generate
        makeEndpoint("/v1/b", 0.75), // verify
        makeEndpoint("/v1/c", 0.5), // refuse
      ],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.meta.confidence_summary.generate).toBe(1);
    expect(result.meta.confidence_summary.verify).toBe(1);
    expect(result.meta.confidence_summary.refuse_excluded).toBe(1);
  });

  it("includes compilation timestamp in UTC", () => {
    const pack = makePack();
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.meta.compiled_at).toMatch(/Z$/);
  });
});

// ── Task 3: Token budget enforcement ──

describe("token budget enforcement", () => {
  it("estimates tokens as chars/4", () => {
    expect(estimateTokens("abcd")).toBe(1);
    expect(estimateTokens("abcdefgh")).toBe(2);
    expect(estimateTokens("")).toBe(0);
  });

  it("trims lowest-confidence facts when over budget", () => {
    // Create a pack with many facts that exceed budget
    const endpoints = [];
    for (let i = 0; i < 50; i++) {
      endpoints.push(makeEndpoint(`/v1/endpoint-${i}`, 0.9 + (i % 10) * 0.01));
    }
    const pack = makePack({ endpoints });

    const tightConfig: CompilationConfig = {
      ...defaultConfig,
      token_budget: 500, // Very tight budget
    };
    const result = compileKnowledgePack(pack, "test-provider", tightConfig);

    expect(result.meta.token_count).toBeLessThanOrEqual(500);
    expect(result.trimmed.length).toBeGreaterThan(0);
  });

  it("reports trimmed facts with warning message", () => {
    const endpoints = [];
    for (let i = 0; i < 50; i++) {
      endpoints.push(makeEndpoint(`/v1/endpoint-${i}`, 0.9 + (i % 10) * 0.01));
    }
    const pack = makePack({ endpoints });

    const tightConfig: CompilationConfig = {
      ...defaultConfig,
      token_budget: 500,
    };
    const result = compileKnowledgePack(pack, "test-provider", tightConfig);
    expect(result.warnings).toContainEqual(
      expect.stringContaining("Trimmed"),
    );
  });

  it("does not trim when within budget", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.95)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.trimmed).toHaveLength(0);
  });
});

// ── Task 4: VERIFY marker generation ──

describe("VERIFY marker generation", () => {
  it("generates marker for verify-band facts (0.7-0.89)", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.75, "official_docs")],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);

    expect(result.compiledPack.facts).toHaveLength(1);
    const fact = result.compiledPack.facts[0];
    expect(fact.verify_marker).toBeDefined();
    expect(fact.verify_marker).toContain("VERIFY:");
    expect(fact.verify_marker).toContain("confidence: 0.75");
    expect(fact.verify_marker).toContain("source: official_docs");
  });

  it("does not generate marker for generate-band facts (>= 0.9)", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.95)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.compiledPack.facts[0].verify_marker).toBeUndefined();
  });

  it("produces exact VERIFY format string", () => {
    const marker = generateVerifyMarker(
      "Endpoint at /v1/payments",
      0.75,
      "official_docs",
      "endpoints",
    );
    expect(marker).toBe(
      "// VERIFY: Endpoint at /v1/payments (confidence: 0.75, source: official_docs, check: Verify this endpoints fact against provider documentation or sandbox)",
    );
  });

  it("handles boundary confidence 0.7 as verify band", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.7)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.compiledPack.facts).toHaveLength(1);
    expect(result.compiledPack.facts[0].verify_marker).toBeDefined();
  });

  it("handles boundary confidence 0.89 as verify band", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.89)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.compiledPack.facts).toHaveLength(1);
    expect(result.compiledPack.facts[0].verify_marker).toBeDefined();
  });
});

// ── Task 5: Refuse threshold exclusion ──

describe("refuse threshold exclusion", () => {
  it("excludes facts below 0.7 from compiled pack", () => {
    const pack = makePack({
      endpoints: [
        makeEndpoint("/v1/payments", 0.95), // keep
        makeEndpoint("/v1/refunds", 0.5), // exclude
        makeEndpoint("/v1/voids", 0.3), // exclude
      ],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.compiledPack.facts).toHaveLength(1);
    expect(result.compiledPack.facts[0].value).toHaveProperty(
      "url",
      "/v1/payments",
    );
  });

  it("reports excluded count", () => {
    const pack = makePack({
      endpoints: [
        makeEndpoint("/v1/payments", 0.95),
        makeEndpoint("/v1/refunds", 0.5),
      ],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.meta.confidence_summary.refuse_excluded).toBe(1);
    expect(result.warnings).toContainEqual(
      expect.stringContaining("Excluded 1"),
    );
  });

  it("includes excluded count in meta", () => {
    const pack = makePack({
      endpoints: [
        makeEndpoint("/v1/a", 0.5),
        makeEndpoint("/v1/b", 0.3),
      ],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.meta.confidence_summary.refuse_excluded).toBe(2);
  });

  it("handles all-refuse pack gracefully", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/a", 0.5)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.compiledPack.facts).toHaveLength(0);
    expect(result.meta.coverage_pct).toBe(0);
  });
});

// ── Task 6: Configurable thresholds ──

describe("configurable thresholds", () => {
  it("uses custom generate threshold", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.85)],
    });
    const customConfig: CompilationConfig = {
      thresholds: { generate: 0.8, verify_min: 0.5, refuse_below: 0.5 },
      token_budget: 8000,
    };
    const result = compileKnowledgePack(pack, "test-provider", customConfig);
    // 0.85 >= 0.8, so no verify marker
    expect(result.compiledPack.facts[0].verify_marker).toBeUndefined();
  });

  it("uses custom verify threshold", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.55)],
    });
    const customConfig: CompilationConfig = {
      thresholds: { generate: 0.9, verify_min: 0.5, refuse_below: 0.5 },
      token_budget: 8000,
    };
    const result = compileKnowledgePack(pack, "test-provider", customConfig);
    // 0.55 >= 0.5 and < 0.9, so verify marker
    expect(result.compiledPack.facts[0].verify_marker).toBeDefined();
  });

  it("uses custom refuse threshold", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.45)],
    });
    const customConfig: CompilationConfig = {
      thresholds: { generate: 0.9, verify_min: 0.5, refuse_below: 0.5 },
      token_budget: 8000,
    };
    const result = compileKnowledgePack(pack, "test-provider", customConfig);
    // 0.45 < 0.5, excluded
    expect(result.compiledPack.facts).toHaveLength(0);
  });

  it("validates threshold consistency", () => {
    const pack = makePack();
    const badConfig: CompilationConfig = {
      thresholds: { generate: 0.5, verify_min: 0.9, refuse_below: 0.9 },
      token_budget: 8000,
    };
    expect(() =>
      compileKnowledgePack(pack, "test-provider", badConfig),
    ).toThrow(/threshold/i);
  });
});

// ── Task 7: Self-contained artifact validation ──

describe("self-contained artifact", () => {
  it("compiled pack includes version for cache invalidation", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.95)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    expect(result.compiledPack.version).toBeDefined();
    expect(result.compiledPack.version).toMatch(/^test-provider@/);
  });

  it("compiled pack contains no external references", () => {
    const pack = makePack({
      endpoints: [makeEndpoint("/v1/payments", 0.95)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    const json = JSON.stringify(result.compiledPack);
    // No file paths or relative references
    expect(json).not.toContain("pack.yaml");
    expect(json).not.toContain("../");
    expect(json).not.toContain("./");
  });

  it("round-trip: all generate-band facts present in output", () => {
    const pack = makePack({
      endpoints: [
        makeEndpoint("/v1/payments", 0.95),
        makeEndpoint("/v1/refunds", 0.92),
      ],
      status_codes: [makeStatusCode("000.000.000", 0.91)],
      errors: [makeError("ERR001", 0.93)],
    });
    const result = compileKnowledgePack(pack, "test-provider", defaultConfig);
    // All 4 facts are >= 0.9, all should be present
    expect(result.compiledPack.facts).toHaveLength(4);
  });
});
