import { describe, it, expect } from "vitest";
import {
  PayRailMcpError,
  MCP_ERROR_CODES,
  formatErrorResponse,
} from "../errors.js";

describe("PayRailMcpError", () => {
  it("creates error with code, message, and context", () => {
    const error = new PayRailMcpError(
      MCP_ERROR_CODES.MCP_TOOL_ERROR,
      "Something went wrong",
      { tool: "query_provider_pack" },
    );

    expect(error.code).toBe("MCP_TOOL_ERROR");
    expect(error.message).toBe("Something went wrong");
    expect(error.domain).toBe("mcp");
    expect(error.context).toEqual({ tool: "query_provider_pack" });
    expect(error.name).toBe("PayRailMcpError");
  });

  it("defaults context to empty object", () => {
    const error = new PayRailMcpError(
      MCP_ERROR_CODES.MCP_CONFIG_ERROR,
      "Bad config",
    );
    expect(error.context).toEqual({});
  });

  it("produces canonical JSON payload", () => {
    const error = new PayRailMcpError(
      MCP_ERROR_CODES.MCP_INVALID_INPUT,
      "Missing provider name",
      { tool: "query_provider_pack", input: {} },
    );

    const payload = error.toPayload();
    expect(payload).toEqual({
      code: "MCP_INVALID_INPUT",
      message: "Missing provider name",
      domain: "mcp",
      context: { tool: "query_provider_pack", input: {} },
    });
  });

  it("supports all MCP error codes", () => {
    const codes = Object.values(MCP_ERROR_CODES);
    expect(codes).toContain("MCP_TOOL_ERROR");
    expect(codes).toContain("MCP_INVALID_INPUT");
    expect(codes).toContain("MCP_CONFIG_ERROR");
    expect(codes).toContain("MCP_PROVIDER_NOT_FOUND");
    expect(codes).toHaveLength(4);
  });
});

describe("formatErrorResponse", () => {
  it("formats PayRailMcpError as CallToolResult", () => {
    const error = new PayRailMcpError(
      MCP_ERROR_CODES.MCP_PROVIDER_NOT_FOUND,
      "No knowledge pack for stripe",
      { tool: "query_provider_pack" },
    );

    const result = formatErrorResponse(error);
    expect(result.isError).toBe(true);
    expect(result.content).toHaveLength(1);

    const parsed = JSON.parse((result.content[0] as { text: string }).text);
    expect(parsed.code).toBe("MCP_PROVIDER_NOT_FOUND");
    expect(parsed.domain).toBe("mcp");
    expect(parsed.message).toBe("No knowledge pack for stripe");
  });

  it("wraps generic Error as MCP_TOOL_ERROR", () => {
    const error = new Error("Unexpected failure");
    const result = formatErrorResponse(error, "generate_adapter");

    const parsed = JSON.parse((result.content[0] as { text: string }).text);
    expect(parsed.code).toBe("MCP_TOOL_ERROR");
    expect(parsed.message).toBe("Unexpected failure");
    expect(parsed.context.tool).toBe("generate_adapter");
  });

  it("wraps string error as MCP_TOOL_ERROR", () => {
    const result = formatErrorResponse("something broke");

    const parsed = JSON.parse((result.content[0] as { text: string }).text);
    expect(parsed.code).toBe("MCP_TOOL_ERROR");
    expect(parsed.message).toBe("something broke");
  });

  it("merges toolName into PayRailMcpError when context.tool is missing", () => {
    const error = new PayRailMcpError(
      MCP_ERROR_CODES.MCP_CONFIG_ERROR,
      "Bad threshold",
    );
    const result = formatErrorResponse(error, "generate_adapter");

    const parsed = JSON.parse((result.content[0] as { text: string }).text);
    expect(parsed.context.tool).toBe("generate_adapter");
  });

  it("preserves existing context.tool on PayRailMcpError", () => {
    const error = new PayRailMcpError(
      MCP_ERROR_CODES.MCP_TOOL_ERROR,
      "Failed",
      { tool: "original_tool" },
    );
    const result = formatErrorResponse(error, "different_tool");

    const parsed = JSON.parse((result.content[0] as { text: string }).text);
    expect(parsed.context.tool).toBe("original_tool");
  });
});
