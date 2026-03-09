import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { registerQueryProviderPack } from "../query-provider-pack.js";
import { clearPackCache } from "../../knowledge/loader.js";
import type { PayRailConfig } from "../../config/schema.js";
import type { CompiledPack, CompilationMeta } from "../../knowledge/compiler.js";

const TEST_DIR = join(import.meta.dirname, "__fixtures__", "knowledge-packs");

function makeConfig(overrides?: Partial<PayRailConfig>): PayRailConfig {
  return {
    confidence: { generate: 0.9, verify_min: 0.7 },
    token_budget: 14000,
    knowledge_packs_path: TEST_DIR,
    ...overrides,
  };
}

function makeCompiledPack(provider: string): CompiledPack {
  return {
    version: `${provider}@2026-03-01`,
    metadata: {
      name: provider,
      display_name: "Test Provider",
      version: "1.0.0",
      base_url: "https://api.test.com/v1",
      sandbox_url: "https://sandbox.test.com/v1",
      documentation_url: "https://docs.test.com",
    },
    facts: [
      {
        category: "endpoints",
        value: {
          url: "/charges",
          method: "POST",
          parameters: ["amount", "currency"],
          response_schema: '{"id": "string"}',
          description: "Create a charge",
        },
        confidence_score: 0.95,
        source: "sandbox_test",
      },
      {
        category: "webhooks",
        value: {
          event_name: "charge.succeeded",
          payload_schema: '{"id": "string"}',
          trigger_conditions: "When charge succeeds",
          description: "Charge succeeded webhook",
        },
        confidence_score: 0.8,
        source: "official_docs",
        verify_marker:
          "// VERIFY: Charge succeeded webhook (confidence: 0.8, source: official_docs, check: Verify this webhooks fact against provider documentation or sandbox)",
      },
      {
        category: "status_codes",
        value: {
          provider_code: "000.000.000",
          canonical_state: "captured",
          description: "Transaction approved",
        },
        confidence_score: 0.92,
        source: "sandbox_test",
      },
      {
        category: "errors",
        value: {
          code: "800.100.100",
          description: "Invalid card number",
          recovery_action: "Check card number format",
        },
        confidence_score: 0.75,
        source: "official_docs",
        verify_marker:
          "// VERIFY: Error 800.100.100 (confidence: 0.75, source: official_docs, check: Verify this errors fact against provider documentation or sandbox)",
      },
      {
        category: "flows",
        value: {
          name: "standard-charge",
          steps: ["create-charge", "3ds-redirect", "capture"],
          description: "Standard charge flow",
        },
        confidence_score: 0.88,
        source: "official_docs",
        verify_marker:
          "// VERIFY: Flow standard-charge (confidence: 0.88, source: official_docs, check: Verify this flows fact against provider documentation or sandbox)",
      },
    ],
  };
}

function makeMeta(provider: string): CompilationMeta {
  return {
    version: `${provider}@2026-03-01`,
    token_count: 800,
    coverage_pct: 85,
    confidence_summary: { generate: 2, verify: 3, refuse_excluded: 0 },
    compiled_at: "2026-03-01T00:00:00.000Z",
  };
}

function writePackFixture(provider: string): void {
  const dir = join(TEST_DIR, provider, "compiled");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "pack.json"), JSON.stringify(makeCompiledPack(provider)));
  writeFileSync(join(dir, "meta.json"), JSON.stringify(makeMeta(provider)));
}

async function createTestSetup(config: PayRailConfig) {
  const server = new McpServer({ name: "test", version: "0.1.0" });
  registerQueryProviderPack(server, config);
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: "test-client", version: "1.0.0" });
  await Promise.all([server.server.connect(serverTransport), client.connect(clientTransport)]);
  return { server, client };
}

async function callQuery(client: Client, provider: string, queryType: string) {
  return client.callTool({
    name: "query_provider_pack",
    arguments: { provider, query_type: queryType },
  });
}

function getText(result: Awaited<ReturnType<typeof callQuery>>): string {
  return (result.content as Array<{ type: string; text: string }>)[0].text;
}

describe("query_provider_pack tool", () => {
  beforeEach(() => {
    clearPackCache();
    rmSync(TEST_DIR, { recursive: true, force: true });
  });

  afterEach(() => {
    clearPackCache();
    rmSync(TEST_DIR, { recursive: true, force: true });
  });

  describe("overview query (AC #2)", () => {
    it("returns provider summary with key fields", async () => {
      writePackFixture("test-provider");
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "test-provider", "overview");

      expect(result.isError).toBeFalsy();
      const text = getText(result);
      expect(text).toContain("Test Provider");
      expect(text).toContain("85"); // coverage %
      expect(text).toContain("test-provider@2026-03-01"); // version
      await client.close();
      await server.close();
    });

    it("includes confidence distribution", async () => {
      writePackFixture("test-provider");
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "test-provider", "overview");

      const text = getText(result);
      expect(text).toContain("Generate");
      expect(text).toContain("Verify");
      await client.close();
      await server.close();
    });
  });

  describe("detail queries (AC #3)", () => {
    it("returns endpoint details with confidence scores", async () => {
      writePackFixture("test-provider");
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "test-provider", "endpoints");

      expect(result.isError).toBeFalsy();
      const text = getText(result);
      expect(text).toContain("/charges");
      expect(text).toContain("0.95");
      await client.close();
      await server.close();
    });

    it("returns webhook details with VERIFY markers", async () => {
      writePackFixture("test-provider");
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "test-provider", "webhooks");

      expect(result.isError).toBeFalsy();
      const text = getText(result);
      expect(text).toContain("charge.succeeded");
      expect(text).toContain("VERIFY");
      await client.close();
      await server.close();
    });

    it("returns status code mappings", async () => {
      writePackFixture("test-provider");
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "test-provider", "status_codes");

      expect(result.isError).toBeFalsy();
      const text = getText(result);
      expect(text).toContain("000.000.000");
      expect(text).toContain("captured");
      await client.close();
      await server.close();
    });

    it("returns error codes with VERIFY markers", async () => {
      writePackFixture("test-provider");
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "test-provider", "error_codes");

      expect(result.isError).toBeFalsy();
      const text = getText(result);
      expect(text).toContain("800.100.100");
      expect(text).toContain("VERIFY");
      await client.close();
      await server.close();
    });

    it("returns payment flows", async () => {
      writePackFixture("test-provider");
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "test-provider", "flows");

      expect(result.isError).toBeFalsy();
      const text = getText(result);
      expect(text).toContain("standard-charge");
      await client.close();
      await server.close();
    });
  });

  describe("missing provider (AC #4)", () => {
    it("returns structured message with two suggested paths", async () => {
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "nonexistent", "overview");

      expect(result.isError).toBeFalsy();
      const text = getText(result);
      expect(text).toContain("No knowledge pack for nonexistent");
      expect(text).toContain("VERIFY");
      expect(text).toContain("knowledge pack");
      await client.close();
      await server.close();
    });
  });

  describe("response format (AC #3, #5)", () => {
    it("uses structured markdown formatting", async () => {
      writePackFixture("test-provider");
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "test-provider", "overview");

      const text = getText(result);
      expect(text).toMatch(/\*\*/);
      await client.close();
      await server.close();
    });
  });

  describe("overview token budget (AC #2)", () => {
    it("overview response fits within ~200 tokens", async () => {
      writePackFixture("test-provider");
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "test-provider", "overview");

      const text = getText(result);
      const tokens = Math.ceil(text.length / 4);
      expect(tokens).toBeLessThanOrEqual(200);
      await client.close();
      await server.close();
    });
  });

  describe("input validation", () => {
    it("rejects provider names with path traversal", async () => {
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "../../../etc", "overview");

      expect(result.isError).toBe(true);
      const text = getText(result);
      expect(text).toContain("INVALID_PROVIDER_NAME");
      await client.close();
      await server.close();
    });

    it("rejects provider names with special characters", async () => {
      const { server, client } = await createTestSetup(makeConfig());
      const result = await callQuery(client, "provider/name", "overview");

      expect(result.isError).toBe(true);
      await client.close();
      await server.close();
    });

    it("accepts valid provider names", async () => {
      const { server, client } = await createTestSetup(makeConfig());
      // nonexistent but valid name format
      const result = await callQuery(client, "valid-provider-name", "overview");

      expect(result.isError).toBeFalsy();
      const text = getText(result);
      expect(text).toContain("No knowledge pack");
      await client.close();
      await server.close();
    });
  });

  describe("performance (AC #5)", () => {
    it("query responds in <2 seconds", async () => {
      writePackFixture("test-provider");
      const { server, client } = await createTestSetup(makeConfig());

      const start = performance.now();
      await callQuery(client, "test-provider", "overview");
      const elapsed = performance.now() - start;

      expect(elapsed).toBeLessThan(2000);
      await client.close();
      await server.close();
    });
  });
});
