import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { formatErrorResponse, PayRailMcpError, MCP_ERROR_CODES } from "../types/errors.js";
import type { PayRailConfig } from "../config/schema.js";
import { analyzeStateMachine, formatValidationOutput } from "../validation/state-machine-analyzer.js";

export const ValidateStateMachineInputSchema = {
  code: z.string().optional().describe("Inline code snippet to validate for state machine correctness"),
  file_path: z
    .string()
    .optional()
    .describe("Path to a file containing adapter code to validate"),
};

const TOOL_NAME = "validate_state_machine";

const TOOL_DESCRIPTION =
  "Validate state machine correctness in adapter code. Analyzes code for valid transitions, " +
  "missing transitions, unreachable states, and self-transition handling against the canonical " +
  "PayRail state machine (8 states: Created, Authorized, Captured, Refunded, Voided, Failed, " +
  "Expired, Pending3ds). Returns issues categorized by severity (error, warning, info). " +
  "Provide either a code snippet or file path.";

export function registerValidateStateMachine(server: McpServer, _config: PayRailConfig): void {
  server.tool(TOOL_NAME, TOOL_DESCRIPTION, ValidateStateMachineInputSchema, async (args) => {
    try {
      if (!args.code && !args.file_path) {
        throw new PayRailMcpError(
          MCP_ERROR_CODES.MCP_INVALID_INPUT,
          "At least one of 'code' or 'file_path' is required. Provide a code snippet or a file path to validate.",
          { tool: TOOL_NAME },
        );
      }

      const result = analyzeStateMachine({
        code: args.code,
        filePath: args.file_path,
      });

      const output = formatValidationOutput(result);

      return {
        content: [{ type: "text" as const, text: output }],
      };
    } catch (error) {
      return formatErrorResponse(error, TOOL_NAME);
    }
  });
}
