import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { loadConfig } from "./config/loader.js";
import { registerQueryProviderPack } from "./tools/query-provider-pack.js";
import { registerGenerateAdapter } from "./tools/generate-adapter.js";
import { registerValidateStateMachine } from "./tools/validate-state-machine.js";
import { registerRunConformance } from "./tools/run-conformance.js";
import type { PayRailConfig } from "./config/schema.js";

export const VERSION = "0.1.0";

export type { PayRailConfig };

export function createServer(configPath?: string) {
  const config = loadConfig(configPath);

  const server = new McpServer(
    { name: "payrail-mcp-server", version: VERSION },
    {
      capabilities: {
        tools: {},
      },
    },
  );

  registerQueryProviderPack(server, config);
  registerGenerateAdapter(server, config);
  registerValidateStateMachine(server, config);
  registerRunConformance(server, config);

  return { server, config };
}
