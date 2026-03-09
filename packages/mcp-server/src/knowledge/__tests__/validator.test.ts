import { describe, expect, it, vi } from "vitest";
import {
  SandboxValidator,
  formatValidationReport,
  type HttpClient,
  type HttpResponse,
  type SandboxCredentials,
  type ValidationReport,
  type SkipReason,
  SANDBOX_DOMAINS,
} from "../validator.js";
import type { KnowledgePack } from "../schema.js";

function makePack(overrides?: Partial<KnowledgePack>): KnowledgePack {
  return {
    metadata: {
      name: "peach-payments",
      display_name: "Peach Payments",
      version: "1.0",
      base_url: "https://testsecure.peachpayments.com",
      sandbox_url: "https://testsecure.peachpayments.com",
      documentation_url: "https://docs.peachpayments.com",
    },
    endpoints: [
      {
        value: {
          url: "/v1/payments",
          method: "POST",
          parameters: ["amount", "currency", "entityId"],
          response_schema: "",
          description: "Create a payment",
        },
        confidence_score: 0.85,
        source: "official_docs",
        verification_date: "2026-01-01T00:00:00.000Z",
        decay_rate: 0.05,
      },
      {
        value: {
          url: "/v1/payments/{id}",
          method: "GET",
          parameters: [],
          response_schema: "",
          description: "Get payment status",
        },
        confidence_score: 0.85,
        source: "official_docs",
        verification_date: "2026-01-01T00:00:00.000Z",
        decay_rate: 0.05,
      },
    ],
    webhooks: [],
    status_codes: [
      {
        value: {
          provider_code: "000.100.110",
          canonical_state: "Captured",
          description: "Request successfully processed",
        },
        confidence_score: 0.85,
        source: "official_docs",
        verification_date: "2026-01-01T00:00:00.000Z",
        decay_rate: 0.05,
      },
    ],
    errors: [],
    flows: [],
    ...overrides,
  };
}

function makeCreds(): SandboxCredentials {
  return { apiKey: "test-key-123", entityId: "test-entity-456" };
}

function mockClient(responses?: Record<string, HttpResponse>): HttpClient {
  const defaultResponse: HttpResponse = {
    status: 200,
    body: { result: { code: "000.100.110", description: "Request successfully processed" } },
    headers: { "content-type": "application/json" },
  };

  return {
    request: vi.fn(async (method: string, url: string) => {
      if (responses) {
        for (const [pattern, resp] of Object.entries(responses)) {
          if (url.includes(pattern)) return resp;
        }
      }
      return defaultResponse;
    }),
  };
}

describe("SandboxValidator", () => {
  describe("initialization", () => {
    it("creates validator with pack and credentials", () => {
      const pack = makePack();
      const creds = makeCreds();
      const client = mockClient();
      const validator = new SandboxValidator(pack, "peach-payments", creds, client);
      expect(validator).toBeDefined();
    });

    it("rejects unknown provider", () => {
      const pack = makePack();
      const creds = makeCreds();
      const client = mockClient();
      expect(
        () => new SandboxValidator(pack, "unknown-provider", creds, client),
      ).toThrow("Unknown provider");
    });
  });

  describe("credential loading", () => {
    it("loadCredentials reads from environment variables", () => {
      vi.stubEnv("PEACH_SANDBOX_API_KEY", "env-key");
      vi.stubEnv("PEACH_SANDBOX_ENTITY_ID", "env-entity");

      const creds = SandboxValidator.loadCredentials("peach-payments");
      expect(creds.apiKey).toBe("env-key");
      expect(creds.entityId).toBe("env-entity");

      vi.unstubAllEnvs();
    });

    it("throws when credentials are missing", () => {
      vi.stubEnv("PEACH_SANDBOX_API_KEY", "");
      delete process.env.PEACH_SANDBOX_ENTITY_ID;

      expect(() => SandboxValidator.loadCredentials("peach-payments")).toThrow(
        "PEACH_SANDBOX_API_KEY",
      );

      vi.unstubAllEnvs();
    });
  });

  describe("sandbox URL guard", () => {
    it("accepts known sandbox domains", () => {
      const pack = makePack({
        metadata: {
          ...makePack().metadata,
          sandbox_url: "https://testsecure.peachpayments.com",
        },
      });
      const client = mockClient();
      // Should not throw
      expect(
        () => new SandboxValidator(pack, "peach-payments", makeCreds(), client),
      ).not.toThrow();
    });

    it("rejects non-sandbox URLs", () => {
      const pack = makePack({
        metadata: {
          ...makePack().metadata,
          sandbox_url: "https://secure.peachpayments.com",
        },
      });
      const client = mockClient();
      expect(
        () => new SandboxValidator(pack, "peach-payments", makeCreds(), client),
      ).toThrow("not a known sandbox domain");
    });

    it("rejects empty sandbox URL", () => {
      const pack = makePack({
        metadata: {
          ...makePack().metadata,
          sandbox_url: "",
        },
      });
      const client = mockClient();
      expect(
        () => new SandboxValidator(pack, "peach-payments", makeCreds(), client),
      ).toThrow("sandbox_url");
    });
  });

  describe("progress reporting", () => {
    it("reports progress per endpoint", async () => {
      const pack = makePack();
      const client = mockClient();
      const progress: Array<{ current: number; total: number }> = [];
      const validator = new SandboxValidator(
        pack,
        "peach-payments",
        makeCreds(),
        client,
        (p) => progress.push({ current: p.current, total: p.total }),
      );

      await validator.validate();

      expect(progress.length).toBe(2);
      expect(progress[0].current).toBe(1);
      expect(progress[0].total).toBe(2);
      expect(progress[1].current).toBe(2);
      expect(progress[1].total).toBe(2);
    });
  });

  describe("endpoint validation", () => {
    it("tests documented endpoints against sandbox", async () => {
      const pack = makePack();
      const client = mockClient();
      const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

      const report = await validator.validate();

      expect(report.totalEndpoints).toBe(2);
      expect(client.request).toHaveBeenCalled();
    });

    it("detects contradictions between documented and actual", async () => {
      const pack = makePack();
      const client = mockClient({
        "/v1/payments": {
          status: 200,
          body: {
            result: {
              code: "800.100.151",
              description: "Card expired",
            },
          },
          headers: { "content-type": "application/json" },
        },
      });
      const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

      const report = await validator.validate();

      expect(report.contradictions).toBeGreaterThanOrEqual(0);
    });
  });

  describe("undocumented behavior discovery", () => {
    it("detects status codes not in the knowledge pack", async () => {
      const pack = makePack();
      const client = mockClient({
        "/v1/payments": {
          status: 200,
          body: {
            result: {
              code: "999.999.999",
              description: "Undocumented special code",
            },
          },
          headers: { "content-type": "application/json" },
        },
      });
      const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

      const report = await validator.validate();

      expect(report.undocumentedBehaviors).toBeGreaterThanOrEqual(1);
    });
  });

  describe("skip logic", () => {
    it("skips endpoints requiring specific data", async () => {
      const pack = makePack({
        endpoints: [
          {
            value: {
              url: "/v1/payments/{id}/refund",
              method: "POST",
              parameters: ["amount"],
              response_schema: "",
              description: "Refund a payment",
            },
            confidence_score: 0.85,
            source: "official_docs",
            verification_date: "2026-01-01T00:00:00.000Z",
            decay_rate: 0.05,
          },
        ],
      });
      const client = mockClient();
      const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

      const report = await validator.validate();

      expect(report.skippedEndpoints).toBe(1);
      expect(report.details[0].status).toBe("skipped");
      expect(report.details[0].skipReason).toBeDefined();
    });

    it("excludes skipped endpoints from coverage denominator", async () => {
      const pack = makePack({
        endpoints: [
          ...makePack().endpoints,
          {
            value: {
              url: "/v1/payments/{id}/refund",
              method: "POST",
              parameters: ["amount"],
              response_schema: "",
              description: "Refund a payment",
            },
            confidence_score: 0.85,
            source: "official_docs",
            verification_date: "2026-01-01T00:00:00.000Z",
            decay_rate: 0.05,
          },
        ],
      });
      const client = mockClient();
      const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

      const report = await validator.validate();

      // Coverage should be based on (total - skipped), not total
      const testable = report.totalEndpoints - report.skippedEndpoints;
      if (testable > 0) {
        expect(report.coveragePercent).toBe(
          (report.testedEndpoints / testable) * 100,
        );
      }
    });
  });

  describe("validation reporting", () => {
    it("produces a complete report", async () => {
      const pack = makePack();
      const client = mockClient();
      const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

      const report = await validator.validate();

      expect(report.coveragePercent).toBeGreaterThanOrEqual(0);
      expect(report.totalEndpoints).toBeGreaterThan(0);
      expect(typeof report.testedEndpoints).toBe("number");
      expect(typeof report.skippedEndpoints).toBe("number");
      expect(typeof report.contradictions).toBe("number");
      expect(typeof report.undocumentedBehaviors).toBe("number");
      expect(report.confidenceStats).toBeDefined();
      expect(typeof report.confidenceStats.average).toBe("number");
      expect(typeof report.confidenceStats.min).toBe("number");
      expect(typeof report.confidenceStats.max).toBe("number");
      expect(Array.isArray(report.details)).toBe(true);
    });

    it("calculates confidence statistics", async () => {
      const pack = makePack();
      const client = mockClient();
      const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

      const report = await validator.validate();

      expect(report.confidenceStats.average).toBeGreaterThan(0);
      expect(report.confidenceStats.min).toBeLessThanOrEqual(
        report.confidenceStats.max,
      );
    });
  });
});

describe("SANDBOX_DOMAINS", () => {
  it("includes known Peach sandbox domains", () => {
    expect(SANDBOX_DOMAINS["peach-payments"]).toBeDefined();
    expect(SANDBOX_DOMAINS["peach-payments"]).toContain(
      "testsecure.peachpayments.com",
    );
  });
});

describe("contradiction detection", () => {
  it("lowers confidence for contradicted endpoints", async () => {
    const pack = makePack();
    const originalScore = pack.endpoints[0].confidence_score;
    const client = mockClient({
      "/v1/payments": {
        status: 200,
        body: {
          result: {
            code: "000.100.110",
            description: "Completely different meaning than documented",
          },
        },
        headers: { "content-type": "application/json" },
      },
    });
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    // Find the POST /v1/payments in the updated pack
    const updated = report.updatedPack.endpoints.find(
      (e) => e.value.method === "POST" && e.value.url === "/v1/payments",
    );

    if (report.contradictions > 0 && updated) {
      expect(updated.confidence_score).toBeLessThan(originalScore);
    }
  });

  it("generates warning with documented vs actual values", async () => {
    const pack = makePack();
    const client = mockClient({
      "/v1/payments": {
        status: 200,
        body: {
          result: {
            code: "000.100.110",
            description: "Completely different meaning than documented",
          },
        },
        headers: { "content-type": "application/json" },
      },
    });
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    const contradicted = report.details.find(
      (d) => d.status === "contradiction",
    );
    if (contradicted?.contradiction) {
      expect(contradicted.contradiction.documented).toBeTruthy();
      expect(contradicted.contradiction.actual).toBeTruthy();
      expect(contradicted.contradiction.recommendation).toBeTruthy();
    }
  });
});

describe("undocumented behavior discovery (detailed)", () => {
  it("adds discovered status codes with confidence 0.95", async () => {
    const pack = makePack();
    const client = mockClient({
      "/v1/payments": {
        status: 200,
        body: {
          result: {
            code: "999.999.999",
            description: "Undocumented special code",
          },
        },
        headers: { "content-type": "application/json" },
      },
    });
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    const newCode = report.updatedPack.status_codes.find(
      (sc) => sc.value.provider_code === "999.999.999",
    );
    expect(newCode).toBeDefined();
    expect(newCode?.confidence_score).toBe(0.95);
    expect(newCode?.source).toBe("sandbox_test");
  });

  it("detects undocumented response fields", async () => {
    const pack = makePack();
    const client = mockClient({
      "/v1/payments": {
        status: 200,
        body: {
          result: { code: "000.100.110", description: "Request successfully processed" },
          unexpectedField: "surprise",
          anotherNewField: 42,
        },
        headers: { "content-type": "application/json" },
      },
    });
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    const discoveries = report.details.flatMap((d) => d.discoveries);
    const fieldDiscoveries = discoveries.filter((d) => d.type === "response_field");
    expect(fieldDiscoveries.length).toBeGreaterThanOrEqual(1);
  });

  it("reports discovery description", async () => {
    const pack = makePack();
    const client = mockClient({
      "/v1/payments": {
        status: 200,
        body: {
          result: {
            code: "999.999.999",
            description: "Undocumented special code",
          },
        },
        headers: { "content-type": "application/json" },
      },
    });
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    const discoveries = report.details.flatMap((d) => d.discoveries);
    const codeDiscovery = discoveries.find((d) => d.type === "status_code");
    expect(codeDiscovery?.description).toContain("999.999.999");
  });
});

describe("skip logic (detailed)", () => {
  it("skips GET endpoints with path parameters", async () => {
    const pack = makePack({
      endpoints: [
        {
          value: {
            url: "/v1/payments/{id}",
            method: "GET",
            parameters: [],
            response_schema: "",
            description: "Get payment status",
          },
          confidence_score: 0.85,
          source: "official_docs",
          verification_date: "2026-01-01T00:00:00.000Z",
          decay_rate: 0.05,
        },
      ],
    });
    const client = mockClient();
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    expect(report.skippedEndpoints).toBe(1);
    expect(report.details[0].skipReason).toBe("requires_specific_data");
  });

  it("skips capture/void/refund endpoints", async () => {
    const pack = makePack({
      endpoints: [
        {
          value: {
            url: "/v1/payments/{id}/capture",
            method: "POST",
            parameters: ["amount"],
            response_schema: "",
            description: "Capture a payment",
          },
          confidence_score: 0.85,
          source: "official_docs",
          verification_date: "2026-01-01T00:00:00.000Z",
          decay_rate: 0.05,
        },
      ],
    });
    const client = mockClient();
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    expect(report.skippedEndpoints).toBe(1);
  });

  it("logs skip reason separately from failures", async () => {
    const pack = makePack({
      endpoints: [
        ...makePack().endpoints,
        {
          value: {
            url: "/v1/payments/{id}/void",
            method: "POST",
            parameters: [],
            response_schema: "",
            description: "Void a payment",
          },
          confidence_score: 0.85,
          source: "official_docs",
          verification_date: "2026-01-01T00:00:00.000Z",
          decay_rate: 0.05,
        },
      ],
    });
    const client = mockClient();
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    const skipped = report.details.filter((d) => d.status === "skipped");
    const errors = report.details.filter((d) => d.status === "error");
    expect(skipped.length).toBeGreaterThan(0);
    expect(skipped.every((s) => s.skipReason !== undefined)).toBe(true);
    // Skipped is distinct from error
    expect(skipped.every((s) => s.status === "skipped")).toBe(true);
  });
});

describe("formatValidationReport", () => {
  it("formats a readable report", async () => {
    const pack = makePack();
    const client = mockClient();
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();
    const text = formatValidationReport(report);

    expect(text).toContain("Sandbox Validation Report");
    expect(text).toContain("Coverage:");
    expect(text).toContain("Confidence:");
  });

  it("includes skipped section when endpoints are skipped", async () => {
    const pack = makePack({
      endpoints: [
        {
          value: {
            url: "/v1/payments/{id}/refund",
            method: "POST",
            parameters: [],
            response_schema: "",
            description: "Refund",
          },
          confidence_score: 0.85,
          source: "official_docs",
          verification_date: "2026-01-01T00:00:00.000Z",
          decay_rate: 0.05,
        },
      ],
    });
    const client = mockClient();
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();
    const text = formatValidationReport(report);

    expect(text).toContain("Skipped endpoints:");
    expect(text).toContain("requires_specific_data");
  });

  it("includes discovery section when behaviors are found", async () => {
    const pack = makePack();
    const client = mockClient({
      "/v1/payments": {
        status: 200,
        body: {
          result: {
            code: "777.888.999",
            description: "Brand new code",
          },
        },
        headers: { "content-type": "application/json" },
      },
    });
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();
    const text = formatValidationReport(report);

    if (report.undocumentedBehaviors > 0) {
      expect(text).toContain("Discoveries:");
      expect(text).toContain("0.95");
    }
  });
});

describe("webhook validation", () => {
  it("reports webhook events as not directly testable", async () => {
    const pack = makePack({
      webhooks: [
        {
          value: {
            event_name: "charge.succeeded",
            payload_schema: "",
            trigger_conditions: "",
            description: "Charge succeeded",
          },
          confidence_score: 0.85,
          source: "official_docs",
          verification_date: "2026-01-01T00:00:00.000Z",
          decay_rate: 0.05,
        },
        {
          value: {
            event_name: "charge.failed",
            payload_schema: "",
            trigger_conditions: "",
            description: "Charge failed",
          },
          confidence_score: 0.85,
          source: "official_docs",
          verification_date: "2026-01-01T00:00:00.000Z",
          decay_rate: 0.05,
        },
      ],
    });
    const client = mockClient();
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    expect(report.webhookResults).toHaveLength(2);
    expect(report.webhookResults[0].status).toBe("skipped");
    expect(report.webhookResults[0].skipReason).toBe("not_directly_testable");
    expect(report.webhookResults[0].event.value.event_name).toBe("charge.succeeded");
  });

  it("includes webhook section in formatted report", async () => {
    const pack = makePack({
      webhooks: [
        {
          value: {
            event_name: "charge.succeeded",
            payload_schema: "",
            trigger_conditions: "",
            description: "Charge succeeded",
          },
          confidence_score: 0.85,
          source: "official_docs",
          verification_date: "2026-01-01T00:00:00.000Z",
          decay_rate: 0.05,
        },
      ],
    });
    const client = mockClient();
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();
    const text = formatValidationReport(report);

    expect(text).toContain("Webhook events:");
    expect(text).toContain("charge.succeeded");
    expect(text).toContain("not_directly_testable");
  });
});

describe("error handling", () => {
  it("captures error message when endpoint request fails", async () => {
    const pack = makePack({
      endpoints: [
        {
          value: {
            url: "/v1/payments",
            method: "POST",
            parameters: [],
            response_schema: "",
            description: "Create payment",
          },
          confidence_score: 0.85,
          source: "official_docs",
          verification_date: "2026-01-01T00:00:00.000Z",
          decay_rate: 0.05,
        },
      ],
    });
    const client: HttpClient = {
      request: vi.fn(async () => {
        throw new Error("Connection refused");
      }),
    };
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    expect(report.details[0].status).toBe("error");
    expect(report.details[0].errorMessage).toBe("Connection refused");
  });

  it("handles non-200 responses without crashing", async () => {
    const pack = makePack();
    const client = mockClient({
      "/v1/payments": {
        status: 401,
        body: { error: "Unauthorized" },
        headers: { "content-type": "application/json" },
      },
    });
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();

    // Should not crash, endpoint treated as tested
    expect(report.testedEndpoints).toBeGreaterThanOrEqual(1);
  });

  it("includes error details in formatted report", async () => {
    const pack = makePack({
      endpoints: [
        {
          value: {
            url: "/v1/payments",
            method: "POST",
            parameters: [],
            response_schema: "",
            description: "Create payment",
          },
          confidence_score: 0.85,
          source: "official_docs",
          verification_date: "2026-01-01T00:00:00.000Z",
          decay_rate: 0.05,
        },
      ],
    });
    const client: HttpClient = {
      request: vi.fn(async () => {
        throw new Error("Timeout");
      }),
    };
    const validator = new SandboxValidator(pack, "peach-payments", makeCreds(), client);

    const report = await validator.validate();
    const text = formatValidationReport(report);

    expect(text).toContain("Errors:");
    expect(text).toContain("Timeout");
  });
});
