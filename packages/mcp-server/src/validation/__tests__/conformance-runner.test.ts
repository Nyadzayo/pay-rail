import { describe, it, expect } from "vitest";
import {
  parseTestOutput,
  formatConformanceOutput,
  type ConformanceRunResult,
  type ConformanceTestResult,
} from "../conformance-runner.js";

// -- Mock cargo test output fixtures --
const CARGO_ALL_PASS = `
running 3 tests
test conformance::test_created_to_authorized ... ok
test conformance::test_authorized_to_captured ... ok
test conformance::test_captured_to_refunded ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
`;

const CARGO_MIXED = `
running 4 tests
test conformance::test_created_to_authorized ... ok
test conformance::test_authorized_to_captured ... ok
test conformance::test_created_to_failed ... FAILED
test conformance::test_authorized_to_voided ... FAILED

failures:
    conformance::test_created_to_failed
    conformance::test_authorized_to_voided

test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
`;

const CARGO_ALL_FAIL = `
running 2 tests
test conformance::test_created_to_authorized ... FAILED
test conformance::test_authorized_to_captured ... FAILED

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
`;

// -- Mock vitest output fixtures --
const VITEST_ALL_PASS = `
 ✓ maps provider status to Authorized
 ✓ maps provider status to Captured
 ✓ maps provider status to Refunded

 Test Files  1 passed (1)
      Tests  3 passed (3)
`;

const VITEST_MIXED = `
 ✓ maps provider status to Authorized
 × maps provider status to Captured
 ✓ maps provider status to Refunded
 × maps provider status to Failed

 Test Files  1 failed (1)
      Tests  2 passed | 2 failed (4)
`;

describe("parseTestOutput", () => {
  describe("cargo output", () => {
    it("parses all-pass cargo output", () => {
      const results = parseTestOutput(CARGO_ALL_PASS, "cargo");
      expect(results).toHaveLength(3);
      expect(results.every((r) => r.passed)).toBe(true);
    });

    it("parses mixed cargo output", () => {
      const results = parseTestOutput(CARGO_MIXED, "cargo");
      expect(results).toHaveLength(4);
      expect(results.filter((r) => r.passed)).toHaveLength(2);
      expect(results.filter((r) => !r.passed)).toHaveLength(2);
    });

    it("parses all-fail cargo output", () => {
      const results = parseTestOutput(CARGO_ALL_FAIL, "cargo");
      expect(results).toHaveLength(2);
      expect(results.every((r) => !r.passed)).toBe(true);
    });

    it("extracts transition names from test names", () => {
      const results = parseTestOutput(CARGO_ALL_PASS, "cargo");
      expect(results[0].transition).toContain("Created");
      expect(results[0].transition).toContain("Authorized");
    });
  });

  describe("vitest output", () => {
    it("parses all-pass vitest output", () => {
      const results = parseTestOutput(VITEST_ALL_PASS, "vitest");
      expect(results).toHaveLength(3);
      expect(results.every((r) => r.passed)).toBe(true);
    });

    it("parses mixed vitest output", () => {
      const results = parseTestOutput(VITEST_MIXED, "vitest");
      expect(results).toHaveLength(4);
      expect(results.filter((r) => r.passed)).toHaveLength(2);
      expect(results.filter((r) => !r.passed)).toHaveLength(2);
    });

    it("extracts state names from vitest test descriptions", () => {
      const results = parseTestOutput(VITEST_ALL_PASS, "vitest");
      expect(results[0].expected).toBe("Authorized");
      expect(results[1].expected).toBe("Captured");
    });
  });
});

describe("formatConformanceOutput", () => {
  function makeResult(overrides: Partial<ConformanceRunResult> = {}): ConformanceRunResult {
    return {
      provider: "test-provider",
      passed: 8,
      failed: 0,
      total: 8,
      skipped: 0,
      results: Array.from({ length: 8 }, (_, i) => ({
        transition: `State${i} -> State${i + 1}`,
        passed: true,
        expected: `State${i + 1}`,
        actual: `State${i + 1}`,
      })),
      durationMs: 500,
      ...overrides,
    };
  }

  it("shows all-pass summary in bold (AC #4)", () => {
    const output = formatConformanceOutput(makeResult());
    expect(output).toContain("**8/8 passed.** 0 failures.");
  });

  it("suggests next steps on all-pass (AC #4)", () => {
    const output = formatConformanceOutput(makeResult());
    expect(output).toContain("Deploy to staging");
    expect(output).toContain("VERIFY markers");
  });

  it("shows failure count summary (AC #5)", () => {
    const failures: ConformanceTestResult[] = [
      { transition: "Created -> Failed", passed: false, expected: "Failed", actual: "unknown", details: "No mapping found" },
      { transition: "Authorized -> Voided", passed: false, expected: "Voided", actual: "Failed", details: "Wrong mapping" },
    ];
    const output = formatConformanceOutput(makeResult({
      passed: 6,
      failed: 2,
      total: 8,
      results: [
        ...Array.from({ length: 6 }, (_, i) => ({
          transition: `Pass${i}`,
          passed: true,
          expected: "ok",
          actual: "ok",
        })),
        ...failures,
      ],
    }));
    expect(output).toContain("6/8 passed.");
    expect(output).toContain("2 failures");
  });

  it("expands failure details (AC #5)", () => {
    const output = formatConformanceOutput(makeResult({
      passed: 0,
      failed: 1,
      total: 1,
      results: [{
        transition: "Created -> Failed",
        passed: false,
        expected: "Failed",
        actual: "Authorized",
        details: "Expected Failed but got Authorized",
        sourceHint: "Check status mapping for decline events",
      }],
    }));
    expect(output).toContain("**Failure Details:**");
    expect(output).toContain("Created -> Failed");
    expect(output).toContain("Fix:");
  });

  it("truncates at 5 failures (AC #5)", () => {
    const failures: ConformanceTestResult[] = Array.from({ length: 7 }, (_, i) => ({
      transition: `Fail${i}`,
      passed: false,
      expected: "Expected",
      actual: "Actual",
      details: `Failure ${i}`,
    }));
    const output = formatConformanceOutput(makeResult({
      passed: 0,
      failed: 7,
      total: 7,
      results: failures,
    }));
    expect(output).toContain("...and 2 more failure(s)");
  });

  it("includes results table with markdown (AC #6)", () => {
    const output = formatConformanceOutput(makeResult());
    expect(output).toContain("| Transition | Expected | Actual | Status |");
    expect(output).toContain("PASS");
  });

  it("shows error message when execution fails", () => {
    const output = formatConformanceOutput(makeResult({
      error: "cargo test command not found",
      passed: 0,
      failed: 0,
      total: 0,
      results: [],
    }));
    expect(output).toContain("**Conformance Error:**");
    expect(output).toContain("cargo test command not found");
  });

  it("suggests fix on failure (AC #5)", () => {
    const output = formatConformanceOutput(makeResult({
      passed: 7,
      failed: 1,
      total: 8,
      results: [
        ...Array.from({ length: 7 }, (_, i) => ({
          transition: `Pass${i}`,
          passed: true,
          expected: "ok",
          actual: "ok",
        })),
        {
          transition: "Authorized -> Voided",
          passed: false,
          expected: "Voided",
          actual: "Failed",
          details: "Wrong state mapping",
        },
      ],
    }));
    expect(output).toContain("Fix the 1 failure");
    expect(output).toContain("run_conformance");
  });
});
