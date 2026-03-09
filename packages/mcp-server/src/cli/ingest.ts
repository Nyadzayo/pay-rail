#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { ingestDocumentation, formatIngestReport } from "../knowledge/ingestion.js";
import type { FactSource } from "../knowledge/schema.js";
import type { KnowledgePack } from "../knowledge/schema.js";
import { KnowledgePackSchema } from "../knowledge/schema.js";
import { parse as parseYaml, stringify as stringifyYaml } from "yaml";

const VALID_TYPES = [
  "official",
  "community",
  "historical",
  "sandbox",
  "inferred",
] as const;

type SourceArg = (typeof VALID_TYPES)[number];

function sourceArgToFactSource(arg: SourceArg): FactSource {
  const map: Record<SourceArg, FactSource> = {
    official: "official_docs",
    community: "community_report",
    historical: "historical_docs",
    sandbox: "sandbox_test",
    inferred: "inferred",
  };
  return map[arg];
}

function usage(): never {
  console.error(
    "Usage: payrail-ingest <provider> --source <path> --type <official|community|historical|sandbox|inferred> [--pack <pack.yaml>]",
  );
  process.exit(1);
}

function main(): void {
  const args = process.argv.slice(2);
  if (args.length < 5) usage();

  const provider = args[0];
  let sourcePath: string | undefined;
  let sourceType: SourceArg | undefined;
  let packPath: string | undefined;

  for (let i = 1; i < args.length; i++) {
    if (args[i] === "--source" && args[i + 1]) {
      sourcePath = args[++i];
    } else if (args[i] === "--type" && args[i + 1]) {
      const val = args[++i] as SourceArg;
      if (!VALID_TYPES.includes(val)) {
        console.error(`Invalid source type: ${val}`);
        console.error(`Valid types: ${VALID_TYPES.join(", ")}`);
        process.exit(1);
      }
      sourceType = val;
    } else if (args[i] === "--pack" && args[i + 1]) {
      packPath = args[++i];
    }
  }

  if (!provider || !sourcePath || !sourceType) usage();

  let sourceText: string;
  try {
    sourceText = readFileSync(sourcePath, "utf-8");
  } catch (err: unknown) {
    const detail = err instanceof Error ? err.message : String(err);
    console.error(`Cannot read source file: ${sourcePath} [${detail}]`);
    process.exit(1);
  }

  let existingPack: KnowledgePack | undefined;
  if (packPath) {
    try {
      const yaml = readFileSync(packPath, "utf-8");
      existingPack = KnowledgePackSchema.parse(parseYaml(yaml));
    } catch (err: unknown) {
      const detail = err instanceof Error ? err.message : String(err);
      console.error(`Cannot read/parse pack file: ${packPath} [${detail}]`);
      process.exit(1);
    }
  }

  const factSource = sourceArgToFactSource(sourceType);
  const result = ingestDocumentation(sourceText, factSource, existingPack);

  // Persist merged pack to disk when --pack is provided
  if (packPath) {
    try {
      writeFileSync(packPath, stringifyYaml(result.pack), "utf-8");
    } catch (err: unknown) {
      const detail = err instanceof Error ? err.message : String(err);
      console.error(`Cannot write pack file: ${packPath} [${detail}]`);
      process.exit(1);
    }
  }

  console.log(`Provider: ${provider}`);
  console.log(`Source: ${sourcePath} (${sourceType})`);
  console.log("");
  console.log(formatIngestReport(result));
}

main();
