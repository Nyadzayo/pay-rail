import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";

export const MCP_ERROR_CODES = {
  MCP_TOOL_ERROR: "MCP_TOOL_ERROR",
  MCP_INVALID_INPUT: "MCP_INVALID_INPUT",
  MCP_CONFIG_ERROR: "MCP_CONFIG_ERROR",
  MCP_PROVIDER_NOT_FOUND: "MCP_PROVIDER_NOT_FOUND",
} as const;

export type McpErrorCode = (typeof MCP_ERROR_CODES)[keyof typeof MCP_ERROR_CODES];

export interface McpErrorContext {
  tool?: string;
  input?: unknown;
  [key: string]: unknown;
}

export interface McpErrorPayload {
  code: McpErrorCode;
  message: string;
  domain: "mcp";
  context: McpErrorContext;
}

export class PayRailMcpError extends Error {
  readonly code: McpErrorCode;
  readonly domain = "mcp" as const;
  readonly context: McpErrorContext;

  constructor(code: McpErrorCode, message: string, context: McpErrorContext = {}) {
    super(message);
    this.name = "PayRailMcpError";
    this.code = code;
    this.context = context;
  }

  toPayload(): McpErrorPayload {
    return {
      code: this.code,
      message: this.message,
      domain: this.domain,
      context: this.context,
    };
  }
}

export function formatErrorResponse(error: unknown, toolName?: string): CallToolResult {
  if (error instanceof PayRailMcpError) {
    const payload = error.toPayload();
    if (toolName && !payload.context.tool) {
      payload.context.tool = toolName;
    }
    return {
      isError: true,
      content: [
        {
          type: "text",
          text: JSON.stringify(payload, null, 2),
        },
      ],
    };
  }

  const mcpError = new PayRailMcpError(
    MCP_ERROR_CODES.MCP_TOOL_ERROR,
    error instanceof Error ? error.message : String(error),
    { tool: toolName },
  );

  return {
    isError: true,
    content: [
      {
        type: "text",
        text: JSON.stringify(mcpError.toPayload(), null, 2),
      },
    ],
  };
}
