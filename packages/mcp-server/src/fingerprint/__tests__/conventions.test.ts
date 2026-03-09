import { describe, it, expect } from "vitest";
import {
  formatFingerprintAsMarkdown,
  type CodebaseFingerprint,
  type DetectedConvention,
} from "../conventions.js";

function makeConvention(overrides: Partial<DetectedConvention> = {}): DetectedConvention {
  return {
    category: "Language",
    value: "TypeScript",
    confidence: "high",
    evidence: "3/3 source files (100%)",
    ...overrides,
  };
}

function makeFingerprint(overrides: Partial<CodebaseFingerprint> = {}): CodebaseFingerprint {
  return {
    projectPath: "/test",
    scannedAt: "2026-03-06T00:00:00.000Z",
    language: makeConvention(),
    framework: makeConvention({ category: "Framework", value: "Next.js" }),
    orm: makeConvention({ category: "ORM", value: "Prisma" }),
    testFramework: makeConvention({ category: "Test Framework", value: "vitest" }),
    namingConvention: makeConvention({ category: "Naming", value: "camelCase" }),
    moduleStructure: makeConvention({ category: "Module Structure", value: "barrel-exports" }),
    additionalConventions: [],
    ...overrides,
  };
}

describe("formatFingerprintAsMarkdown", () => {
  it("produces a valid markdown table with header", () => {
    const md = formatFingerprintAsMarkdown(makeFingerprint());

    expect(md).toContain("**Codebase Fingerprint**");
    expect(md).toContain("| Category | Detected | Confidence | Evidence |");
    expect(md).toContain("| Language | TypeScript | high |");
  });

  it("includes all 6 convention rows", () => {
    const md = formatFingerprintAsMarkdown(makeFingerprint());
    const tableRows = md.split("\n").filter((l) => l.startsWith("| ") && !l.startsWith("|--"));

    // 6 convention rows + header row
    expect(tableRows.length).toBe(7);
  });

  it("renders alternatives when present", () => {
    const fp = makeFingerprint({
      namingConvention: makeConvention({
        category: "Naming",
        value: "camelCase",
        confidence: "low",
        alternatives: [
          { value: "snake_case", evidence: "3/10 identifiers (30%)" },
        ],
      }),
    });
    const md = formatFingerprintAsMarkdown(fp);

    expect(md).toContain("_alt:_ snake_case");
    expect(md).toContain("3/10 identifiers (30%)");
  });

  it("escapes pipe characters in values", () => {
    const fp = makeFingerprint({
      language: makeConvention({
        evidence: "found in src|lib directories",
      }),
    });
    const md = formatFingerprintAsMarkdown(fp);

    expect(md).toContain("src\\|lib");
    expect(md).not.toMatch(/\| found in src\|lib/);
  });

  it("handles empty additionalConventions", () => {
    const fp = makeFingerprint({ additionalConventions: [] });
    const md = formatFingerprintAsMarkdown(fp);

    expect(md).toBeDefined();
    expect(md.length).toBeGreaterThan(0);
  });

  it("includes additionalConventions in output", () => {
    const fp = makeFingerprint({
      additionalConventions: [
        makeConvention({ category: "Custom", value: "some-pattern", evidence: "found it" }),
      ],
    });
    const md = formatFingerprintAsMarkdown(fp);

    expect(md).toContain("Custom");
    expect(md).toContain("some-pattern");
  });

  it("uses scannedAt date in header", () => {
    const fp = makeFingerprint({ scannedAt: "2026-03-06T12:34:56.000Z" });
    const md = formatFingerprintAsMarkdown(fp);

    expect(md).toContain("scanned: 2026-03-06");
  });
});
