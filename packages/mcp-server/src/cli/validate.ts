#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import {
  SandboxValidator,
  formatValidationReport,
  type HttpClient,
} from "../knowledge/validator.js";
import type { KnowledgePack } from "../knowledge/schema.js";
import { KnowledgePackSchema } from "../knowledge/schema.js";
import { parse as parseYaml, stringify as stringifyYaml } from "yaml";

function usage(): never {
  console.error(
    "Usage: payrail-validate <provider> --pack <pack.yaml> [--sandbox]",
  );
  process.exit(1);
}

function createHttpClient(): HttpClient {
  return {
    async request(method, url, options) {
      const headers: Record<string, string> = {
        ...options?.headers,
      };

      let body: string | undefined;
      if (options?.body) {
        if (options.formEncoded && typeof options.body === "object") {
          const params = new URLSearchParams();
          for (const [k, v] of Object.entries(
            options.body as Record<string, string>,
          )) {
            params.set(k, v);
          }
          body = params.toString();
          headers["Content-Type"] = "application/x-www-form-urlencoded";
        } else if (typeof options.body === "string") {
          body = options.body;
        }
      }

      const resp = await fetch(url, {
        method,
        headers,
        body,
        signal: AbortSignal.timeout(30_000),
      });

      const contentType = resp.headers.get("content-type") ?? "";
      let respBody: unknown;
      if (contentType.includes("json")) {
        respBody = await resp.json();
      } else {
        respBody = await resp.text();
      }

      const respHeaders: Record<string, string> = {};
      resp.headers.forEach((v, k) => {
        respHeaders[k] = v;
      });

      return {
        status: resp.status,
        body: respBody,
        headers: respHeaders,
      };
    },
  };
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  if (args.length < 3) usage();

  const provider = args[0];
  let packPath: string | undefined;
  let sandbox = false;

  for (let i = 1; i < args.length; i++) {
    if (args[i] === "--pack" && args[i + 1]) {
      packPath = args[++i];
    } else if (args[i] === "--sandbox") {
      sandbox = true;
    }
  }

  if (!provider || !packPath) usage();
  if (!sandbox) {
    console.error("--sandbox flag is required for validation");
    process.exit(1);
  }

  let pack: KnowledgePack;
  try {
    const yaml = readFileSync(packPath, "utf-8");
    pack = KnowledgePackSchema.parse(parseYaml(yaml));
  } catch (err: unknown) {
    const detail = err instanceof Error ? err.message : String(err);
    console.error(`Cannot read/parse pack file: ${packPath} [${detail}]`);
    process.exit(1);
  }

  let credentials;
  try {
    credentials = SandboxValidator.loadCredentials(provider);
  } catch (err: unknown) {
    const detail = err instanceof Error ? err.message : String(err);
    console.error(`Credential error: ${detail}`);
    process.exit(1);
  }

  const client = createHttpClient();
  const validator = new SandboxValidator(
    pack,
    provider,
    credentials,
    client,
    (progress) => {
      process.stdout.write(
        `\rTesting endpoint ${progress.current}/${progress.total}: ${progress.endpoint}`,
      );
    },
  );

  const report = await validator.validate();

  // Clear progress line
  process.stdout.write("\r" + " ".repeat(80) + "\r");

  console.log(`Provider: ${provider}`);
  console.log(`Pack: ${packPath}`);
  console.log("");
  console.log(formatValidationReport(report));

  // Write updated pack back to disk
  try {
    writeFileSync(packPath, stringifyYaml(report.updatedPack), "utf-8");
    console.log("");
    console.log(`Updated pack written to: ${packPath}`);
  } catch (err: unknown) {
    const detail = err instanceof Error ? err.message : String(err);
    console.error(`Cannot write updated pack: ${packPath} [${detail}]`);
    process.exit(1);
  }
}

main().catch((err: unknown) => {
  const detail = err instanceof Error ? err.message : String(err);
  console.error(`Validation failed: ${detail}`);
  process.exit(1);
});
