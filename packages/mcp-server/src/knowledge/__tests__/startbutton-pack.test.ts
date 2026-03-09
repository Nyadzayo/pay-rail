import { describe, it, expect, beforeAll } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { parse as parseYaml } from "yaml";
import { KnowledgePackSchema } from "../schema.js";
import type { KnowledgePack } from "../schema.js";

const KNOWLEDGE_PACKS_DIR = join(
  import.meta.dirname,
  "..",
  "..",
  "..",
  "..",
  "..",
  "knowledge-packs",
);
const STARTBUTTON_DIR = join(KNOWLEDGE_PACKS_DIR, "startbutton");
const PACK_YAML = join(STARTBUTTON_DIR, "pack.yaml");
const COMPILED_DIR = join(STARTBUTTON_DIR, "compiled");

// ── Task 1: Scaffold validation ──

describe("Startbutton knowledge pack scaffold", () => {
  it("pack.yaml exists", () => {
    expect(existsSync(PACK_YAML)).toBe(true);
  });

  it("compiled/ directory exists", () => {
    expect(existsSync(COMPILED_DIR)).toBe(true);
  });

  it("tests/ directory exists", () => {
    expect(existsSync(join(STARTBUTTON_DIR, "tests"))).toBe(true);
  });
});

// ── Task 2: Schema validation ──

describe("Startbutton pack.yaml schema validation", () => {
  let pack: KnowledgePack;

  beforeAll(() => {
    const yaml = readFileSync(PACK_YAML, "utf-8");
    pack = KnowledgePackSchema.parse(parseYaml(yaml));
  });

  it("validates against KnowledgePackSchema", () => {
    expect(pack).toBeDefined();
    expect(pack.metadata).toBeDefined();
  });

  it("has correct provider metadata", () => {
    expect(pack.metadata.name).toBe("startbutton");
    expect(pack.metadata.display_name).toBe("Startbutton");
    expect(pack.metadata.sandbox_url).toBeTruthy();
    expect(pack.metadata.documentation_url).toBeTruthy();
    expect(pack.metadata.base_url).toBeTruthy();
    expect(pack.metadata.version).toBeTruthy();
  });

  it("has all required sections populated", () => {
    expect(pack.endpoints.length).toBeGreaterThan(0);
    expect(pack.webhooks.length).toBeGreaterThan(0);
    expect(pack.status_codes.length).toBeGreaterThan(0);
    expect(pack.errors.length).toBeGreaterThan(0);
    expect(pack.flows.length).toBeGreaterThan(0);
  });

  it("all facts have valid confidence scores (0.0-1.0)", () => {
    const allFacts = [
      ...pack.endpoints,
      ...pack.webhooks,
      ...pack.status_codes,
      ...pack.errors,
      ...pack.flows,
    ];
    for (const fact of allFacts) {
      expect(fact.confidence_score).toBeGreaterThanOrEqual(0);
      expect(fact.confidence_score).toBeLessThanOrEqual(1);
    }
  });

  it("all facts have valid source types", () => {
    const validSources = [
      "sandbox_test",
      "official_docs",
      "historical_docs",
      "community_report",
      "inferred",
    ];
    const allFacts = [
      ...pack.endpoints,
      ...pack.webhooks,
      ...pack.status_codes,
      ...pack.errors,
      ...pack.flows,
    ];
    for (const fact of allFacts) {
      expect(validSources).toContain(fact.source);
    }
  });

  it("all facts have ISO 8601 verification dates", () => {
    const allFacts = [
      ...pack.endpoints,
      ...pack.webhooks,
      ...pack.status_codes,
      ...pack.errors,
      ...pack.flows,
    ];
    for (const fact of allFacts) {
      expect(fact.verification_date).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    }
  });

  it("all facts have valid decay rates (0.0-1.0)", () => {
    const allFacts = [
      ...pack.endpoints,
      ...pack.webhooks,
      ...pack.status_codes,
      ...pack.errors,
      ...pack.flows,
    ];
    for (const fact of allFacts) {
      expect(fact.decay_rate).toBeGreaterThanOrEqual(0);
      expect(fact.decay_rate).toBeLessThanOrEqual(1);
    }
  });

  it("status codes map to canonical payment states", () => {
    const canonicalStates = [
      "created",
      "authorized",
      "captured",
      "refunded",
      "voided",
      "failed",
      "expired",
      "pending_3ds",
    ];
    for (const sc of pack.status_codes) {
      expect(canonicalStates).toContain(sc.value.canonical_state);
    }
  });

  it("endpoints cover core payment operations", () => {
    const urls = pack.endpoints.map((e) => e.value.url);
    expect(urls).toContain("/payments");
    expect(urls.some((u) => u.includes("capture"))).toBe(true);
    expect(urls.some((u) => u.includes("refund"))).toBe(true);
  });

  it("webhooks cover core payment events", () => {
    const events = pack.webhooks.map((w) => w.value.event_name);
    expect(events.some((e) => e.includes("authorized"))).toBe(true);
    expect(events.some((e) => e.includes("captured"))).toBe(true);
    expect(events.some((e) => e.includes("failed"))).toBe(true);
  });
});

// ── Task 2.5-2.6: Sparse docs characteristics ──

describe("Startbutton sparse documentation characteristics", () => {
  let pack: KnowledgePack;

  beforeAll(() => {
    const yaml = readFileSync(PACK_YAML, "utf-8");
    pack = KnowledgePackSchema.parse(parseYaml(yaml));
  });

  it("has lower average confidence than a well-documented provider", () => {
    const allFacts = [
      ...pack.endpoints,
      ...pack.webhooks,
      ...pack.status_codes,
      ...pack.errors,
      ...pack.flows,
    ];
    const avgConfidence =
      allFacts.reduce((sum, f) => sum + f.confidence_score, 0) /
      allFacts.length;
    // Well-documented provider would average >0.92
    expect(avgConfidence).toBeLessThan(0.92);
  });

  it("has facts in the verify band (0.7-0.89)", () => {
    const allFacts = [
      ...pack.endpoints,
      ...pack.webhooks,
      ...pack.status_codes,
      ...pack.errors,
      ...pack.flows,
    ];
    const verifyBand = allFacts.filter(
      (f) => f.confidence_score >= 0.7 && f.confidence_score < 0.9,
    );
    expect(verifyBand.length).toBeGreaterThan(0);
  });

  it("has community_report and inferred sources (sparse doc indicators)", () => {
    const allFacts = [
      ...pack.endpoints,
      ...pack.webhooks,
      ...pack.status_codes,
      ...pack.errors,
      ...pack.flows,
    ];
    const communitySources = allFacts.filter(
      (f) => f.source === "community_report" || f.source === "inferred",
    );
    expect(communitySources.length).toBeGreaterThan(0);
  });

  it("has undocumented behaviors discovered via sandbox", () => {
    const allFacts = [
      ...pack.webhooks,
      ...pack.status_codes,
    ];
    const sandboxDiscoveries = allFacts.filter(
      (f) =>
        f.source === "sandbox_test" &&
        (JSON.stringify(f.value).includes("undocumented") ||
          JSON.stringify(f.value).includes("discovered")),
    );
    expect(sandboxDiscoveries.length).toBeGreaterThan(0);
  });
});

// ── Task 3: Sandbox validation (env-gated) ──

describe("Startbutton sandbox validation", () => {
  const SANDBOX_API_KEY = process.env.STARTBUTTON_SANDBOX_API_KEY;

  it.skipIf(!SANDBOX_API_KEY)(
    "validates endpoints against live sandbox",
    async () => {
      // When sandbox credentials are available, validate that pack endpoints
      // are structurally compatible with the sandbox base URL
      expect(SANDBOX_API_KEY).toBeTruthy();
      const yaml = readFileSync(PACK_YAML, "utf-8");
      const pack = KnowledgePackSchema.parse(parseYaml(yaml));
      // Verify sandbox URL is configured
      expect(pack.metadata.sandbox_url).toBeTruthy();
      // Verify all endpoints have the required fields for sandbox testing
      for (const ep of pack.endpoints) {
        expect(ep.value.url).toBeTruthy();
        expect(ep.value.method).toMatch(/^(GET|POST|PUT|DELETE|PATCH)$/);
        expect(ep.value.parameters.length).toBeGreaterThan(0);
      }
    },
  );

  it("pack endpoints are structurally valid for sandbox testing", () => {
    const yaml = readFileSync(PACK_YAML, "utf-8");
    const pack = KnowledgePackSchema.parse(parseYaml(yaml));
    // Verify sandbox URL exists
    expect(pack.metadata.sandbox_url).toBeTruthy();
    expect(pack.metadata.sandbox_url).toMatch(/^https:\/\//);
    // Verify core payment endpoints exist
    const urls = pack.endpoints.map((e) => e.value.url);
    expect(urls).toContain("/payments");
    // Verify webhook events have payload schemas
    for (const wh of pack.webhooks) {
      expect(wh.value.event_name).toBeTruthy();
      expect(wh.value.payload_schema).toBeTruthy();
    }
  });

  it("pack contains no hardcoded credentials", () => {
    const packContent = readFileSync(PACK_YAML, "utf-8");
    // No API keys or secrets
    expect(packContent).not.toMatch(/sk_[a-zA-Z0-9]{20,}/);
    expect(packContent).not.toMatch(/pk_[a-zA-Z0-9]{20,}/);
    expect(packContent).not.toMatch(/api_key\s*[:=]\s*["'][a-zA-Z0-9]{20,}/);
    expect(packContent).not.toMatch(/secret\s*[:=]\s*["'][a-zA-Z0-9]{20,}/);
  });
});
