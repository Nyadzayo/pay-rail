import { describe, it, expect } from "vitest";
import { analyzeStateMachine, formatValidationOutput } from "../state-machine-analyzer.js";
import { formatConformanceOutput, type ConformanceRunResult } from "../conformance-runner.js";
import { estimateTokenCount } from "../../context/token-budget.js";

const LARGE_ADAPTER = `
const STATUS_MAP: Record<string, CanonicalState> = {
  "000.000.000": "Captured",
  "000.100.110": "Authorized",
  "000.100.112": "Authorized",
  "100.100.100": "Pending3ds",
  "100.100.101": "Pending3ds",
  "800.100.100": "Failed",
  "800.100.200": "Failed",
  "800.200.100": "Failed",
  "900.100.100": "Expired",
  "900.100.200": "Expired",
};

// Created -> Authorized, Created -> Failed, Created -> Expired, Created -> Pending3ds
// Pending3ds -> Authorized, Pending3ds -> Failed, Pending3ds -> Expired
// Authorized -> Captured, Authorized -> Voided, Authorized -> Failed, Authorized -> Expired
// Captured -> Refunded, Captured -> Failed

const WEBHOOK_EVENTS = {
  "charge.succeeded": true,
  "charge.failed": true,
  "auth.succeeded": true,
  "auth.failed": true,
  "void.succeeded": true,
  "refund.succeeded": true,
  "3ds.pending": true,
  "payment.expired": true,
};

export class TestAdapter {
  async execute(command) {}

  translateWebhook(event) {
    // Handle self-transition / duplicate events
    if (event.state_before === event.state_after) {
      return { state: event.state_before };
    }

    switch (event.type) {
      case "charge.succeeded": return { state: "Captured" };
      case "charge.failed": return { state: "Failed" };
      case "auth.succeeded": return { state: "Authorized" };
      case "void.succeeded": return { state: "Voided" };
      case "refund.succeeded": return { state: "Refunded" };
      case "3ds.pending": return { state: "Pending3ds" };
      case "payment.expired": return { state: "Expired" };
    }
  }

  signatureConfig() {
    return { header: "x-signature", algorithm: "sha256", secret: "" };
  }
}
`;

describe("performance", () => {
  it("state machine validation completes in <2 seconds (AC #6, Task 4.1)", () => {
    const start = Date.now();
    analyzeStateMachine({ code: LARGE_ADAPTER });
    const elapsed = Date.now() - start;
    expect(elapsed).toBeLessThan(2000);
  });

  it("validation output fits within ~500 token budget (AC #6, Task 4.3)", () => {
    const result = analyzeStateMachine({ code: LARGE_ADAPTER });
    const output = formatValidationOutput(result);
    const tokens = estimateTokenCount(output);
    expect(tokens).toBeLessThan(600); // ~500 with some margin
  });

  it("conformance output fits within ~500 token budget (AC #6, Task 4.3)", () => {
    const result: ConformanceRunResult = {
      provider: "test-provider",
      passed: 6,
      failed: 2,
      total: 8,
      skipped: 0,
      results: [
        ...Array.from({ length: 6 }, (_, i) => ({
          transition: `Created -> State${i}`,
          passed: true as const,
          expected: `State${i}`,
          actual: `State${i}`,
        })),
        {
          transition: "Created -> Failed",
          passed: false as const,
          expected: "Failed",
          actual: "Authorized",
          details: "Expected Failed but got Authorized for decline event",
          sourceHint: "Check status mapping for 800.* result codes",
        },
        {
          transition: "Authorized -> Voided",
          passed: false as const,
          expected: "Voided",
          actual: "Failed",
          details: "Void event mapped to Failed instead of Voided",
          sourceHint: "Map void.succeeded event to Voided state",
        },
      ],
      durationMs: 1200,
    };
    const output = formatConformanceOutput(result);
    const tokens = estimateTokenCount(output);
    expect(tokens).toBeLessThan(600);
  });

  it("validation output uses structured markdown (AC #6, Task 4.4)", () => {
    const result = analyzeStateMachine({ code: LARGE_ADAPTER });
    const output = formatValidationOutput(result);
    expect(output).toContain("|"); // table
    expect(output).toContain("**"); // bold
  });

  it("conformance output uses structured markdown (AC #6, Task 4.4)", () => {
    const result: ConformanceRunResult = {
      provider: "test-provider",
      passed: 8,
      failed: 0,
      total: 8,
      skipped: 0,
      results: [],
      durationMs: 500,
    };
    const output = formatConformanceOutput(result);
    expect(output).toContain("**"); // bold summary
  });
});
