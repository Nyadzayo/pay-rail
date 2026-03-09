import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { analyzeStateMachine, formatValidationOutput } from "../state-machine-analyzer.js";

const FIXTURES_DIR = join(import.meta.dirname, "__fixtures__");

beforeEach(() => {
  rmSync(FIXTURES_DIR, { recursive: true, force: true });
  mkdirSync(FIXTURES_DIR, { recursive: true });
});

afterEach(() => {
  rmSync(FIXTURES_DIR, { recursive: true, force: true });
});

// -- Valid adapter code covering all 8 states --
const VALID_ADAPTER = `
const STATUS_MAP = {
  "000.000": "Captured",
  "100.100": "Authorized",
  "800.100": "Failed",
  "900.100": "Expired",
};

// Transitions: Created -> Authorized, Created -> Failed, Created -> Expired, Created -> Pending3ds
// Pending3ds -> Authorized, Pending3ds -> Failed, Pending3ds -> Expired
// Authorized -> Captured, Authorized -> Voided, Authorized -> Failed, Authorized -> Expired
// Captured -> Refunded, Captured -> Failed

function translateWebhook(event) {
  // Handle duplicate events (self-transition / idempotency)
  if (event.state_before === event.state_after) {
    return; // already in state
  }

  switch (event.type) {
    case "charge.succeeded": return "Captured";
    case "charge.failed": return "Failed";
    case "auth.succeeded": return "Authorized";
    case "void.succeeded": return "Voided";
    case "refund.succeeded": return "Refunded";
    case "3ds.pending": return "Pending3ds";
    case "payment.expired": return "Expired";
  }
}
`;

const MISSING_STATES_ADAPTER = `
const STATUS_MAP = {
  "000.000": "Captured",
  "100.100": "Authorized",
};
// Created -> Authorized
// Authorized -> Captured
`;

const INVALID_TRANSITION_ADAPTER = `
// Created -> Refunded (INVALID: can't refund from Created)
const STATUS_MAP = {
  "000": "Created",
  "100": "Authorized",
  "200": "Captured",
  "300": "Refunded",
};
// Created -> Refunded
switch (event.type) {
  case "refund.completed": return "Refunded"; // from Created
}
`;

describe("analyzeStateMachine", () => {
  describe("valid adapter (AC #1)", () => {
    it("returns valid=true for adapter with all states covered", () => {
      const result = analyzeStateMachine({ code: VALID_ADAPTER });
      expect(result.valid).toBe(true);
      expect(result.issues.filter((i) => i.severity === "error")).toHaveLength(0);
    });

    it("detects all 8 mapped states", () => {
      const result = analyzeStateMachine({ code: VALID_ADAPTER });
      expect(result.mappedStates).toContain("Created");
      expect(result.mappedStates).toContain("Authorized");
      expect(result.mappedStates).toContain("Captured");
      expect(result.mappedStates).toContain("Refunded");
      expect(result.mappedStates).toContain("Voided");
      expect(result.mappedStates).toContain("Failed");
      expect(result.mappedStates).toContain("Expired");
      expect(result.mappedStates).toContain("Pending3ds");
    });

    it("detects self-transition handling", () => {
      const result = analyzeStateMachine({ code: VALID_ADAPTER });
      expect(result.hasSelfTransitionHandling).toBe(true);
    });
  });

  describe("missing transitions (AC #1, #2)", () => {
    it("reports missing states as issues", () => {
      const result = analyzeStateMachine({ code: MISSING_STATES_ADAPTER });
      expect(result.valid).toBe(false);
      const missingIssues = result.issues.filter((i) => i.description.includes("No mapping found"));
      expect(missingIssues.length).toBeGreaterThan(0);
    });

    it("reports missing self-transition handling", () => {
      const result = analyzeStateMachine({ code: MISSING_STATES_ADAPTER });
      const selfIssue = result.issues.find((i) => i.description.includes("self-transition"));
      expect(selfIssue).toBeDefined();
      expect(selfIssue!.severity).toBe("info");
    });
  });

  describe("invalid transitions (AC #1, #2)", () => {
    it("detects invalid transition Created -> Refunded", () => {
      const code = `
        // Created -> Refunded
        const map = { "Created": "start", "Refunded": "end" };
      `;
      const result = analyzeStateMachine({ code });
      // It should find the arrow in comment
      const invalidIssues = result.issues.filter((i) => i.description.includes("Invalid transition"));
      // Created -> Refunded is not a valid transition
      expect(invalidIssues.some((i) => i.description.includes("Created") && i.description.includes("Refunded"))).toBe(true);
    });
  });

  describe("severity categorization (AC #2)", () => {
    it("assigns error severity to missing non-terminal states", () => {
      const code = `const map = { "state": "Captured" };`;
      const result = analyzeStateMachine({ code });
      const createdIssue = result.issues.find((i) => i.description.includes('"Created"'));
      expect(createdIssue).toBeDefined();
      expect(createdIssue!.severity).toBe("error");
    });

    it("assigns warning severity to missing terminal states", () => {
      const code = `
        const map = { "Created": true, "Authorized": true, "Captured": true, "Pending3ds": true };
        // Created -> Authorized -> Captured
        // Created -> Pending3ds
      `;
      const result = analyzeStateMachine({ code });
      const refundedIssue = result.issues.find((i) => i.description.includes('"Refunded"'));
      expect(refundedIssue).toBeDefined();
      expect(refundedIssue!.severity).toBe("warning");
    });

    it("assigns info severity to missing self-transition handling", () => {
      const code = `const map = { "state": "Captured" };`;
      const result = analyzeStateMachine({ code });
      const selfIssue = result.issues.find((i) => i.description.includes("self-transition"));
      expect(selfIssue).toBeDefined();
      expect(selfIssue!.severity).toBe("info");
    });

    it("sorts issues: errors first, then warnings, then info", () => {
      const result = analyzeStateMachine({ code: MISSING_STATES_ADAPTER });
      const severities = result.issues.map((i) => i.severity);
      const errorIdx = severities.indexOf("error");
      const warningIdx = severities.indexOf("warning");
      const infoIdx = severities.indexOf("info");
      if (errorIdx >= 0 && warningIdx >= 0) expect(errorIdx).toBeLessThan(warningIdx);
      if (warningIdx >= 0 && infoIdx >= 0) expect(warningIdx).toBeLessThan(infoIdx);
    });
  });

  describe("issue details (AC #2)", () => {
    it("includes description, location, fix, and reference", () => {
      const code = `const map = { "state": "Captured" };`;
      const result = analyzeStateMachine({ code });
      for (const issue of result.issues) {
        expect(issue.description).toBeTruthy();
        expect(issue.location).toBeTruthy();
        expect(issue.fix).toBeTruthy();
        expect(issue.reference).toBeTruthy();
      }
    });
  });

  describe("code input modes (AC #1, Task 2.1)", () => {
    it("accepts inline code snippet", () => {
      const result = analyzeStateMachine({ code: VALID_ADAPTER });
      expect(result.mappedStates.length).toBeGreaterThan(0);
    });

    it("accepts file path", () => {
      const filePath = join(FIXTURES_DIR, "test-adapter.ts");
      writeFileSync(filePath, VALID_ADAPTER);
      const result = analyzeStateMachine({ filePath });
      expect(result.mappedStates.length).toBeGreaterThan(0);
    });

    it("returns error when no input provided", () => {
      const result = analyzeStateMachine({});
      expect(result.valid).toBe(false);
      expect(result.issues).toHaveLength(1);
      expect(result.issues[0].severity).toBe("error");
    });

    it("throws on non-existent file path", () => {
      expect(() => analyzeStateMachine({ filePath: "/nonexistent/path.ts" })).toThrow();
    });
  });

  describe("TypeScript patterns (Task 2.2)", () => {
    it("detects states in string literals", () => {
      const code = `const state = "Authorized"; const other = "Captured";`;
      const result = analyzeStateMachine({ code });
      expect(result.mappedStates).toContain("Authorized");
      expect(result.mappedStates).toContain("Captured");
    });

    it("detects states in switch/case patterns", () => {
      const code = `
        switch (event.type) {
          case "charge.succeeded": return "Captured";
          case "auth.succeeded": return "Authorized";
        }
      `;
      const result = analyzeStateMachine({ code });
      expect(result.mappedStates).toContain("Captured");
      expect(result.mappedStates).toContain("Authorized");
    });

    it("detects states in object map patterns", () => {
      const code = `const STATUS_MAP = { "000": "Captured", "100": "Failed" };`;
      const result = analyzeStateMachine({ code });
      expect(result.mappedStates).toContain("Captured");
      expect(result.mappedStates).toContain("Failed");
    });
  });

  describe("Rust patterns (Task 2.3)", () => {
    it("detects PaymentState:: enum variants", () => {
      const code = `
        match event.state {
          PaymentState::Created => {},
          PaymentState::Authorized => {},
          PaymentState::Captured => {},
          PaymentState::Refunded => {},
          PaymentState::Voided => {},
          PaymentState::Failed => {},
          PaymentState::TimedOut => {},
          PaymentState::Pending3ds => {},
        }
      `;
      const result = analyzeStateMachine({ code });
      expect(result.mappedStates).toContain("Created");
      expect(result.mappedStates).toContain("Authorized");
      expect(result.mappedStates).toContain("Expired"); // TimedOut maps to Expired
      expect(result.mappedStates).toContain("Pending3ds");
    });
  });
});

describe("formatValidationOutput", () => {
  it("shows PASSED for valid result (AC #6)", () => {
    const result = analyzeStateMachine({ code: VALID_ADAPTER });
    const output = formatValidationOutput(result);
    expect(output).toContain("**State Machine Validation: PASSED**");
    expect(output).toContain("| Check | Result |");
  });

  it("shows FAILED with error count for invalid result", () => {
    const result = analyzeStateMachine({ code: MISSING_STATES_ADAPTER });
    const output = formatValidationOutput(result);
    expect(output).toContain("**State Machine Validation: FAILED**");
    expect(output).toContain("error");
  });

  it("includes summary table", () => {
    const result = analyzeStateMachine({ code: VALID_ADAPTER });
    const output = formatValidationOutput(result);
    expect(output).toContain("States covered");
    expect(output).toContain("Valid transitions");
    expect(output).toContain("Self-transition handling");
  });

  it("truncates issues list beyond 8", () => {
    // Minimal code to trigger many issues
    const code = `const x = 1;`;
    const result = analyzeStateMachine({ code });
    const output = formatValidationOutput(result);
    // Should have "...and N more" if > 8 issues
    if (result.issues.length > 8) {
      expect(output).toContain("more issue");
    }
  });
});
