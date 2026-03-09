import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { formatErrorResponse, PayRailMcpError, MCP_ERROR_CODES } from "../types/errors.js";
import type { PayRailConfig } from "../config/schema.js";
import { runPipeline, formatGenerationOutput } from "../generation/pipeline.js";

export const GenerateAdapterInputSchema = {
  provider: z.string().describe("Provider name (e.g., 'peach-payments', 'startbutton')"),
  target_language: z
    .enum(["typescript", "rust"])
    .default("typescript")
    .describe("Target language for the generated adapter"),
  project_path: z
    .string()
    .optional()
    .describe("Path to the developer's project for codebase fingerprinting"),
};

const TOOL_NAME = "generate_adapter";

const TOOL_DESCRIPTION =
  "Generate a complete, convention-matching payment adapter with conformance tests. " +
  "Produces a provider adapter file (~150-200 lines), webhook handler, conformance test suite, " +
  "and idempotency configuration. Uses the three-layer context stack: universal payment rules + " +
  "provider knowledge pack + codebase conventions. Facts with confidence below 0.7 are refused; " +
  "0.7-0.89 get VERIFY markers; >=0.9 are generated directly.";

const PROVIDER_NAME_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$/;

export function registerGenerateAdapter(server: McpServer, config: PayRailConfig): void {
  server.tool(TOOL_NAME, TOOL_DESCRIPTION, GenerateAdapterInputSchema, async (args) => {
    try {
      const provider = args.provider;
      if (!PROVIDER_NAME_PATTERN.test(provider)) {
        throw new PayRailMcpError(
          MCP_ERROR_CODES.MCP_INVALID_INPUT,
          `[INVALID_PROVIDER_NAME] Provider name "${provider}" must be lowercase alphanumeric with hyphens (e.g., "peach-payments")`,
          { tool: TOOL_NAME, provider },
        );
      }

      const result = runPipeline(
        provider,
        args.target_language,
        config,
        args.project_path,
      );

      const output = formatGenerationOutput(result);

      return {
        content: [{ type: "text" as const, text: output }],
      };
    } catch (error) {
      return formatErrorResponse(error, TOOL_NAME);
    }
  });
}
