import { z } from "zod";
import { resolve } from "node:path";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { formatErrorResponse, PayRailMcpError, MCP_ERROR_CODES } from "../types/errors.js";
import type { PayRailConfig } from "../config/schema.js";
import { loadCompiledPack, type LoadedPack } from "../knowledge/loader.js";
import type { CompiledFact } from "../knowledge/compiler.js";
import { estimateTokenCount, enforceTokenBudget } from "../context/token-budget.js";

export const QueryProviderPackInputSchema = {
  provider: z.string().describe("Provider name (e.g., 'peach-payments', 'startbutton')"),
  query_type: z
    .enum(["overview", "endpoints", "webhooks", "status_codes", "error_codes", "flows"])
    .describe("Type of information to query from the knowledge pack"),
};

const TOOL_NAME = "query_provider_pack";

const TOOL_DESCRIPTION =
  "Query a provider knowledge pack for structured payment domain information. " +
  "Use this to get an overview of a provider's capabilities, look up specific endpoints, " +
  "webhook event types, status code mappings, error codes, or payment flows. " +
  "Returns structured markdown with confidence scores for each fact.";

type QueryType = "overview" | "endpoints" | "webhooks" | "status_codes" | "error_codes" | "flows";

const OVERVIEW_TOKEN_BUDGET = 200;

const PROVIDER_NAME_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$/;

function resolveKnowledgePacksPath(config: PayRailConfig): string {
  return config.knowledge_packs_path
    ? resolve(config.knowledge_packs_path)
    : resolve(process.cwd(), "knowledge-packs");
}

function escapeMarkdownCell(text: string): string {
  return String(text ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
}

function safeStr(value: unknown, fallback: string = "N/A"): string {
  if (value === undefined || value === null) return fallback;
  return String(value);
}

function formatMissingProvider(provider: string): string {
  return (
    `**No knowledge pack for ${escapeMarkdownCell(provider)}.**\n\n` +
    `Generate with VERIFY markers on all mappings, or create a knowledge pack first.\n\n` +
    `**Suggested paths:**\n` +
    `1. Generate adapter code with VERIFY markers on ALL provider-specific mappings (graceful degradation)\n` +
    `2. Create a knowledge pack first via \`knowledge init ${escapeMarkdownCell(provider)}\` for higher-confidence generation`
  );
}

function formatOverview(loaded: LoadedPack): string {
  const { pack, meta } = loaded;
  const m = pack.metadata;

  const flows = pack.facts
    .filter((f) => f.category === "flows")
    .map((f) => {
      const v = f.value as Record<string, unknown>;
      return safeStr(v.name, "unnamed");
    });

  const flowsList = flows.length > 0 ? flows.slice(0, 10).join(", ") : "None documented";

  const overview =
    `**${escapeMarkdownCell(m.display_name)}** (${meta.version})\n\n` +
    `| Field | Value |\n|-------|-------|\n` +
    `| **Provider** | ${escapeMarkdownCell(m.name)} |\n` +
    `| **Version** | ${meta.version} |\n` +
    `| **Coverage** | ${meta.coverage_pct}% |\n` +
    `| **Total Facts** | ${pack.facts.length} |\n` +
    `| **Token Count** | ${meta.token_count} |\n\n` +
    `**Confidence Distribution**\n\n` +
    `| Band | Count |\n|------|-------|\n` +
    `| Generate (>=0.9) | ${meta.confidence_summary.generate} |\n` +
    `| Verify (0.7-0.89) | ${meta.confidence_summary.verify} |\n` +
    `| Refused (<0.7) | ${meta.confidence_summary.refuse_excluded} |\n\n` +
    `**Supported Flows:** ${flowsList}`;

  return enforceTokenBudget(overview, OVERVIEW_TOKEN_BUDGET);
}

function formatEndpoints(facts: CompiledFact[]): string {
  if (facts.length === 0) return "No endpoint facts available.";

  const lines = [
    "**Endpoints**\n",
    "| URL | Method | Confidence | Source | Status |",
    "|-----|--------|------------|--------|--------|",
  ];

  for (const f of facts) {
    const v = f.value as Record<string, unknown>;
    const status = f.verify_marker ? "VERIFY" : "OK";
    lines.push(`| \`${escapeMarkdownCell(safeStr(v.url))}\` | ${escapeMarkdownCell(safeStr(v.method))} | ${f.confidence_score} | ${f.source} | ${status} |`);
    if (f.verify_marker) {
      lines.push(`| | | | | \`${escapeMarkdownCell(f.verify_marker)}\` |`);
    }
  }

  return lines.join("\n");
}

function formatWebhooks(facts: CompiledFact[]): string {
  if (facts.length === 0) return "No webhook facts available.";

  const lines = [
    "**Webhooks**\n",
    "| Event | Trigger | Confidence | Source | Status |",
    "|-------|---------|------------|--------|--------|",
  ];

  for (const f of facts) {
    const v = f.value as Record<string, unknown>;
    const status = f.verify_marker ? "VERIFY" : "OK";
    lines.push(`| \`${escapeMarkdownCell(safeStr(v.event_name))}\` | ${escapeMarkdownCell(safeStr(v.trigger_conditions))} | ${f.confidence_score} | ${f.source} | ${status} |`);
    if (f.verify_marker) {
      lines.push(`| | | | | \`${escapeMarkdownCell(f.verify_marker)}\` |`);
    }
  }

  return lines.join("\n");
}

function formatStatusCodes(facts: CompiledFact[]): string {
  if (facts.length === 0) return "No status code facts available.";

  const lines = [
    "**Status Codes**\n",
    "| Provider Code | Canonical State | Confidence | Source | Status |",
    "|---------------|-----------------|------------|--------|--------|",
  ];

  for (const f of facts) {
    const v = f.value as Record<string, unknown>;
    const status = f.verify_marker ? "VERIFY" : "OK";
    lines.push(`| \`${escapeMarkdownCell(safeStr(v.provider_code))}\` | ${escapeMarkdownCell(safeStr(v.canonical_state))} | ${f.confidence_score} | ${f.source} | ${status} |`);
    if (f.verify_marker) {
      lines.push(`| | | | | \`${escapeMarkdownCell(f.verify_marker)}\` |`);
    }
  }

  return lines.join("\n");
}

function formatErrorCodes(facts: CompiledFact[]): string {
  if (facts.length === 0) return "No error code facts available.";

  const lines = [
    "**Error Codes**\n",
    "| Code | Description | Recovery | Confidence | Source | Status |",
    "|------|-------------|----------|------------|--------|--------|",
  ];

  for (const f of facts) {
    const v = f.value as Record<string, unknown>;
    const status = f.verify_marker ? "VERIFY" : "OK";
    lines.push(`| \`${escapeMarkdownCell(safeStr(v.code))}\` | ${escapeMarkdownCell(safeStr(v.description))} | ${escapeMarkdownCell(safeStr(v.recovery_action))} | ${f.confidence_score} | ${f.source} | ${status} |`);
    if (f.verify_marker) {
      lines.push(`| | | | | | \`${escapeMarkdownCell(f.verify_marker)}\` |`);
    }
  }

  return lines.join("\n");
}

function formatFlows(facts: CompiledFact[]): string {
  if (facts.length === 0) return "No payment flow facts available.";

  const lines = ["**Payment Flows**\n"];

  for (const f of facts) {
    const v = f.value as Record<string, unknown>;
    const name = safeStr(v.name, "unnamed");
    const description = safeStr(v.description);
    const steps = Array.isArray(v.steps) ? v.steps.map(String) : [];
    const status = f.verify_marker ? " (VERIFY)" : "";
    lines.push(`**${escapeMarkdownCell(name)}**${status}`);
    lines.push(escapeMarkdownCell(description));
    lines.push(`Steps: ${steps.join(" -> ")}`);
    lines.push(`Confidence: ${f.confidence_score} | Source: ${f.source}`);
    if (f.verify_marker) {
      lines.push(`\`${f.verify_marker}\``);
    }
    lines.push("");
  }

  return lines.join("\n");
}

const CATEGORY_MAP: Record<QueryType, string> = {
  overview: "",
  endpoints: "endpoints",
  webhooks: "webhooks",
  status_codes: "status_codes",
  error_codes: "errors",
  flows: "flows",
};

function formatDetail(loaded: LoadedPack, queryType: QueryType): string {
  const category = CATEGORY_MAP[queryType];
  const facts = loaded.pack.facts.filter((f) => f.category === category);

  switch (queryType) {
    case "endpoints":
      return formatEndpoints(facts);
    case "webhooks":
      return formatWebhooks(facts);
    case "status_codes":
      return formatStatusCodes(facts);
    case "error_codes":
      return formatErrorCodes(facts);
    case "flows":
      return formatFlows(facts);
    default:
      return "Unknown query type.";
  }
}

function validateProviderName(provider: string): void {
  if (!PROVIDER_NAME_PATTERN.test(provider)) {
    throw new PayRailMcpError(
      MCP_ERROR_CODES.MCP_INVALID_INPUT,
      `[INVALID_PROVIDER_NAME] Provider name "${provider}" contains invalid characters. Use lowercase alphanumeric and hyphens only (e.g., "peach-payments"). [Check provider name format]`,
      { tool: TOOL_NAME, provider },
    );
  }
}

export function registerQueryProviderPack(server: McpServer, config: PayRailConfig): void {
  server.tool(TOOL_NAME, TOOL_DESCRIPTION, QueryProviderPackInputSchema, async (args) => {
    try {
      const provider = args.provider;
      const queryType = args.query_type as QueryType;

      validateProviderName(provider);

      const basePath = resolveKnowledgePacksPath(config);
      const loaded = loadCompiledPack(provider, basePath);

      if (!loaded) {
        return {
          content: [{ type: "text" as const, text: formatMissingProvider(provider) }],
        };
      }

      const text = queryType === "overview"
        ? formatOverview(loaded)
        : formatDetail(loaded, queryType);

      return {
        content: [{ type: "text" as const, text }],
      };
    } catch (error) {
      return formatErrorResponse(error, TOOL_NAME);
    }
  });
}
