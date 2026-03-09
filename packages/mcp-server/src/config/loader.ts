import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import { parse as parseYaml } from "yaml";
import { PayRailConfigSchema, type PayRailConfig } from "./schema.js";
import { PayRailMcpError, MCP_ERROR_CODES } from "../types/errors.js";

const CONFIG_FILENAME = "payrail.config.yaml";

export function loadConfig(configPath?: string): PayRailConfig {
  const resolvedPath = configPath ?? resolve(process.cwd(), CONFIG_FILENAME);

  if (!existsSync(resolvedPath)) {
    return PayRailConfigSchema.parse({});
  }

  let rawContent: string;
  try {
    rawContent = readFileSync(resolvedPath, "utf-8");
  } catch (err) {
    throw new PayRailMcpError(
      MCP_ERROR_CODES.MCP_CONFIG_ERROR,
      `Failed to read config file: ${resolvedPath}. ${err instanceof Error ? err.message : String(err)}. Check file permissions and path.`,
      { path: resolvedPath },
    );
  }

  let parsed: unknown;
  try {
    parsed = parseYaml(rawContent);
  } catch (err) {
    throw new PayRailMcpError(
      MCP_ERROR_CODES.MCP_CONFIG_ERROR,
      `Invalid YAML in config file: ${resolvedPath}. ${err instanceof Error ? err.message : String(err)}. Validate YAML syntax.`,
      { path: resolvedPath },
    );
  }

  const result = PayRailConfigSchema.safeParse(parsed ?? {});
  if (!result.success) {
    const issues = result.error.issues
      .map((i) => `${i.path.join(".")}: ${i.message}`)
      .join("; ");
    throw new PayRailMcpError(
      MCP_ERROR_CODES.MCP_CONFIG_ERROR,
      `Invalid config schema: ${issues}. See payrail.config.yaml documentation for valid fields.`,
      { path: resolvedPath, issues: result.error.issues },
    );
  }

  return result.data;
}
