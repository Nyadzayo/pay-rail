import { describe, expect, it, beforeAll } from "vitest";
import { execSync } from "node:child_process";
import { writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

describe("CLI ingest integration", () => {
  let tempDir: string;
  let docsPath: string;

  beforeAll(() => {
    tempDir = mkdtempSync(join(tmpdir(), "payrail-cli-test-"));
    docsPath = join(tempDir, "provider-docs.md");
    writeFileSync(
      docsPath,
      `
# Test Provider API

## Endpoints

### Create Payment
POST /v1/payments
Parameters:
- \`amount\`: Payment amount
- \`currency\`: Currency code

## Webhook Events

### charge.succeeded
Fired when a charge is successfully processed.

## Status Codes

| Code | Description |
|------|-------------|
| 000.100.110 | Request successfully processed |

## Error Codes

| Code | Description |
|------|-------------|
| 800.100.151 | Card expired |
`,
    );
  });

  it("runs full ingest-and-report flow", () => {
    const cliScript = join(
      __dirname,
      "..",
      "..",
      "..",
      "dist",
      "cli",
      "ingest.js",
    );
    const output = execSync(
      `node ${cliScript} test-provider --source ${docsPath} --type official`,
      { encoding: "utf-8" },
    );

    expect(output).toContain("Provider: test-provider");
    expect(output).toContain("Ingestion Report");
    expect(output).toContain("endpoints:");
    expect(output).toContain("webhooks:");
    expect(output).toContain("Total facts:");
    expect(output).toContain("Average confidence:");
    expect(output).toContain("New facts:");
  });

  it("reports gaps for missing categories", () => {
    const cliScript = join(
      __dirname,
      "..",
      "..",
      "..",
      "dist",
      "cli",
      "ingest.js",
    );
    const output = execSync(
      `node ${cliScript} test-provider --source ${docsPath} --type official`,
      { encoding: "utf-8" },
    );
    expect(output).toContain("Gaps detected:");
    expect(output).toContain("[flows]");
  });

  it("supports different source types", () => {
    const cliScript = join(
      __dirname,
      "..",
      "..",
      "..",
      "dist",
      "cli",
      "ingest.js",
    );
    const output = execSync(
      `node ${cliScript} test-provider --source ${docsPath} --type community`,
      { encoding: "utf-8" },
    );
    expect(output).toContain("(community)");
    expect(output).toContain("Average confidence: 0.65");
  });

  // Cleanup handled by OS temp dir
});
