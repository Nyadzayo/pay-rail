import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import {
  runPipeline,
  formatGenerationOutput,
  classifyFacts,
  type GenerationResult,
} from "../pipeline.js";
import { clearPackCache } from "../../knowledge/loader.js";
import { clearFingerprintCache } from "../../fingerprint/scanner.js";
import type { CompiledPack, CompiledFact } from "../../knowledge/compiler.js";
import type { PayRailConfig } from "../../config/schema.js";

const FIXTURES_DIR = join(import.meta.dirname, "__fixtures__");
const PACKS_DIR = join(FIXTURES_DIR, "knowledge-packs");
const PROJECT_DIR = join(FIXTURES_DIR, "sample-project");

function makeConfig(overrides: Partial<PayRailConfig> = {}): PayRailConfig {
  return {
    confidence: { generate: 0.9, verify_min: 0.7 },
    token_budget: 14000,
    knowledge_packs_path: PACKS_DIR,
    ...overrides,
  };
}

function makeFact(overrides: Partial<CompiledFact> = {}): CompiledFact {
  return {
    category: "endpoints",
    value: {
      url: "/v1/payments",
      method: "POST",
      parameters: ["amount", "currency"],
      response_schema: '{"id": "string"}',
      description: "Create a payment",
    },
    confidence_score: 0.95,
    source: "sandbox_test",
    ...overrides,
  };
}

function writePackFixture(provider: string, facts: CompiledFact[]): void {
  const dir = join(PACKS_DIR, provider, "compiled");
  mkdirSync(dir, { recursive: true });
  const pack: CompiledPack = {
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
  writeFileSync(join(dir, "pack.json"), JSON.stringify(pack));
  writeFileSync(
    join(dir, "meta.json"),
    JSON.stringify({
      version: pack.version,
      token_count: 500,
      coverage_pct: 85,
      confidence_summary: { generate: facts.length, verify: 0, refuse_excluded: 0 },
      compiled_at: "2026-03-01T00:00:00.000Z",
    }),
  );
}

function createSampleProject(): void {
  mkdirSync(join(PROJECT_DIR, "src"), { recursive: true });
  writeFileSync(
    join(PROJECT_DIR, "package.json"),
    JSON.stringify({
      name: "sample",
      dependencies: { express: "4.18.0" },
      devDependencies: { vitest: "1.0.0", typescript: "5.0.0" },
    }),
  );
  writeFileSync(join(PROJECT_DIR, "src", "app.ts"), "export function handleRequest() {}\nexport const maxRetries = 3;\n");
}

describe("classifyFacts", () => {
  it("classifies facts by confidence thresholds", () => {
    const facts: CompiledFact[] = [
      makeFact({ confidence_score: 0.95 }),
      makeFact({ confidence_score: 0.85 }),
      makeFact({ confidence_score: 0.5 }),
    ];

    const result = classifyFacts(facts, 0.9, 0.7);

    expect(result.generate).toHaveLength(1);
    expect(result.verify).toHaveLength(1);
    expect(result.refused).toHaveLength(1);
  });

  it("puts boundary value 0.9 in generate", () => {
    const facts = [makeFact({ confidence_score: 0.9 })];
    const result = classifyFacts(facts, 0.9, 0.7);
    expect(result.generate).toHaveLength(1);
  });

  it("puts boundary value 0.7 in verify", () => {
    const facts = [makeFact({ confidence_score: 0.7 })];
    const result = classifyFacts(facts, 0.9, 0.7);
    expect(result.verify).toHaveLength(1);
  });

  it("puts value below 0.7 in refused", () => {
    const facts = [makeFact({ confidence_score: 0.69 })];
    const result = classifyFacts(facts, 0.9, 0.7);
    expect(result.refused).toHaveLength(1);
  });
});

describe("runPipeline", () => {
  beforeEach(() => {
    clearPackCache();
    clearFingerprintCache();
    rmSync(FIXTURES_DIR, { recursive: true, force: true });
  });

  afterEach(() => {
    clearPackCache();
    clearFingerprintCache();
    rmSync(FIXTURES_DIR, { recursive: true, force: true });
  });

  it("executes 4-step pipeline with narration (AC #2)", () => {
    const facts = [makeFact({ confidence_score: 0.95 })];
    writePackFixture("test-provider", facts);
    const config = makeConfig();

    const result = runPipeline("test-provider", "typescript", config);

    expect(result.narration).toContain("Loading test-provider knowledge pack...");
    expect(result.narration).toContain("Scanning codebase conventions...");
    expect(result.narration).toContain("Generating adapter...");
    expect(result.narration).toContain("Validating state machine correctness...");
    expect(result.narration).toContain("Checking convention match...");
  });

  it("generates 4 TypeScript files (AC #1)", () => {
    const facts = [
      makeFact({ confidence_score: 0.95, category: "endpoints" }),
      makeFact({
        confidence_score: 0.92,
        category: "status_codes",
        value: { provider_code: "000.000", canonical_state: "Captured", description: "Success" },
      }),
    ];
    writePackFixture("test-provider", facts);
    const config = makeConfig();

    const result = runPipeline("test-provider", "typescript", config);

    expect(result.files).toHaveLength(4);
    const names = result.files.map((f) => f.name);
    expect(names).toContain("test-provider-adapter.ts");
    expect(names).toContain("test-provider-webhook.ts");
    expect(names).toContain("test-provider-adapter.test.ts");
    expect(names).toContain("test-provider-idempotency.ts");
  });

  it("includes confidence stats in result (AC #7)", () => {
    const facts = [
      makeFact({ confidence_score: 0.95 }),
      makeFact({ confidence_score: 0.8 }),
      makeFact({ confidence_score: 0.5 }),
    ];
    writePackFixture("stats-provider", facts);
    const config = makeConfig();

    const result = runPipeline("stats-provider", "typescript", config);

    expect(result.confidenceStats.totalFacts).toBe(3);
    expect(result.confidenceStats.generatedDirectly).toBe(1);
    expect(result.confidenceStats.withVerifyMarkers).toBe(1);
    expect(result.confidenceStats.refused).toBe(1);
    expect(result.confidenceStats.overallPct).toBe(33);
  });

  it("generates VERIFY markers for facts with 0.7-0.89 confidence (AC #4)", () => {
    const facts = [
      makeFact({
        confidence_score: 0.8,
        category: "status_codes",
        value: { provider_code: "100.100", canonical_state: "Authorized", description: "Auth pending" },
      }),
    ];
    writePackFixture("verify-provider", facts);
    const config = makeConfig();

    const result = runPipeline("verify-provider", "typescript", config);

    const adapterFile = result.files.find((f) => f.name.includes("adapter") && !f.name.includes("test"));
    expect(adapterFile).toBeDefined();
    expect(adapterFile!.content).toContain("// VERIFY:");
    expect(result.verifyCount).toBeGreaterThan(0);
  });

  it("generates directly for facts >= 0.9 without VERIFY (AC #3)", () => {
    const facts = [
      makeFact({
        confidence_score: 0.95,
        category: "status_codes",
        value: { provider_code: "000.000", canonical_state: "Captured", description: "Success" },
      }),
    ];
    writePackFixture("direct-provider", facts);
    const config = makeConfig();

    const result = runPipeline("direct-provider", "typescript", config);

    const adapterFile = result.files.find((f) => f.name.includes("adapter") && !f.name.includes("test"));
    expect(adapterFile).toBeDefined();
    expect(adapterFile!.content).toContain('"000.000": "Captured"');
    // The status code line itself should not have a VERIFY marker
    const statusLine = adapterFile!.content.split("\n").find((l) => l.includes('"000.000"'));
    expect(statusLine).not.toContain("VERIFY");
  });

  it("refuses facts < 0.7 and reports them (AC #5)", () => {
    const facts = [
      makeFact({ confidence_score: 0.5, category: "endpoints" }),
    ];
    writePackFixture("low-provider", facts);
    const config = makeConfig();

    const result = runPipeline("low-provider", "typescript", config);

    expect(result.confidenceStats.refused).toBe(1);
  });

  it("handles missing knowledge pack with warnings (AC #6)", () => {
    const config = makeConfig();

    const result = runPipeline("nonexistent-provider", "typescript", config);

    expect(result.warnings.some((w) => w.includes("No knowledge pack"))).toBe(true);
    expect(result.narration.some((n) => n.includes("VERIFY markers"))).toBe(true);
    // Adapter should still be generated with VERIFY markers on all mappings
    expect(result.files.length).toBeGreaterThan(0);
    const adapterFile = result.files.find((f) => f.name.includes("adapter") && !f.name.includes("test"));
    expect(adapterFile).toBeDefined();
    expect(adapterFile!.content).toContain("VERIFY");
  });

  it("populates convention match when projectPath provided (AC #1)", () => {
    const facts = [makeFact({ confidence_score: 0.95 })];
    writePackFixture("conv-provider", facts);
    createSampleProject();
    const config = makeConfig();

    const result = runPipeline("conv-provider", "typescript", config, PROJECT_DIR);

    expect(result.conventionMatch.language).toBe("TypeScript");
    expect(result.conventionMatch.testFramework).toBe("vitest");
  });

  it("adapter includes all 8 canonical state references (AC #1)", () => {
    const facts = [makeFact({ confidence_score: 0.95 })];
    writePackFixture("states-provider", facts);
    const config = makeConfig();

    const result = runPipeline("states-provider", "typescript", config);
    const adapterFile = result.files.find((f) => f.name.includes("adapter") && !f.name.includes("test"));

    // Type definition includes all canonical states
    expect(adapterFile!.content).toContain("Created");
    expect(adapterFile!.content).toContain("Authorized");
    expect(adapterFile!.content).toContain("Captured");
    expect(adapterFile!.content).toContain("Refunded");
    expect(adapterFile!.content).toContain("Voided");
    expect(adapterFile!.content).toContain("Failed");
    expect(adapterFile!.content).toContain("Expired");
    expect(adapterFile!.content).toContain("Pending3ds");
  });

  it("generates idempotency config with correct key format", () => {
    const facts = [makeFact({ confidence_score: 0.95 })];
    writePackFixture("idem-provider", facts);
    const config = makeConfig();

    const result = runPipeline("idem-provider", "typescript", config);
    const idempFile = result.files.find((f) => f.name.includes("idempotency"));

    expect(idempFile).toBeDefined();
    expect(idempFile!.content).toContain("idem-provider:{merchantId}:webhook:{eventId}");
    expect(idempFile!.content).toContain("30 * 24 * 60 * 60");
  });

  it("webhook handler includes signature verification", () => {
    const facts = [makeFact({ confidence_score: 0.95 })];
    writePackFixture("sig-provider", facts);
    const config = makeConfig();

    const result = runPipeline("sig-provider", "typescript", config);
    const webhookFile = result.files.find((f) => f.name.includes("webhook"));

    expect(webhookFile).toBeDefined();
    expect(webhookFile!.content).toContain("verifySignature");
    expect(webhookFile!.content).toContain("createHmac");
  });

  it("includes inline comments for payment logic (AC #8)", () => {
    const facts = [makeFact({ confidence_score: 0.95 })];
    writePackFixture("comments-provider", facts);
    const config = makeConfig();

    const result = runPipeline("comments-provider", "typescript", config);
    const adapterFile = result.files.find((f) => f.name.includes("adapter") && !f.name.includes("test"));

    expect(adapterFile!.content).toContain("integer cents");
    expect(adapterFile!.content).toContain("never floating point");
  });
});

describe("formatGenerationOutput", () => {
  function makeResult(overrides: Partial<GenerationResult> = {}): GenerationResult {
    return {
      provider: "test-provider",
      targetLanguage: "typescript",
      files: [
        { name: "test.ts", path: "src/test.ts", content: "const x = 1;", lineCount: 1 },
      ],
      confidenceStats: {
        totalFacts: 3,
        generatedDirectly: 2,
        withVerifyMarkers: 1,
        refused: 0,
        overallPct: 67,
      },
      verifyCount: 1,
      conventionMatch: {
        language: "TypeScript",
        framework: "Express",
        naming: "camelCase",
        testFramework: "vitest",
        moduleStructure: "direct-imports",
      },
      narration: ["Loading knowledge pack...", "Generating adapter..."],
      warnings: [],
      ...overrides,
    };
  }

  it("produces structured output with file table (AC #7)", () => {
    const output = formatGenerationOutput(makeResult());

    expect(output).toContain("## Generated Files");
    expect(output).toContain("| File | Path | Lines |");
    expect(output).toContain("test.ts");
  });

  it("includes confidence and VERIFY stats (AC #7)", () => {
    const output = formatGenerationOutput(makeResult());

    expect(output).toContain("67% facts generated directly");
    expect(output).toContain("VERIFY markers");
  });

  it("includes narration steps", () => {
    const output = formatGenerationOutput(makeResult());

    expect(output).toContain("> Loading knowledge pack...");
    expect(output).toContain("> Generating adapter...");
  });

  it("includes run_conformance suggestion (AC #7)", () => {
    const output = formatGenerationOutput(makeResult());

    expect(output).toContain("run_conformance");
  });

  it("includes warnings when present", () => {
    const output = formatGenerationOutput(makeResult({ warnings: ["Something is off"] }));

    expect(output).toContain("## Warnings");
    expect(output).toContain("Something is off");
  });

  it("includes file contents in fenced code blocks (AC #7)", () => {
    const output = formatGenerationOutput(makeResult());

    expect(output).toContain("## File Contents");
    expect(output).toContain("```typescript");
    expect(output).toContain("const x = 1;");
  });
});
