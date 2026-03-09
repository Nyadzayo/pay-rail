import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { formatErrorResponse, PayRailMcpError, MCP_ERROR_CODES } from "../types/errors.js";
import type { PayRailConfig } from "../config/schema.js";
import { runConformanceTests, formatConformanceOutput } from "../validation/conformance-runner.js";

export const RunConformanceInputSchema = {
  provider: z.string().describe("Provider name (e.g., 'peach-payments', 'startbutton')"),
  adapter_path: z.string().describe("Path to the adapter file or directory to test"),
  test_runner: z
    .enum(["cargo", "vitest"])
    .optional()
    .describe("Test runner to use. Auto-detected from adapter_path extension if not specified."),
};

const TOOL_NAME = "run_conformance";

const TOOL_DESCRIPTION =
  "Execute the conformance test suite against a provider adapter. Runs state transition tests " +
  "from the PayRail conformance harness and returns results per state transition. Reports pass/fail " +
  "counts, and for failures includes: expected canonical state, actual mapped state, source location, " +
  "impact description, and fix guidance. Results fit within ~500 tokens.";

const PROVIDER_NAME_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$/;

export function registerRunConformance(server: McpServer, _config: PayRailConfig): void {
  server.tool(TOOL_NAME, TOOL_DESCRIPTION, RunConformanceInputSchema, async (args) => {
    try {
      if (!PROVIDER_NAME_PATTERN.test(args.provider)) {
        throw new PayRailMcpError(
          MCP_ERROR_CODES.MCP_INVALID_INPUT,
          `[INVALID_PROVIDER_NAME] Provider name "${args.provider}" must be lowercase alphanumeric with hyphens`,
          { tool: TOOL_NAME, provider: args.provider },
        );
      }

      const result = await runConformanceTests({
        provider: args.provider,
        adapterPath: args.adapter_path,
        testRunner: args.test_runner,
      });

      const output = formatConformanceOutput(result);

      return {
        content: [{ type: "text" as const, text: output }],
      };
    } catch (error) {
      return formatErrorResponse(error, TOOL_NAME);
    }
  });
}
