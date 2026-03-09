import { describe, it, expect } from "vitest";
import { applyConventions, applyConventionsToAll } from "../convention-matcher.js";
import type { GeneratedFile } from "../pipeline.js";
import type { CodebaseFingerprint, DetectedConvention } from "../../fingerprint/conventions.js";

function makeConvention(overrides: Partial<DetectedConvention> = {}): DetectedConvention {
  return {
    category: "test",
    value: "camelCase",
    confidence: "high",
    evidence: "test",
    ...overrides,
  };
}

function makeFingerprint(overrides: Partial<CodebaseFingerprint> = {}): CodebaseFingerprint {
  return {
    projectPath: "/test",
    scannedAt: "2026-03-06T00:00:00.000Z",
    language: makeConvention({ category: "Language", value: "TypeScript" }),
    framework: makeConvention({ category: "Framework", value: "Express" }),
    orm: makeConvention({ category: "ORM", value: "none" }),
    testFramework: makeConvention({ category: "Test Framework", value: "vitest" }),
    namingConvention: makeConvention({ category: "Naming", value: "camelCase" }),
    moduleStructure: makeConvention({ category: "Module Structure", value: "direct-imports" }),
    additionalConventions: [],
    ...overrides,
  };
}

function makeFile(overrides: Partial<GeneratedFile> = {}): GeneratedFile {
  return {
    name: "test-adapter.ts",
    path: "src/adapters/test-adapter.ts",
    content: "const some_var = 1;\nfunction get_data() {}\n",
    lineCount: 2,
    ...overrides,
  };
}

describe("applyConventions", () => {
  it("converts snake_case identifiers to camelCase when convention is camelCase", () => {
    const fp = makeFingerprint({ namingConvention: makeConvention({ value: "camelCase" }) });
    const file = makeFile({ content: "const some_var = 1;\nfunction get_data() {}\n" });

    const result = applyConventions(file, fp);

    expect(result.content).toContain("someVar");
    expect(result.content).toContain("getData");
  });

  it("converts camelCase identifiers to snake_case when convention is snake_case", () => {
    const fp = makeFingerprint({ namingConvention: makeConvention({ value: "snake_case" }) });
    const file = makeFile({ content: "const someVar = 1;\nfunction getData() {}\n" });

    const result = applyConventions(file, fp);

    expect(result.content).toContain("some_var");
    expect(result.content).toContain("get_data");
  });

  it("does not modify test files", () => {
    const fp = makeFingerprint({ namingConvention: makeConvention({ value: "snake_case" }) });
    const file = makeFile({
      name: "test-adapter.test.ts",
      content: "const someVar = 1;\n",
    });

    const result = applyConventions(file, fp);

    expect(result.content).toContain("someVar");
  });

  it("preserves SCREAMING_SNAKE_CASE constants", () => {
    const fp = makeFingerprint({ namingConvention: makeConvention({ value: "camelCase" }) });
    const file = makeFile({ content: "const MAX_RETRIES = 3;\n" });

    const result = applyConventions(file, fp);

    expect(result.content).toContain("MAX_RETRIES");
  });
});

describe("applyConventionsToAll", () => {
  it("adds barrel export index.ts when moduleStructure is barrel-exports", () => {
    const fp = makeFingerprint({
      moduleStructure: makeConvention({ value: "barrel-exports" }),
    });
    const files: GeneratedFile[] = [
      makeFile({ name: "test-adapter.ts", path: "src/adapters/test-adapter.ts" }),
      makeFile({ name: "test-webhook.ts", path: "src/adapters/test-webhook.ts" }),
      makeFile({ name: "test-idempotency.ts", path: "src/adapters/test-idempotency.ts" }),
    ];

    const result = applyConventionsToAll(files, fp);

    const indexFile = result.find((f) => f.name === "index.ts");
    expect(indexFile).toBeDefined();
    expect(indexFile!.content).toContain('export * from "./test-adapter"');
    expect(indexFile!.content).toContain('export * from "./test-webhook"');
  });

  it("does not add index.ts when moduleStructure is direct-imports", () => {
    const fp = makeFingerprint({
      moduleStructure: makeConvention({ value: "direct-imports" }),
    });
    const files: GeneratedFile[] = [makeFile()];

    const result = applyConventionsToAll(files, fp);

    expect(result.find((f) => f.name === "index.ts")).toBeUndefined();
  });
});
