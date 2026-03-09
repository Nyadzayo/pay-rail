import { execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname } from "node:path";
import {
  CANONICAL_STATES,
} from "./canonical-states.js";

export interface ConformanceTestResult {
  transition: string;
  passed: boolean;
  expected: string;
  actual: string;
  details?: string;
  sourceHint?: string;
}

export interface ConformanceRunResult {
  provider: string;
  passed: number;
  failed: number;
  total: number;
  skipped: number;
  results: ConformanceTestResult[];
  durationMs: number;
  rawOutput?: string;
  error?: string;
}

export interface ConformanceInput {
  provider: string;
  adapterPath: string;
  testRunner?: "cargo" | "vitest";
}

/**
 * Run conformance tests against a provider adapter.
 * Shells out to cargo test or vitest depending on adapter type.
 */
export async function runConformanceTests(input: ConformanceInput): Promise<ConformanceRunResult> {
  const runner = input.testRunner ?? detectTestRunner(input.adapterPath);
  const start = Date.now();

  try {
    const rawOutput = executeTestCommand(runner, input.provider, input.adapterPath);
    const results = parseTestOutput(rawOutput, runner);
    const durationMs = Date.now() - start;

    const passed = results.filter((r) => r.passed).length;
    const failed = results.filter((r) => !r.passed).length;

    return {
      provider: input.provider,
      passed,
      failed,
      total: results.length,
      skipped: 0,
      results,
      durationMs,
      rawOutput,
    };
  } catch (err) {
    const durationMs = Date.now() - start;
    const errorMessage = err instanceof Error ? err.message : String(err);

    // Try to parse partial output from failed test runs
    const partialOutput = extractOutputFromError(err);
    if (partialOutput) {
      const results = parseTestOutput(partialOutput, runner);
      const passed = results.filter((r) => r.passed).length;
      const failed = results.filter((r) => !r.passed).length;

      return {
        provider: input.provider,
        passed,
        failed,
        total: results.length,
        skipped: 0,
        results,
        durationMs,
        rawOutput: partialOutput,
      };
    }

    return {
      provider: input.provider,
      passed: 0,
      failed: 0,
      total: 0,
      skipped: 0,
      results: [],
      durationMs,
      error: errorMessage,
    };
  }
}

function detectTestRunner(adapterPath: string): "cargo" | "vitest" {
  if (adapterPath.endsWith(".rs") || adapterPath.includes("/crates/")) return "cargo";
  return "vitest";
}

const SAFE_PROVIDER_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$/;

function executeTestCommand(runner: "cargo" | "vitest", provider: string, adapterPath: string): string {
  // Defense-in-depth: validate provider before shell interpolation
  if (!SAFE_PROVIDER_PATTERN.test(provider)) {
    throw new Error(`[CONFORMANCE] Unsafe provider name rejected: "${provider}"`);
  }

  const timeout = 55_000; // 55s to stay within 60s budget

  if (runner === "cargo") {
    const cmd = `cargo test -p payrail-adapters -- conformance --provider ${provider}`;
    return execSync(cmd, {
      timeout,
      encoding: "utf-8",
      cwd: findCargoRoot(adapterPath),
      env: { ...process.env, RUST_LOG: "warn" },
    });
  }

  // vitest runner
  const cmd = `npx vitest run --reporter=verbose conformance`;
  return execSync(cmd, {
    timeout,
    encoding: "utf-8",
    cwd: findPackageRoot(adapterPath),
  });
}

function findCargoRoot(adapterPath: string): string {
  let dir = adapterPath;
  for (let i = 0; i < 10; i++) {
    if (existsSync(`${dir}/Cargo.toml`)) return dir;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return process.cwd();
}

function findPackageRoot(adapterPath: string): string {
  let dir = adapterPath;
  for (let i = 0; i < 10; i++) {
    if (existsSync(`${dir}/package.json`)) return dir;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return process.cwd();
}

function extractOutputFromError(err: unknown): string | null {
  if (err && typeof err === "object" && "stdout" in err) {
    const stdout = (err as { stdout?: string }).stdout;
    if (stdout && typeof stdout === "string") return stdout;
  }
  if (err && typeof err === "object" && "stderr" in err) {
    const stderr = (err as { stderr?: string }).stderr;
    if (stderr && typeof stderr === "string") return stderr;
  }
  return null;
}

/**
 * Parse test output from cargo test or vitest into structured results.
 */
export function parseTestOutput(output: string, runner: "cargo" | "vitest"): ConformanceTestResult[] {
  if (runner === "cargo") return parseCargoOutput(output);
  return parseVitestOutput(output);
}

function parseCargoOutput(output: string): ConformanceTestResult[] {
  const results: ConformanceTestResult[] = [];
  const lines = output.split("\n");

  for (const line of lines) {
    // cargo test output: "test conformance::test_name ... ok" or "... FAILED"
    const testMatch = line.match(/test\s+(?:conformance::)?(\S+)\s+\.\.\.\s+(ok|FAILED)/);
    if (!testMatch) continue;

    const testName = testMatch[1];
    const passed = testMatch[2] === "ok";
    const transition = testNameToTransition(testName);

    results.push({
      transition,
      passed,
      expected: extractExpectedState(transition),
      actual: passed ? extractExpectedState(transition) : "unknown",
      details: passed ? undefined : `Test ${testName} failed`,
      sourceHint: passed ? undefined : "Check adapter state mapping for this transition",
    });
  }

  // Also parse failure details from cargo output
  const failureSection = output.indexOf("failures:");
  if (failureSection >= 0) {
    const failureText = output.slice(failureSection);
    enrichFailureDetails(results, failureText);
  }

  return results;
}

function parseVitestOutput(output: string): ConformanceTestResult[] {
  const results: ConformanceTestResult[] = [];
  const lines = output.split("\n");

  for (const line of lines) {
    // vitest output: "✓ maps provider status to Captured" or "× maps provider status to Failed"
    const passMatch = line.match(/[✓✔]\s+(.+)/);
    const failMatch = line.match(/[×✗✘]\s+(.+)/);

    if (passMatch) {
      const testName = passMatch[1].trim();
      const state = extractStateFromTestName(testName);
      results.push({
        transition: testName,
        passed: true,
        expected: state,
        actual: state,
      });
    } else if (failMatch) {
      const testName = failMatch[1].trim();
      const state = extractStateFromTestName(testName);
      results.push({
        transition: testName,
        passed: false,
        expected: state,
        actual: "unknown",
        details: `Test failed: ${testName}`,
        sourceHint: `Check adapter status mapping for ${state} state`,
      });
    }
  }

  return results;
}

function testNameToTransition(testName: string): string {
  // Convert test_created_to_authorized -> Created -> Authorized
  return testName
    .replace(/^test_/, "")
    .replace(/_to_/g, " -> ")
    .replace(/_self$/g, " (self)")
    .split(/[_\s]+/)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ")
    .replace(/ -> /g, " -> ");
}

function extractExpectedState(transition: string): string {
  const parts = transition.split(" -> ");
  return parts.length > 1 ? parts[parts.length - 1].replace(" (self)", "") : transition;
}

function extractStateFromTestName(name: string): string {
  for (const state of CANONICAL_STATES) {
    if (name.includes(state)) return state;
  }
  return "unknown";
}

function enrichFailureDetails(results: ConformanceTestResult[], failureText: string): void {
  for (const result of results) {
    if (result.passed) continue;
    // Look for assertion details in failure text
    const detailMatch = failureText.match(
      new RegExp(`${result.transition.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}[\\s\\S]*?Expected.*?(?:but got|actual)\\s*(\\w+)`, "i"),
    );
    if (detailMatch) {
      result.actual = detailMatch[1];
    }
  }
}

/**
 * Format conformance results as structured markdown within ~500 tokens.
 */
export function formatConformanceOutput(result: ConformanceRunResult): string {
  const sections: string[] = [];

  if (result.error) {
    sections.push(`**Conformance Error:** ${result.error}`);
    return sections.join("\n");
  }

  // Summary line (AC #4, #5)
  if (result.failed === 0) {
    sections.push(`**${result.passed}/${result.total} passed.** 0 failures.\n`);
  } else {
    sections.push(`**Conformance Results: ${result.passed}/${result.total} passed.** ${result.failed} failure${result.failed !== 1 ? "s" : ""}.\n`);
  }

  // Results table
  sections.push("| Transition | Expected | Actual | Status |");
  sections.push("|-----------|----------|--------|--------|");

  for (const r of result.results) {
    const status = r.passed ? "PASS" : "**FAIL**";
    const actual = r.passed ? r.expected : r.actual;
    sections.push(`| ${escapeCell(r.transition)} | ${escapeCell(r.expected)} | ${escapeCell(actual)} | ${status} |`);
  }
  sections.push("");

  // Failure details (AC #5) — show top 5
  const failures = result.results.filter((r) => !r.passed);
  if (failures.length > 0) {
    sections.push("**Failure Details:**\n");
    const maxFailures = 5;
    const displayed = failures.slice(0, maxFailures);

    for (let i = 0; i < displayed.length; i++) {
      const f = displayed[i];
      sections.push(`${i + 1}. **${f.transition}**: ${f.details ?? `Expected \`${f.expected}\` but got \`${f.actual}\``}`);
      if (f.sourceHint) {
        sections.push(`   Fix: ${f.sourceHint}`);
      }
    }

    if (failures.length > maxFailures) {
      sections.push(`\n...and ${failures.length - maxFailures} more failure(s).`);
    }
    sections.push("");
  }

  // Next step suggestion (AC #4)
  if (result.failed === 0) {
    sections.push("**Next steps:** Deploy to staging, resolve any remaining VERIFY markers.");
  } else {
    sections.push(`**Next step:** Fix the ${result.failed} failure${result.failed !== 1 ? "s" : ""} and run \`run_conformance\` again.`);
  }

  return sections.join("\n");
}

function escapeCell(text: string): string {
  return String(text ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
}
