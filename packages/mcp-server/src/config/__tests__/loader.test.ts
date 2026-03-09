import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { loadConfig } from "../loader.js";
import { PayRailMcpError } from "../../types/errors.js";

const TEST_DIR = join(tmpdir(), "payrail-config-test-" + Date.now());

beforeEach(() => {
  mkdirSync(TEST_DIR, { recursive: true });
});

afterEach(() => {
  rmSync(TEST_DIR, { recursive: true, force: true });
});

describe("loadConfig", () => {
  it("returns defaults when config file does not exist", () => {
    const config = loadConfig(join(TEST_DIR, "nonexistent.yaml"));

    expect(config.confidence.generate).toBe(0.9);
    expect(config.confidence.verify_min).toBe(0.7);
    expect(config.token_budget).toBe(14000);
    expect(config.knowledge_packs_path).toBeUndefined();
  });

  it("loads valid YAML config", () => {
    const configPath = join(TEST_DIR, "payrail.config.yaml");
    writeFileSync(
      configPath,
      `confidence:
  generate: 0.85
  verify_min: 0.6
token_budget: 10000
knowledge_packs_path: ./my-packs
`,
    );

    const config = loadConfig(configPath);
    expect(config.confidence.generate).toBe(0.85);
    expect(config.confidence.verify_min).toBe(0.6);
    expect(config.token_budget).toBe(10000);
    expect(config.knowledge_packs_path).toBe("./my-packs");
  });

  it("applies defaults for missing fields", () => {
    const configPath = join(TEST_DIR, "partial.yaml");
    writeFileSync(configPath, "token_budget: 8000\n");

    const config = loadConfig(configPath);
    expect(config.confidence.generate).toBe(0.9);
    expect(config.confidence.verify_min).toBe(0.7);
    expect(config.token_budget).toBe(8000);
  });

  it("throws MCP_CONFIG_ERROR on invalid YAML syntax", () => {
    const configPath = join(TEST_DIR, "bad.yaml");
    writeFileSync(configPath, "confidence:\n  generate: [invalid yaml {{");

    expect(() => loadConfig(configPath)).toThrow(PayRailMcpError);
    try {
      loadConfig(configPath);
    } catch (err) {
      expect((err as PayRailMcpError).code).toBe("MCP_CONFIG_ERROR");
      expect((err as PayRailMcpError).message).toContain("Invalid YAML");
    }
  });

  it("throws MCP_CONFIG_ERROR on schema validation failure", () => {
    const configPath = join(TEST_DIR, "schema-fail.yaml");
    writeFileSync(configPath, "token_budget: -5\n");

    expect(() => loadConfig(configPath)).toThrow(PayRailMcpError);
    try {
      loadConfig(configPath);
    } catch (err) {
      expect((err as PayRailMcpError).code).toBe("MCP_CONFIG_ERROR");
      expect((err as PayRailMcpError).message).toContain("Invalid config schema");
    }
  });

  it("accepts empty YAML file with all defaults", () => {
    const configPath = join(TEST_DIR, "empty.yaml");
    writeFileSync(configPath, "");

    const config = loadConfig(configPath);
    expect(config.confidence.generate).toBe(0.9);
    expect(config.token_budget).toBe(14000);
  });
});
