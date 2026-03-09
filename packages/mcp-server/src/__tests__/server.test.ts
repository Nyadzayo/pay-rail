import { describe, it, expect } from "vitest";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { createServer, VERSION } from "../index.js";

function createTestClient() {
  const { server } = createServer();
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();

  const client = new Client({ name: "test-client", version: "1.0.0" });

  return { server, client, clientTransport, serverTransport };
}

describe("MCP Server", () => {
  it("exports correct version", () => {
    expect(VERSION).toBe("0.1.0");
  });

  it("creates server with createServer()", () => {
    const { server, config } = createServer();
    expect(server).toBeDefined();
    expect(config).toBeDefined();
    expect(config.confidence.generate).toBe(0.9);
    expect(config.confidence.verify_min).toBe(0.7);
    expect(config.token_budget).toBe(14000);
  });

  it("registers exactly 4 tools", async () => {
    const { server, client, clientTransport, serverTransport } = createTestClient();

    await Promise.all([
      server.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const { tools } = await client.listTools();
    const toolNames = tools.map((t) => t.name).sort();

    expect(toolNames).toEqual([
      "generate_adapter",
      "query_provider_pack",
      "run_conformance",
      "validate_state_machine",
    ]);

    await client.close();
    await server.close();
  });

  it("each tool has description and input schema", async () => {
    const { server, client, clientTransport, serverTransport } = createTestClient();

    await Promise.all([
      server.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const { tools } = await client.listTools();

    for (const tool of tools) {
      expect(tool.description).toBeTruthy();
      expect(tool.description!.length).toBeGreaterThan(20);
      expect(tool.inputSchema).toBeDefined();
      expect(tool.inputSchema.type).toBe("object");
    }

    await client.close();
    await server.close();
  });

  it("query_provider_pack has correct input schema", async () => {
    const { server, client, clientTransport, serverTransport } = createTestClient();

    await Promise.all([
      server.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const { tools } = await client.listTools();
    const queryTool = tools.find((t) => t.name === "query_provider_pack")!;

    expect(queryTool.inputSchema.properties).toHaveProperty("provider");
    expect(queryTool.inputSchema.properties).toHaveProperty("query_type");
    expect(queryTool.inputSchema.required).toContain("provider");
    expect(queryTool.inputSchema.required).toContain("query_type");

    await client.close();
    await server.close();
  });

  it("invokes stub tool and receives structured response", async () => {
    const { server, client, clientTransport, serverTransport } = createTestClient();

    await Promise.all([
      server.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const result = await client.callTool({
      name: "query_provider_pack",
      arguments: { provider: "peach-payments", query_type: "overview" },
    });

    expect(result.isError).toBeFalsy();
    expect(result.content).toHaveLength(1);
    const content = result.content as Array<{ type: string; text: string }>;
    const text = content[0].text;
    // peach-payments has no compiled pack, so returns missing-provider message
    expect(text).toContain("No knowledge pack for peach-payments");
    expect(text).toContain("VERIFY");
    expect(text).toContain("knowledge pack");
    expect(text).not.toMatch(/[😀-🙏🌀-🗿🚀-🛿🤀-🧿]/u);

    await client.close();
    await server.close();
  });

  it("rejects invalid tool input via MCP SDK validation", async () => {
    const { server, client, clientTransport, serverTransport } = createTestClient();

    await Promise.all([
      server.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const result = await client.callTool({
      name: "query_provider_pack",
      arguments: { provider: "test", query_type: "invalid_type" },
    });

    expect(result.isError).toBe(true);

    await client.close();
    await server.close();
  });

  it("validate_state_machine rejects empty input", async () => {
    const { server, client, clientTransport, serverTransport } = createTestClient();

    await Promise.all([
      server.server.connect(serverTransport),
      client.connect(clientTransport),
    ]);

    const result = await client.callTool({
      name: "validate_state_machine",
      arguments: {},
    });

    expect(result.isError).toBe(true);
    const content = result.content as Array<{ type: string; text: string }>;
    const parsed = JSON.parse(content[0].text);
    expect(parsed.code).toBe("MCP_INVALID_INPUT");
    expect(parsed.context.tool).toBe("validate_state_machine");

    await client.close();
    await server.close();
  });

  it("can be imported as library without side effects", async () => {
    const { createServer: importedCreate, VERSION: importedVersion } = await import("../index.js");
    expect(importedCreate).toBeTypeOf("function");
    expect(importedVersion).toBe("0.1.0");
  });
});
