#!/usr/bin/env node
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import {
  compileKnowledgePack,
  formatCompileReport,
  type CompilationConfig,
} from "../knowledge/compiler.js";
import { KnowledgePackSchema } from "../knowledge/schema.js";
import { parse as parseYaml } from "yaml";

function usage(): never {
  console.error(
    "Usage: payrail-compile <provider> --pack <pack.yaml> [--budget <tokens>] [--config <payrail.config.yaml>]",
  );
  process.exit(1);
}

interface PayrailConfig {
  knowledge?: {
    thresholds?: {
      generate?: number;
      verify_min?: number;
      refuse_below?: number;
    };
    token_budget?: number;
    provider_overrides?: Record<
      string,
      {
        thresholds?: {
          generate?: number;
          verify_min?: number;
          refuse_below?: number;
        };
        token_budget?: number;
      }
    >;
  };
}

function loadConfig(
  configPath: string | undefined,
  provider: string,
  budgetOverride: number | undefined,
): CompilationConfig {
  const defaults: CompilationConfig = {
    thresholds: { generate: 0.9, verify_min: 0.7, refuse_below: 0.7 },
    token_budget: 8000,
  };

  if (!configPath || !existsSync(configPath)) {
    if (budgetOverride !== undefined) {
      defaults.token_budget = budgetOverride;
    }
    return defaults;
  }

  try {
    const raw = readFileSync(configPath, "utf-8");
    const parsed = parseYaml(raw) as PayrailConfig;
    const k = parsed?.knowledge;

    if (k?.thresholds) {
      if (k.thresholds.generate !== undefined)
        defaults.thresholds.generate = k.thresholds.generate;
      if (k.thresholds.verify_min !== undefined)
        defaults.thresholds.verify_min = k.thresholds.verify_min;
      if (k.thresholds.refuse_below !== undefined)
        defaults.thresholds.refuse_below = k.thresholds.refuse_below;
    }
    if (k?.token_budget !== undefined) defaults.token_budget = k.token_budget;

    // Per-provider overrides
    const providerOverride = k?.provider_overrides?.[provider];
    if (providerOverride) {
      if (providerOverride.thresholds) {
        if (providerOverride.thresholds.generate !== undefined)
          defaults.thresholds.generate = providerOverride.thresholds.generate;
        if (providerOverride.thresholds.verify_min !== undefined)
          defaults.thresholds.verify_min =
            providerOverride.thresholds.verify_min;
        if (providerOverride.thresholds.refuse_below !== undefined)
          defaults.thresholds.refuse_below =
            providerOverride.thresholds.refuse_below;
      }
      if (providerOverride.token_budget !== undefined)
        defaults.token_budget = providerOverride.token_budget;
    }

    // CLI budget override takes highest priority
    if (budgetOverride !== undefined) {
      defaults.token_budget = budgetOverride;
    }

    return defaults;
  } catch (err: unknown) {
    const detail = err instanceof Error ? err.message : String(err);
    console.error(`Warning: Could not parse config file: ${detail}`);
    return defaults;
  }
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  if (args.length < 3) usage();

  const provider = args[0];
  let packPath: string | undefined;
  let configPath: string | undefined;
  let budgetOverride: number | undefined;

  for (let i = 1; i < args.length; i++) {
    if (args[i] === "--pack" && args[i + 1]) {
      packPath = args[++i];
    } else if (args[i] === "--config" && args[i + 1]) {
      configPath = args[++i];
    } else if (args[i] === "--budget" && args[i + 1]) {
      budgetOverride = parseInt(args[++i], 10);
      if (isNaN(budgetOverride) || budgetOverride <= 0) {
        console.error("--budget must be a positive integer");
        process.exit(1);
      }
    }
  }

  if (!provider || !packPath) usage();

  // Load and parse pack
  let pack;
  try {
    const yaml = readFileSync(packPath, "utf-8");
    pack = KnowledgePackSchema.parse(parseYaml(yaml));
  } catch (err: unknown) {
    const detail = err instanceof Error ? err.message : String(err);
    console.error(`Cannot read/parse pack file: ${packPath} [${detail}]`);
    process.exit(1);
  }

  // Load config
  const config = loadConfig(configPath, provider, budgetOverride);

  // Compile
  let result;
  try {
    result = compileKnowledgePack(pack, provider, config);
  } catch (err: unknown) {
    const detail = err instanceof Error ? err.message : String(err);
    console.error(`Compilation failed: ${detail}`);
    process.exit(1);
  }

  // Write outputs
  const packDir = dirname(packPath);
  const compiledDir = join(packDir, "compiled");
  mkdirSync(compiledDir, { recursive: true });

  const packJsonPath = join(compiledDir, "pack.json");
  const metaJsonPath = join(compiledDir, "meta.json");

  try {
    writeFileSync(
      packJsonPath,
      JSON.stringify(result.compiledPack, null, 2),
      "utf-8",
    );
    writeFileSync(
      metaJsonPath,
      JSON.stringify(result.meta, null, 2),
      "utf-8",
    );
  } catch (err: unknown) {
    const detail = err instanceof Error ? err.message : String(err);
    console.error(`Cannot write compiled artifacts: ${detail}`);
    process.exit(1);
  }

  console.log(`Provider: ${provider}`);
  console.log(`Pack: ${packPath}`);
  console.log("");
  console.log(formatCompileReport(result));
  console.log("");
  console.log(`Compiled pack written to: ${packJsonPath}`);
  console.log(`Metadata written to: ${metaJsonPath}`);
}

main().catch((err: unknown) => {
  const detail = err instanceof Error ? err.message : String(err);
  console.error(`Compilation failed: ${detail}`);
  process.exit(1);
});
