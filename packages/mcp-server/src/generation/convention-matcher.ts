import type { CodebaseFingerprint } from "../fingerprint/conventions.js";
import type { GeneratedFile } from "./pipeline.js";

function toSnakeCase(str: string): string {
  return str
    .replace(/([a-z])([A-Z])/g, "$1_$2")
    .replace(/[-\s]+/g, "_")
    .toLowerCase();
}

function toCamelCase(str: string): string {
  return str
    .replace(/[-_\s]+(.)/g, (_, c: string) => c.toUpperCase())
    .replace(/^[A-Z]/, (c) => c.toLowerCase());
}

const IDENTIFIER_PATTERN = /\b(const|let|var|function)\s+([a-zA-Z_][a-zA-Z0-9_]*)/g;

function convertIdentifiers(content: string, targetConvention: string): string {
  if (targetConvention === "camelCase") {
    return content.replace(IDENTIFIER_PATTERN, (match, keyword: string, name: string) => {
      if (name.includes("_") && !/^[A-Z_]+$/.test(name)) {
        return `${keyword} ${toCamelCase(name)}`;
      }
      return match;
    });
  }

  if (targetConvention === "snake_case") {
    return content.replace(IDENTIFIER_PATTERN, (match, keyword: string, name: string) => {
      if (/[a-z][A-Z]/.test(name)) {
        return `${keyword} ${toSnakeCase(name)}`;
      }
      return match;
    });
  }

  return content;
}

function addBarrelExport(files: GeneratedFile[], fingerprint: CodebaseFingerprint): GeneratedFile[] {
  if (fingerprint.moduleStructure.value !== "barrel-exports") return files;

  const adapterFile = files.find((f) => f.name.includes("adapter") && !f.name.includes(".test."));
  const webhookFile = files.find((f) => f.name.includes("webhook"));
  const idempotencyFile = files.find((f) => f.name.includes("idempotency"));

  const exports: string[] = [];
  if (adapterFile) exports.push(`export * from "./${adapterFile.name.replace(".ts", "")}";`);
  if (webhookFile) exports.push(`export * from "./${webhookFile.name.replace(".ts", "")}";`);
  if (idempotencyFile) exports.push(`export * from "./${idempotencyFile.name.replace(".ts", "")}";`);

  if (exports.length > 0) {
    const indexContent = exports.join("\n") + "\n";
    files.push({
      name: "index.ts",
      path: "src/adapters/index.ts",
      content: indexContent,
      lineCount: exports.length,
    });
  }

  return files;
}

export function applyConventions(
  file: GeneratedFile,
  fingerprint: CodebaseFingerprint,
): GeneratedFile {
  let content = file.content;

  // Apply naming convention to non-test files
  if (!file.name.includes(".test.")) {
    content = convertIdentifiers(content, fingerprint.namingConvention.value);
  }

  return {
    ...file,
    content,
    lineCount: content.split("\n").length,
  };
}

export function applyConventionsToAll(
  files: GeneratedFile[],
  fingerprint: CodebaseFingerprint,
): GeneratedFile[] {
  let result = files.map((f) => applyConventions(f, fingerprint));
  result = addBarrelExport(result, fingerprint);
  return result;
}
