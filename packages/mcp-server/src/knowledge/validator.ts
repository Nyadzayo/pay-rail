import type {
  KnowledgePack,
  EndpointFactEntry,
  WebhookEventFactEntry,
} from "./schema.js";

export interface HttpResponse {
  status: number;
  body: unknown;
  headers: Record<string, string>;
}

export interface HttpClient {
  request(
    method: string,
    url: string,
    options?: {
      headers?: Record<string, string>;
      body?: string | Record<string, string>;
      formEncoded?: boolean;
    },
  ): Promise<HttpResponse>;
}

export interface SandboxCredentials {
  apiKey: string;
  entityId: string;
}

export type SkipReason =
  | "requires_specific_data"
  | "requires_setup"
  | "requires_auth"
  | "not_available_in_sandbox";

export interface ValidationProgress {
  current: number;
  total: number;
  endpoint: string;
}

export interface ContradictionDetail {
  documented: string;
  actual: string;
  recommendation: string;
}

export interface UndocumentedBehavior {
  description: string;
  type: "status_code" | "response_field" | "webhook_event";
  value: unknown;
}

export interface EndpointValidationResult {
  endpoint: EndpointFactEntry;
  status: "passed" | "contradiction" | "skipped" | "error";
  skipReason?: SkipReason;
  contradiction?: ContradictionDetail;
  discoveries: UndocumentedBehavior[];
  errorMessage?: string;
}

export interface ConfidenceStats {
  average: number;
  min: number;
  max: number;
}

export interface WebhookValidationResult {
  event: WebhookEventFactEntry;
  status: "skipped";
  skipReason: "not_directly_testable";
}

export interface ValidationReport {
  coveragePercent: number;
  totalEndpoints: number;
  testedEndpoints: number;
  skippedEndpoints: number;
  contradictions: number;
  undocumentedBehaviors: number;
  confidenceStats: ConfidenceStats;
  details: EndpointValidationResult[];
  webhookResults: WebhookValidationResult[];
  updatedPack: KnowledgePack;
}

export const SANDBOX_DOMAINS: Record<string, string[]> = {
  "peach-payments": [
    "testsecure.peachpayments.com",
    "eu-test.oppwa.com",
  ],
};

const CREDENTIAL_ENV_VARS: Record<string, { apiKey: string; entityId: string }> = {
  "peach-payments": {
    apiKey: "PEACH_SANDBOX_API_KEY",
    entityId: "PEACH_SANDBOX_ENTITY_ID",
  },
};

// URL patterns that require a prior resource to exist (e.g., refund needs a payment ID)
const REQUIRES_PRIOR_RESOURCE = [
  /\{id\}\/refund/,
  /\{id\}\/void/,
  /\{id\}\/capture/,
  /\{id\}\/cancel/,
  /\{[^}]+\}\/\{[^}]+\}/,  // nested path params (e.g., /payments/{id}/items/{itemId})
];

// URL patterns that can be tested with synthetic data
const TESTABLE_WITH_SYNTHETICS = [
  /^\/v\d+\/payments$/,  // POST /v1/payments (create)
];

// Provider-specific test data for POST requests
const PROVIDER_TEST_DATA: Record<string, Record<string, string>> = {
  "peach-payments": {
    amount: "1.00",
    currency: "ZAR",
    paymentType: "PA",
  },
};

export class SandboxValidator {
  private pack: KnowledgePack;
  private provider: string;
  private credentials: SandboxCredentials;
  private client: HttpClient;
  private onProgress?: (progress: ValidationProgress) => void;
  private sandboxBaseUrl: string;

  constructor(
    pack: KnowledgePack,
    provider: string,
    credentials: SandboxCredentials,
    client: HttpClient,
    onProgress?: (progress: ValidationProgress) => void,
  ) {
    if (!SANDBOX_DOMAINS[provider]) {
      throw new Error(
        `[SANDBOX_UNKNOWN_PROVIDER] Unknown provider '${provider}' [Supported: ${Object.keys(SANDBOX_DOMAINS).join(", ")}]`,
      );
    }

    const sandboxUrl = pack.metadata.sandbox_url;
    if (!sandboxUrl) {
      throw new Error(
        `[SANDBOX_NO_URL] Knowledge pack has no sandbox_url in metadata [Set metadata.sandbox_url to the provider's sandbox base URL]`,
      );
    }

    const hostname = extractHostname(sandboxUrl);
    const allowedDomains = SANDBOX_DOMAINS[provider];
    if (!allowedDomains.includes(hostname)) {
      throw new Error(
        `[SANDBOX_GUARD] '${hostname}' is not a known sandbox domain for ${provider} [Allowed: ${allowedDomains.join(", ")}]`,
      );
    }

    this.pack = structuredClone(pack);
    this.provider = provider;
    this.credentials = credentials;
    this.client = client;
    this.onProgress = onProgress;
    this.sandboxBaseUrl = sandboxUrl.replace(/\/$/, "");
  }

  static loadCredentials(provider: string): SandboxCredentials {
    const envVars = CREDENTIAL_ENV_VARS[provider];
    if (!envVars) {
      throw new Error(
        `[SANDBOX_UNKNOWN_PROVIDER] Unknown provider '${provider}' for credential loading`,
      );
    }

    const apiKey = process.env[envVars.apiKey];
    if (!apiKey) {
      throw new Error(
        `[SANDBOX_MISSING_CRED] Missing environment variable ${envVars.apiKey} [Set ${envVars.apiKey} to your sandbox API key]`,
      );
    }

    const entityId = process.env[envVars.entityId];
    if (!entityId) {
      throw new Error(
        `[SANDBOX_MISSING_CRED] Missing environment variable ${envVars.entityId} [Set ${envVars.entityId} to your sandbox entity ID]`,
      );
    }

    return { apiKey, entityId };
  }

  async validate(): Promise<ValidationReport> {
    const endpoints = this.pack.endpoints;
    const details: EndpointValidationResult[] = [];
    let testedCount = 0;
    let skippedCount = 0;
    let contradictionCount = 0;
    let discoveryCount = 0;

    for (let i = 0; i < endpoints.length; i++) {
      const ep = endpoints[i];

      this.onProgress?.({
        current: i + 1,
        total: endpoints.length,
        endpoint: `${ep.value.method} ${ep.value.url}`,
      });

      const skipReason = this.checkSkipReason(ep);
      if (skipReason) {
        details.push({
          endpoint: ep,
          status: "skipped",
          skipReason,
          discoveries: [],
        });
        skippedCount++;
        continue;
      }

      try {
        const result = await this.testEndpoint(ep);
        details.push(result);

        if (result.status === "contradiction") {
          contradictionCount++;
        }
        discoveryCount += result.discoveries.length;
        testedCount++;
      } catch (err: unknown) {
        details.push({
          endpoint: ep,
          status: "error",
          discoveries: [],
          errorMessage: err instanceof Error ? err.message : String(err),
        });
        testedCount++;
      }
    }

    // Webhook validation: webhooks are async events and can't be directly
    // tested via synchronous API calls. Report each as skipped.
    const webhookResults: WebhookValidationResult[] = this.pack.webhooks.map(
      (wh) => ({
        event: wh,
        status: "skipped" as const,
        skipReason: "not_directly_testable" as const,
      }),
    );

    // Add discoveries to the pack
    for (const detail of details) {
      for (const discovery of detail.discoveries) {
        this.addDiscoveryToPack(discovery);
      }
    }

    // Lower confidence for contradicted facts
    for (const detail of details) {
      if (detail.status === "contradiction") {
        this.lowerConfidenceForContradiction(detail.endpoint);
      }
    }

    const testable = endpoints.length - skippedCount;
    const coveragePercent = testable > 0 ? (testedCount / testable) * 100 : 0;

    return {
      coveragePercent,
      totalEndpoints: endpoints.length,
      testedEndpoints: testedCount,
      skippedEndpoints: skippedCount,
      contradictions: contradictionCount,
      undocumentedBehaviors: discoveryCount,
      confidenceStats: this.calculateConfidenceStats(),
      details,
      webhookResults,
      updatedPack: this.pack,
    };
  }

  private checkSkipReason(ep: EndpointFactEntry): SkipReason | null {
    const url = ep.value.url;

    // Endpoints that require a prior resource (refund, void, capture)
    for (const pattern of REQUIRES_PRIOR_RESOURCE) {
      if (pattern.test(url)) {
        return "requires_specific_data";
      }
    }

    // GET with path params that aren't testable without a real resource
    if (
      ep.value.method === "GET" &&
      url.includes("{") &&
      !TESTABLE_WITH_SYNTHETICS.some((p) => p.test(url))
    ) {
      return "requires_specific_data";
    }

    return null;
  }

  private async testEndpoint(
    ep: EndpointFactEntry,
  ): Promise<EndpointValidationResult> {
    const url = `${this.sandboxBaseUrl}${ep.value.url}`;
    const discoveries: UndocumentedBehavior[] = [];

    const headers: Record<string, string> = {
      Authorization: `Bearer ${this.credentials.apiKey}`,
    };

    const options: Parameters<HttpClient["request"]>[2] = { headers };

    // For POST endpoints, send provider-specific test data
    if (ep.value.method === "POST") {
      const providerData = PROVIDER_TEST_DATA[this.provider] ?? {};
      options.formEncoded = true;
      options.body = {
        entityId: this.credentials.entityId,
        ...providerData,
      };
    }

    const response = await this.client.request(ep.value.method, url, options);

    // Check for undocumented status codes
    const resultCode = extractResultCode(response.body);
    if (resultCode) {
      const knownCodes = this.pack.status_codes.map(
        (sc) => sc.value.provider_code,
      );
      if (!knownCodes.includes(resultCode)) {
        const description = extractResultDescription(response.body);
        discoveries.push({
          description: `Undocumented status code: ${resultCode} — ${description}`,
          type: "status_code",
          value: { code: resultCode, description },
        });
      }
    }

    // Check for contradictions using word-overlap comparison
    let contradiction: ContradictionDetail | undefined;
    if (resultCode) {
      const documented = this.pack.status_codes.find(
        (sc) => sc.value.provider_code === resultCode,
      );
      if (documented) {
        const actualDescription = extractResultDescription(response.body);
        if (
          actualDescription &&
          documented.value.description &&
          !descriptionsMatch(documented.value.description, actualDescription)
        ) {
          contradiction = {
            documented: documented.value.description,
            actual: actualDescription,
            recommendation: "Update status code description to match sandbox behavior",
          };
        }
      }
    }

    // Check for undocumented response fields (compare against known response fields,
    // not request parameters which are inputs)
    if (response.body && typeof response.body === "object") {
      const responseFields = getTopLevelKeys(response.body);
      for (const field of responseFields) {
        if (!isCommonResponseField(field)) {
          discoveries.push({
            description: `Undocumented response field: '${field}'`,
            type: "response_field",
            value: { field },
          });
        }
      }
    }

    return {
      endpoint: ep,
      status: contradiction ? "contradiction" : "passed",
      contradiction,
      discoveries,
    };
  }

  private addDiscoveryToPack(discovery: UndocumentedBehavior): void {
    if (discovery.type === "status_code") {
      const { code, description } = discovery.value as {
        code: string;
        description: string;
      };
      // Don't add if already exists
      if (this.pack.status_codes.some((sc) => sc.value.provider_code === code)) {
        return;
      }
      this.pack.status_codes.push({
        value: {
          provider_code: code,
          canonical_state: "Unknown",
          description: description || "Discovered via sandbox validation",
        },
        confidence_score: 0.95,
        source: "sandbox_test",
        verification_date: new Date().toISOString(),
        decay_rate: 0.05,
      });
    }
  }

  private lowerConfidenceForContradiction(ep: EndpointFactEntry): void {
    const idx = this.pack.endpoints.findIndex(
      (e) =>
        e.value.url === ep.value.url && e.value.method === ep.value.method,
    );
    if (idx >= 0) {
      this.pack.endpoints[idx].confidence_score = Math.max(
        0,
        this.pack.endpoints[idx].confidence_score - 0.2,
      );
    }
  }

  private calculateConfidenceStats(): ConfidenceStats {
    const scores: number[] = [
      ...this.pack.endpoints.map((e) => e.confidence_score),
      ...this.pack.webhooks.map((e) => e.confidence_score),
      ...this.pack.status_codes.map((e) => e.confidence_score),
      ...this.pack.errors.map((e) => e.confidence_score),
      ...this.pack.flows.map((e) => e.confidence_score),
    ];

    if (scores.length === 0) {
      return { average: 0, min: 0, max: 0 };
    }

    return {
      average: scores.reduce((a, b) => a + b, 0) / scores.length,
      min: Math.min(...scores),
      max: Math.max(...scores),
    };
  }
}

export function formatValidationReport(report: ValidationReport): string {
  const lines: string[] = [];
  lines.push("=== Sandbox Validation Report ===");
  lines.push("");
  lines.push(`Coverage: ${report.coveragePercent.toFixed(1)}%`);
  lines.push(`  Total endpoints: ${report.totalEndpoints}`);
  lines.push(`  Tested: ${report.testedEndpoints}`);
  lines.push(`  Skipped: ${report.skippedEndpoints}`);
  lines.push("");
  lines.push(`Contradictions: ${report.contradictions}`);
  lines.push(`Undocumented behaviors: ${report.undocumentedBehaviors}`);
  lines.push("");
  lines.push(`Confidence: avg=${report.confidenceStats.average.toFixed(2)}, min=${report.confidenceStats.min.toFixed(2)}, max=${report.confidenceStats.max.toFixed(2)}`);

  if (report.contradictions > 0) {
    lines.push("");
    lines.push("Contradictions:");
    for (const detail of report.details) {
      if (detail.status === "contradiction" && detail.contradiction) {
        lines.push(
          `  ${detail.endpoint.value.method} ${detail.endpoint.value.url}`,
        );
        lines.push(`    Documented: ${detail.contradiction.documented}`);
        lines.push(`    Actual: ${detail.contradiction.actual}`);
        lines.push(`    → ${detail.contradiction.recommendation}`);
      }
    }
  }

  if (report.undocumentedBehaviors > 0) {
    lines.push("");
    lines.push("Discoveries:");
    for (const detail of report.details) {
      for (const disc of detail.discoveries) {
        lines.push(`  ${disc.description} (confidence: 0.95)`);
      }
    }
  }

  if (report.skippedEndpoints > 0) {
    lines.push("");
    lines.push("Skipped endpoints:");
    for (const detail of report.details) {
      if (detail.status === "skipped") {
        lines.push(
          `  ${detail.endpoint.value.method} ${detail.endpoint.value.url} — ${detail.skipReason}`,
        );
      }
    }
  }

  const errorDetails = report.details.filter((d) => d.status === "error");
  if (errorDetails.length > 0) {
    lines.push("");
    lines.push("Errors:");
    for (const detail of errorDetails) {
      lines.push(
        `  ${detail.endpoint.value.method} ${detail.endpoint.value.url} — ${detail.errorMessage ?? "unknown error"}`,
      );
    }
  }

  if (report.webhookResults.length > 0) {
    lines.push("");
    lines.push(`Webhook events: ${report.webhookResults.length} documented (not directly testable — async events)`);
    for (const wh of report.webhookResults) {
      lines.push(`  ${wh.event.value.event_name} — ${wh.skipReason}`);
    }
  }

  return lines.join("\n");
}

function extractHostname(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

function extractResultCode(body: unknown): string | null {
  if (body && typeof body === "object") {
    const obj = body as Record<string, unknown>;
    if (obj.result && typeof obj.result === "object") {
      const result = obj.result as Record<string, unknown>;
      if (typeof result.code === "string") {
        return result.code;
      }
    }
    // Also check top-level code
    if (typeof obj.code === "string") {
      return obj.code;
    }
  }
  return null;
}

function extractResultDescription(body: unknown): string {
  if (body && typeof body === "object") {
    const obj = body as Record<string, unknown>;
    if (obj.result && typeof obj.result === "object") {
      const result = obj.result as Record<string, unknown>;
      if (typeof result.description === "string") {
        return result.description;
      }
    }
    if (typeof obj.description === "string") {
      return obj.description;
    }
  }
  return "";
}

function getTopLevelKeys(body: unknown): string[] {
  if (body && typeof body === "object" && !Array.isArray(body)) {
    return Object.keys(body as Record<string, unknown>);
  }
  return [];
}

const COMMON_RESPONSE_FIELDS = new Set([
  "id",
  "result",
  "status",
  "timestamp",
  "ndc",
  "buildNumber",
  "registrationId",
  "paymentType",
  "paymentBrand",
  "amount",
  "currency",
  "descriptor",
  "merchantTransactionId",
  "resultDetails",
  "card",
  "customer",
  "redirect",
  "risk",
  "threeDSecure",
  "customParameters",
]);

function isCommonResponseField(field: string): boolean {
  return COMMON_RESPONSE_FIELDS.has(field);
}

function descriptionsMatch(documented: string, actual: string): boolean {
  const docWords = new Set(
    documented
      .toLowerCase()
      .split(/\W+/)
      .filter((w) => w.length > 2),
  );
  const actualWords = actual
    .toLowerCase()
    .split(/\W+/)
    .filter((w) => w.length > 2);
  if (docWords.size === 0 || actualWords.length === 0) return true;

  let matchCount = 0;
  for (const word of actualWords) {
    if (docWords.has(word)) matchCount++;
  }
  // At least 40% of documented words must appear in actual
  const overlap = matchCount / docWords.size;
  return overlap >= 0.4;
}
