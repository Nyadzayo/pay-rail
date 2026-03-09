import { describe, expect, it } from "vitest";
import {
  ingestDocumentation,
  formatIngestReport,
  adjustFactConfidence,
  type IngestResult,
} from "../ingestion.js";
import type { KnowledgePack } from "../schema.js";

const SAMPLE_DOCS = `
# Provider API

## Endpoints

### Create Payment
POST /v1/payments
Parameters:
- \`amount\`: Payment amount
- \`currency\`: Currency code

### Get Payment
GET /v1/payments/{id}

## Webhook Events

### charge.succeeded
Fired when a charge succeeds.

### charge.failed
Triggered when a charge fails.

## Status Codes

| Code | Description |
|------|-------------|
| 000.100.110 | Request successfully processed |
| 800.100.151 | Card expired |

## Error Codes

| Code | Description |
|------|-------------|
| 800.100.151 | Card expired |
| 800.100.152 | Insufficient funds on card |
`;

describe("ingestDocumentation", () => {
  it("extracts facts from documentation", () => {
    const result = ingestDocumentation(SAMPLE_DOCS, "official_docs");
    expect(result.totalFacts).toBeGreaterThan(0);
    expect(result.factsPerCategory.endpoints).toBeGreaterThanOrEqual(2);
    expect(result.factsPerCategory.webhooks).toBeGreaterThanOrEqual(2);
    expect(result.factsPerCategory.status_codes).toBeGreaterThanOrEqual(2);
    expect(result.factsPerCategory.errors).toBeGreaterThanOrEqual(2);
  });

  it("detects gaps for empty categories", () => {
    const result = ingestDocumentation(SAMPLE_DOCS, "official_docs");
    // flows category should be a gap since no flow data in sample
    const flowGap = result.gaps.find((g) => g.category === "flows");
    expect(flowGap).toBeDefined();
    expect(flowGap?.suggestion).toBeTruthy();
  });

  it("reports average confidence matching source type", () => {
    const result = ingestDocumentation(SAMPLE_DOCS, "official_docs");
    // All facts come from official_docs (0.85), so average should be 0.85
    expect(result.averageConfidence).toBeCloseTo(0.85, 2);
  });

  it("reports all new facts on first ingestion", () => {
    const result = ingestDocumentation(SAMPLE_DOCS, "official_docs");
    expect(result.newFacts).toBe(result.totalFacts);
    expect(result.updatedFacts).toBe(0);
    expect(result.unchangedFacts).toBe(0);
  });

  it("returns empty results for empty text", () => {
    const result = ingestDocumentation("", "official_docs");
    expect(result.totalFacts).toBe(0);
    expect(result.gaps.length).toBe(5); // all categories are gaps
  });
});

describe("merge logic", () => {
  function makeExistingPack(): KnowledgePack {
    return {
      metadata: {
        name: "test",
        display_name: "Test",
        version: "1.0",
        base_url: "",
        sandbox_url: "",
        documentation_url: "",
      },
      endpoints: [
        {
          value: {
            url: "/v1/payments",
            method: "POST",
            parameters: ["amount"],
            response_schema: "",
            description: "Old description",
          },
          confidence_score: 0.65,
          source: "community_report",
          verification_date: "2026-01-01T00:00:00.000Z",
          decay_rate: 0.1,
        },
      ],
      webhooks: [],
      status_codes: [],
      errors: [],
      flows: [],
    };
  }

  it("merges new facts with existing pack", () => {
    const existing = makeExistingPack();
    const result = ingestDocumentation(
      SAMPLE_DOCS,
      "official_docs",
      existing,
    );
    // POST /v1/payments already exists, GET /v1/payments is new
    expect(result.factsPerCategory.endpoints).toBeGreaterThanOrEqual(2);
  });

  it("does not create duplicates", () => {
    const existing = makeExistingPack();
    const result = ingestDocumentation(
      SAMPLE_DOCS,
      "official_docs",
      existing,
    );
    // Should have some unchanged or updated, not all new
    expect(result.updatedFacts + result.unchangedFacts).toBeGreaterThan(0);
  });

  it("upgrades confidence when higher source provides same fact", () => {
    const existing = makeExistingPack();
    // The existing POST /v1/payments has community_report (0.65)
    // Ingesting with official_docs (0.85) should upgrade it
    const result = ingestDocumentation(
      SAMPLE_DOCS,
      "official_docs",
      existing,
    );
    expect(result.updatedFacts).toBeGreaterThanOrEqual(1);
  });

  it("does not downgrade confidence", () => {
    const existing = makeExistingPack();
    // Set existing to high confidence
    existing.endpoints[0].confidence_score = 0.95;
    existing.endpoints[0].source = "sandbox_test";
    // Ingesting with community_report (0.65) should not downgrade
    const result = ingestDocumentation(
      SAMPLE_DOCS,
      "community_report",
      existing,
    );
    // The existing endpoint should remain unchanged
    expect(result.unchangedFacts).toBeGreaterThanOrEqual(1);
  });

  it("re-ingestion with same source produces unchanged facts", () => {
    const first = ingestDocumentation(SAMPLE_DOCS, "official_docs");
    // Use the merged pack from first ingestion as existing state
    const second = ingestDocumentation(
      SAMPLE_DOCS,
      "official_docs",
      first.pack,
    );
    // All facts should be unchanged on re-ingestion
    expect(second.unchangedFacts).toBe(second.totalFacts);
    expect(second.newFacts).toBe(0);
    expect(second.updatedFacts).toBe(0);
  });
});

describe("gap detection", () => {
  it("identifies all empty categories as gaps", () => {
    const result = ingestDocumentation("", "official_docs");
    expect(result.gaps).toHaveLength(5);
    const categories = result.gaps.map((g) => g.category);
    expect(categories).toContain("endpoints");
    expect(categories).toContain("webhooks");
    expect(categories).toContain("status_codes");
    expect(categories).toContain("errors");
    expect(categories).toContain("flows");
  });

  it("provides suggestions for each gap", () => {
    const result = ingestDocumentation("", "official_docs");
    for (const gap of result.gaps) {
      expect(gap.suggestion.length).toBeGreaterThan(10);
    }
  });
});

describe("formatIngestReport", () => {
  it("formats a readable report", () => {
    const result: IngestResult = {
      pack: {
        metadata: {
          name: "test",
          display_name: "Test",
          version: "",
          base_url: "",
          sandbox_url: "",
          documentation_url: "",
        },
        endpoints: [],
        webhooks: [],
        status_codes: [],
        errors: [],
        flows: [],
      },
      factsPerCategory: {
        endpoints: 3,
        webhooks: 2,
        status_codes: 5,
        errors: 4,
        flows: 0,
      },
      totalFacts: 14,
      gaps: [
        {
          category: "flows",
          suggestion: "Add payment flow sequences",
        },
      ],
      averageConfidence: 0.82,
      newFacts: 10,
      updatedFacts: 3,
      unchangedFacts: 1,
    };
    const report = formatIngestReport(result);
    expect(report).toContain("Ingestion Report");
    expect(report).toContain("endpoints: 3");
    expect(report).toContain("Total facts: 14");
    expect(report).toContain("Average confidence: 0.82");
    expect(report).toContain("New facts: 10");
    expect(report).toContain("Gaps detected:");
    expect(report).toContain("[flows]");
  });
});

describe("adjustFactConfidence", () => {
  it("adjusts confidence for a matching fact", () => {
    const facts = [
      { confidence_score: 0.85, value: { code: "A" } },
      { confidence_score: 0.85, value: { code: "B" } },
    ];
    const result = adjustFactConfidence(
      facts,
      (f) => f.value.code,
      "A",
      0.95,
    );
    expect(result[0].confidence_score).toBe(0.95);
    expect(result[1].confidence_score).toBe(0.85);
  });

  it("rejects out-of-range scores", () => {
    const facts = [{ confidence_score: 0.85, value: { code: "A" } }];
    expect(() =>
      adjustFactConfidence(facts, (f) => f.value.code, "A", 1.5),
    ).toThrow("between 0 and 1");
  });

  it("returns facts unchanged when key does not match", () => {
    const facts = [{ confidence_score: 0.85, value: { code: "A" } }];
    const result = adjustFactConfidence(
      facts,
      (f) => f.value.code,
      "Z",
      0.5,
    );
    expect(result[0].confidence_score).toBe(0.85);
  });
});
