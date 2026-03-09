import type {
  EndpointFact,
  ErrorCodeFact,
  FactSource,
  PaymentFlowSequence,
  StatusCodeMapping,
  WebhookEventFact,
} from "./schema.js";
import { defaultConfidence, decayRate } from "./confidence.js";

export interface ParsedFact<T> {
  value: T;
  confidence_score: number;
  source: FactSource;
  verification_date: string;
  decay_rate: number;
}

export interface ParseResult {
  endpoints: ParsedFact<EndpointFact>[];
  webhooks: ParsedFact<WebhookEventFact>[];
  status_codes: ParsedFact<StatusCodeMapping>[];
  errors: ParsedFact<ErrorCodeFact>[];
  flows: ParsedFact<PaymentFlowSequence>[];
}

function makeFact<T>(value: T, source: FactSource): ParsedFact<T> {
  return {
    value,
    confidence_score: defaultConfidence(source),
    source,
    verification_date: new Date().toISOString(),
    decay_rate: decayRate(source),
  };
}

const HTTP_METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];

function parseEndpoints(
  text: string,
  source: FactSource,
): ParsedFact<EndpointFact>[] {
  const results: ParsedFact<EndpointFact>[] = [];
  // Match patterns like: POST /v1/payments  or  GET /api/charges/{id}
  // URL stops at whitespace, comma, pipe, or bracket
  const methodUrlPattern = new RegExp(
    `(?:^|\\s)(${HTTP_METHODS.join("|")})\\s+(\/[^\\s,|\\[\\]]+)`,
    "gm",
  );
  let match: RegExpExecArray | null;
  while ((match = methodUrlPattern.exec(text)) !== null) {
    const method = match[1];
    const url = match[2];
    // Look for parameters mentioned near this endpoint (next ~200 chars)
    const contextStart = match.index + match[0].length;
    const context = text.slice(contextStart, contextStart + 500);
    const parameters = extractParameters(context);
    const description = extractNearbyDescription(text, match.index);
    const responseSchema = extractResponseSchema(context);
    results.push(
      makeFact(
        {
          url,
          method,
          parameters,
          response_schema: responseSchema,
          description,
        },
        source,
      ),
    );
  }
  return deduplicateEndpoints(results);
}

function extractParameters(context: string): string[] {
  const params: Set<string> = new Set();
  // Match parameter names from various documentation patterns
  // Pattern: `parameter_name` or **parameter_name** or - parameter_name:
  const patterns = [
    /[`](\w+)[`]/g,
    /\*\*(\w+)\*\*/g,
    /^[\s-]*(\w+)\s*[:|-]/gm,
  ];
  for (const pattern of patterns) {
    let m: RegExpExecArray | null;
    while ((m = pattern.exec(context)) !== null) {
      const name = m[1];
      if (isLikelyParameter(name)) {
        params.add(name);
      }
    }
  }
  return [...params];
}

const COMMON_NON_PARAMS = new Set([
  "the",
  "and",
  "or",
  "not",
  "for",
  "with",
  "this",
  "that",
  "from",
  "note",
  "example",
  "required",
  "optional",
  "description",
  "type",
  "string",
  "number",
  "boolean",
  "object",
  "array",
  "null",
  "true",
  "false",
  "returns",
  "response",
  "request",
  "header",
  "headers",
  "body",
  "query",
  "path",
]);

function isLikelyParameter(name: string): boolean {
  if (name.length < 2 || name.length > 40) return false;
  if (COMMON_NON_PARAMS.has(name.toLowerCase())) return false;
  // Parameters typically contain lowercase, underscores, or camelCase
  return /^[a-z][a-zA-Z0-9_]*$/.test(name);
}

function extractNearbyDescription(text: string, index: number): string {
  // Look for a preceding heading or sentence
  const before = text.slice(Math.max(0, index - 200), index);
  const lines = before.split("\n").filter((l) => l.trim().length > 0);
  const lastLine = lines[lines.length - 1]?.trim() ?? "";
  // If it's a heading, use it
  if (lastLine.startsWith("#")) {
    return lastLine.replace(/^#+\s*/, "");
  }
  return lastLine.length > 10 ? lastLine : "";
}

function extractResponseSchema(context: string): string {
  // Look for response-related keywords
  const responseMatch = context.match(
    /(?:returns?|response)\s*[:]\s*(.{10,100})/i,
  );
  if (responseMatch) {
    return responseMatch[1].trim().replace(/[.\n].*$/, "");
  }
  return "";
}

function deduplicateEndpoints(
  facts: ParsedFact<EndpointFact>[],
): ParsedFact<EndpointFact>[] {
  const seen = new Map<string, ParsedFact<EndpointFact>>();
  for (const fact of facts) {
    const key = `${fact.value.method}:${fact.value.url}`;
    const existing = seen.get(key);
    if (!existing) {
      seen.set(key, fact);
    } else {
      // Merge parameters from both occurrences
      const mergedParams = [
        ...new Set([...existing.value.parameters, ...fact.value.parameters]),
      ];
      const winner =
        fact.confidence_score > existing.confidence_score ? fact : existing;
      winner.value.parameters = mergedParams;
      // Keep longer description and response_schema
      if (fact.value.description.length > winner.value.description.length) {
        winner.value.description = fact.value.description;
      }
      if (
        fact.value.response_schema.length > winner.value.response_schema.length
      ) {
        winner.value.response_schema = fact.value.response_schema;
      }
      seen.set(key, winner);
    }
  }
  return [...seen.values()];
}

function parseWebhookEvents(
  text: string,
  source: FactSource,
): ParsedFact<WebhookEventFact>[] {
  const results: ParsedFact<WebhookEventFact>[] = [];
  // Match event names like: charge.succeeded, payment.completed, transaction.created
  // Must have action-like segments (not file extensions or domain names)
  const eventPattern =
    /(?:^|[\s"`*])([a-z]+\.[a-z]+(?:\.[a-z]+)?)(?:[\s"`*,;:]|$)/gm;
  let match: RegExpExecArray | null;
  const seen = new Set<string>();
  const FILE_EXTENSIONS = new Set([
    "md",
    "ts",
    "js",
    "json",
    "yaml",
    "yml",
    "rs",
    "toml",
    "sh",
    "css",
    "html",
    "xml",
    "csv",
    "txt",
    "log",
    "env",
    "lock",
    "config",
  ]);
  while ((match = eventPattern.exec(text)) !== null) {
    const eventName = match[1];
    if (seen.has(eventName)) continue;
    // Reject file extensions, URLs, domain names, and version numbers
    if (eventName.includes("/")) continue;
    const lastSegment = eventName.split(".").pop() ?? "";
    if (FILE_EXTENSIONS.has(lastSegment)) continue;
    // Reject if looks like a domain (segment has 2+ chars that are all alpha, like com, org, net)
    const segments = eventName.split(".");
    if (
      segments.length === 2 &&
      ["com", "org", "net", "io", "co", "dev"].includes(segments[1])
    )
      continue;
    // Reject version-like patterns (e.g., "version.1")
    if (segments.some((s) => /^\d+$/.test(s))) continue;
    seen.add(eventName);
    const contextStart = match.index + match[0].length;
    const context = text.slice(contextStart, contextStart + 300);
    const description = extractNearbyDescription(text, match.index);
    const payloadSchema = extractPayloadInfo(context);
    const triggerConditions = extractTriggerConditions(context);
    results.push(
      makeFact(
        {
          event_name: eventName,
          payload_schema: payloadSchema,
          trigger_conditions: triggerConditions,
          description,
        },
        source,
      ),
    );
  }
  return results;
}

function extractPayloadInfo(context: string): string {
  const payloadMatch = context.match(
    /(?:payload|body|data)\s*[:]\s*(.{10,100})/i,
  );
  return payloadMatch ? payloadMatch[1].trim().replace(/[.\n].*$/, "") : "";
}

function extractTriggerConditions(context: string): string {
  const triggerMatch = context.match(
    /(?:trigger|fired|sent|emitted)\s+(?:when|if|on)\s+(.{10,100})/i,
  );
  return triggerMatch ? triggerMatch[1].trim().replace(/[.\n].*$/, "") : "";
}

function parseStatusCodes(
  text: string,
  source: FactSource,
): ParsedFact<StatusCodeMapping>[] {
  const results: ParsedFact<StatusCodeMapping>[] = [];
  // Match status/result codes like: 000.100.110, 800.100.151, or simple codes like 200, 400
  const codePattern =
    /(?:^|[\s|])(\d{3}(?:\.\d{3}\.\d{3})?)\s*[|:-]\s*(.{5,100})/gm;
  let match: RegExpExecArray | null;
  const seen = new Set<string>();
  while ((match = codePattern.exec(text)) !== null) {
    const code = match[1];
    if (seen.has(code)) continue;
    seen.add(code);
    const rawDescription = match[2].trim();
    const { description, canonicalState } = parseCodeDescription(
      code,
      rawDescription,
    );
    results.push(
      makeFact(
        {
          provider_code: code,
          canonical_state: canonicalState,
          description,
        },
        source,
      ),
    );
  }
  return results;
}

function parseCodeDescription(
  code: string,
  raw: string,
): { description: string; canonicalState: string } {
  // Try to infer canonical state from description
  const lower = raw.toLowerCase();
  let canonicalState = "Unknown";
  if (lower.includes("success") || lower.includes("approved")) {
    canonicalState = "Captured";
  } else if (lower.includes("pending") || lower.includes("processing")) {
    canonicalState = "Pending";
  } else if (lower.includes("declined") || lower.includes("rejected")) {
    canonicalState = "Failed";
  } else if (lower.includes("timed out") || lower.includes("timeout")) {
    canonicalState = "TimedOut";
  } else if (lower.includes("expired")) {
    canonicalState = "Failed";
  } else if (lower.includes("refund")) {
    canonicalState = "Refunded";
  } else if (lower.includes("void") || lower.includes("cancel")) {
    canonicalState = "Voided";
  }
  // Truncate description for token budget awareness
  const description =
    raw.length > 120 ? raw.slice(0, 117) + "..." : raw;
  return { description, canonicalState };
}

function parseErrorCodes(
  text: string,
  source: FactSource,
): ParsedFact<ErrorCodeFact>[] {
  const results: ParsedFact<ErrorCodeFact>[] = [];
  // Match error code patterns like: 800.100.151 - Description
  const errorPattern =
    /(?:^|[\s|])(\d{3}\.\d{3}\.\d{3})\s*[|:-]\s*(.{5,200})/gm;
  let match: RegExpExecArray | null;
  const seen = new Set<string>();
  while ((match = errorPattern.exec(text)) !== null) {
    const code = match[1];
    if (seen.has(code)) continue;
    seen.add(code);
    const rawDescription = match[2].trim();
    const description =
      rawDescription.length > 120
        ? rawDescription.slice(0, 117) + "..."
        : rawDescription;
    const recoveryAction = inferRecoveryAction(rawDescription);
    results.push(
      makeFact(
        {
          code,
          description,
          recovery_action: recoveryAction,
        },
        source,
      ),
    );
  }
  return results;
}

function inferRecoveryAction(description: string): string {
  const lower = description.toLowerCase();
  if (lower.includes("expired")) return "Request updated card details";
  if (lower.includes("insufficient")) return "Use alternative payment method";
  if (lower.includes("invalid") && lower.includes("card"))
    return "Verify card number";
  if (lower.includes("declined")) return "Contact card issuer";
  if (lower.includes("timeout") || lower.includes("timed out"))
    return "Retry request";
  if (lower.includes("limit")) return "Try smaller amount or wait";
  return "Review error details and retry";
}

function parseFlows(
  text: string,
  source: FactSource,
): ParsedFact<PaymentFlowSequence>[] {
  const results: ParsedFact<PaymentFlowSequence>[] = [];
  // Match flow sections: headings containing "flow" followed by numbered/bulleted steps
  const flowSectionPattern =
    /#+\s+(.+?(?:flow|sequence|lifecycle|process).*?)\n([\s\S]*?)(?=\n#+\s|\n\n\n|$)/gi;
  let match: RegExpExecArray | null;
  while ((match = flowSectionPattern.exec(text)) !== null) {
    const name = match[1]
      .trim()
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9-]/g, "");
    const body = match[2];
    // Extract steps from numbered or bulleted lists
    const stepPattern = /(?:^\s*(?:\d+[.)]\s*|-\s*))(.+)/gm;
    const steps: string[] = [];
    let stepMatch: RegExpExecArray | null;
    while ((stepMatch = stepPattern.exec(body)) !== null) {
      const step = stepMatch[1].trim();
      if (step.length > 3 && step.length < 200) {
        steps.push(step);
      }
    }
    if (steps.length >= 2) {
      results.push(
        makeFact(
          {
            name,
            steps,
            description: match[1].trim(),
          },
          source,
        ),
      );
    }
  }
  return results;
}

export function parseDocumentation(
  text: string,
  source: FactSource,
): ParseResult {
  return {
    endpoints: parseEndpoints(text, source),
    webhooks: parseWebhookEvents(text, source),
    status_codes: parseStatusCodes(text, source),
    errors: parseErrorCodes(text, source),
    flows: parseFlows(text, source),
  };
}
