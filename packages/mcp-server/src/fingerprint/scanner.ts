import { readdirSync, readFileSync, existsSync, statSync } from "node:fs";
import { join, extname, resolve } from "node:path";
import type {
  CodebaseFingerprint,
  DetectedConvention,
  ConfidenceLevel,
} from "./conventions.js";

const EXCLUDED_DIRS = new Set([
  "node_modules", "dist", ".git", "target", "build",
  ".next", ".cache", "coverage", "__pycache__", ".turbo",
]);

const LANGUAGE_MAP: Record<string, string> = {
  ".ts": "TypeScript",
  ".tsx": "TypeScript",
  ".js": "JavaScript",
  ".jsx": "JavaScript",
  ".rs": "Rust",
  ".py": "Python",
  ".go": "Go",
  ".java": "Java",
};

const FRAMEWORK_DEPS: Record<string, string> = {
  next: "Next.js",
  express: "Express",
  fastify: "Fastify",
  "@nestjs/core": "NestJS",
  koa: "Koa",
  hono: "Hono",
};

const ORM_DEPS: Record<string, string> = {
  prisma: "Prisma",
  "@prisma/client": "Prisma",
  typeorm: "TypeORM",
  "drizzle-orm": "Drizzle",
  sequelize: "Sequelize",
};

const TEST_FRAMEWORK_DEPS: Record<string, string> = {
  vitest: "vitest",
  jest: "jest",
  mocha: "mocha",
  ava: "ava",
};

const fingerprintCache = new Map<string, CodebaseFingerprint>();

function loadGitignoreDirs(dir: string): Set<string> {
  const gitignorePath = join(dir, ".gitignore");
  if (!existsSync(gitignorePath)) return new Set();

  try {
    const content = readFileSync(gitignorePath, "utf-8");
    const dirs = new Set<string>();
    for (const raw of content.split("\n")) {
      const line = raw.trim();
      if (!line || line.startsWith("#")) continue;
      // Extract directory name from patterns like "dir/", "dir", "/dir"
      const cleaned = line.replace(/^\//, "").replace(/\/\*.*$/, "").replace(/\/$/, "");
      if (cleaned && !cleaned.includes("*") && !cleaned.includes("?")) {
        dirs.add(cleaned);
      }
    }
    return dirs;
  } catch {
    return new Set();
  }
}

function walkFiles(dir: string, maxDepth: number = 6): string[] {
  const files: string[] = [];
  const gitignoreDirs = loadGitignoreDirs(dir);

  function walk(current: string, depth: number): void {
    if (depth > maxDepth) return;

    let entries: string[];
    try {
      entries = readdirSync(current);
    } catch {
      return;
    }

    for (const entry of entries) {
      if (EXCLUDED_DIRS.has(entry) || gitignoreDirs.has(entry)) continue;
      if (entry.startsWith(".") && entry !== ".mocharc.yml" && entry !== ".mocharc.json") continue;

      const fullPath = join(current, entry);
      let stat;
      try {
        stat = statSync(fullPath);
      } catch {
        continue;
      }

      if (stat.isDirectory()) {
        walk(fullPath, depth + 1);
      } else if (stat.isFile()) {
        files.push(fullPath);
      }
    }
  }

  walk(dir, 0);
  return files.sort();
}

function detectLanguage(files: string[]): DetectedConvention {
  const counts = new Map<string, number>();

  for (const f of files) {
    const base = f.split("/").pop() ?? "";
    if (base.endsWith(".config.js") || base.endsWith(".config.ts") || base.endsWith(".config.mjs")) continue;

    const ext = extname(f);
    const lang = LANGUAGE_MAP[ext];
    if (lang) {
      counts.set(lang, (counts.get(lang) ?? 0) + 1);
    }
  }

  if (counts.size === 0) {
    return { category: "Language", value: "unknown", confidence: "low", evidence: "No recognized source files found" };
  }

  const sorted = [...counts.entries()].sort((a, b) => b[1] - a[1]);
  const total = sorted.reduce((s, [, c]) => s + c, 0);
  const [primary, primaryCount] = sorted[0];
  const pct = Math.round((primaryCount / total) * 100);
  const confidence: ConfidenceLevel = pct >= 80 ? "high" : pct >= 60 ? "medium" : "low";

  const alternatives = sorted.slice(1).map(([lang, count]) => ({
    value: lang,
    evidence: `${count}/${total} source files (${Math.round((count / total) * 100)}%)`,
  }));

  return {
    category: "Language",
    value: primary,
    confidence,
    evidence: `${primaryCount}/${total} source files (${pct}%)`,
    ...(alternatives.length > 0 ? { alternatives } : {}),
  };
}

interface PkgJson {
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
}

function loadPackageJson(dir: string): PkgJson | null {
  const pkgPath = join(dir, "package.json");
  if (!existsSync(pkgPath)) return null;
  try {
    return JSON.parse(readFileSync(pkgPath, "utf-8")) as PkgJson;
  } catch {
    return null;
  }
}

function detectFramework(dir: string, pkg: PkgJson | null): DetectedConvention {
  if (!pkg) {
    // Check for Cargo.toml (Rust) or go.mod (Go)
    if (existsSync(join(dir, "Cargo.toml"))) {
      return { category: "Framework", value: "none", confidence: "medium", evidence: "Rust project (Cargo.toml), no web framework detected" };
    }
    return { category: "Framework", value: "none", confidence: "medium", evidence: "No package.json found" };
  }

  const allDeps = { ...pkg.dependencies, ...pkg.devDependencies };

  for (const [dep, name] of Object.entries(FRAMEWORK_DEPS)) {
    if (dep in allDeps) {
      // Check for config file for extra confidence
      const hasConfig = (
        (name === "Next.js" && (existsSync(join(dir, "next.config.js")) || existsSync(join(dir, "next.config.mjs")) || existsSync(join(dir, "next.config.ts")))) ||
        (name === "Fastify" && existsSync(join(dir, "fastify.config.js")))
      );
      const confidence: ConfidenceLevel = hasConfig ? "high" : "medium";
      const configNote = hasConfig ? " + config file found" : "";
      return { category: "Framework", value: name, confidence, evidence: `"${dep}" found in package.json dependencies${configNote}` };
    }
  }

  return { category: "Framework", value: "none", confidence: "medium", evidence: "No recognized framework in package.json" };
}

function detectOrm(dir: string, pkg: PkgJson | null): DetectedConvention {
  if (!pkg) {
    return { category: "ORM", value: "none", confidence: "medium", evidence: "No package.json found" };
  }

  const allDeps = { ...pkg.dependencies, ...pkg.devDependencies };

  for (const [dep, name] of Object.entries(ORM_DEPS)) {
    if (dep in allDeps) {
      const hasConfig = (
        (name === "Prisma" && existsSync(join(dir, "prisma", "schema.prisma"))) ||
        (name === "TypeORM" && (existsSync(join(dir, "ormconfig.json")) || existsSync(join(dir, "ormconfig.ts"))))
      );
      const confidence: ConfidenceLevel = hasConfig ? "high" : "medium";
      const configNote = hasConfig ? " + config file found" : "";
      return { category: "ORM", value: name, confidence, evidence: `"${dep}" in package.json${configNote}` };
    }
  }

  return { category: "ORM", value: "none", confidence: "medium", evidence: "No recognized ORM in package.json" };
}

function detectTestFramework(dir: string, pkg: PkgJson | null): DetectedConvention {
  if (!pkg) {
    return { category: "Test Framework", value: "none", confidence: "medium", evidence: "No package.json found" };
  }

  const allDeps = { ...pkg.dependencies, ...pkg.devDependencies };

  for (const [dep, name] of Object.entries(TEST_FRAMEWORK_DEPS)) {
    if (dep in allDeps) {
      return { category: "Test Framework", value: name, confidence: "high", evidence: `"${dep}" in package.json dependencies` };
    }
  }

  // Check for config files
  const configChecks: [string, string][] = [
    ["vitest.config.ts", "vitest"],
    ["vitest.config.js", "vitest"],
    ["jest.config.js", "jest"],
    ["jest.config.ts", "jest"],
    [".mocharc.yml", "mocha"],
    [".mocharc.json", "mocha"],
  ];

  for (const [file, name] of configChecks) {
    if (existsSync(join(dir, file))) {
      return { category: "Test Framework", value: name, confidence: "high", evidence: `${file} config found` };
    }
  }

  return { category: "Test Framework", value: "none", confidence: "medium", evidence: "No recognized test framework" };
}

const CAMEL_CASE = /^[a-z][a-zA-Z0-9]*$/;
const SNAKE_CASE = /^[a-z][a-z0-9]*(_[a-z0-9]+)+$/;
const PASCAL_CASE = /^[A-Z][a-zA-Z0-9]*$/;

function detectNamingConvention(files: string[]): DetectedConvention {
  const sourceExts = new Set([".ts", ".tsx", ".js", ".jsx"]);
  const sourceFiles = files
    .filter((f) => sourceExts.has(extname(f)))
    .filter((f) => {
      const base = f.split("/").pop() ?? "";
      return !base.includes(".test.") && !base.includes(".spec.") && !base.endsWith(".config.js") && !base.endsWith(".config.ts");
    })
    .slice(0, 20); // Sample up to 20 files

  if (sourceFiles.length === 0) {
    return { category: "Naming", value: "unknown", confidence: "low", evidence: "No source files to analyze" };
  }

  let camelCount = 0;
  let snakeCount = 0;
  let pascalCount = 0;

  const identifierPattern = /(?:function\s+|const\s+|let\s+|var\s+|class\s+|interface\s+|type\s+|enum\s+|export\s+(?:function\s+|const\s+|let\s+|class\s+|interface\s+|type\s+|enum\s+))([a-zA-Z_][a-zA-Z0-9_]*)/g;

  for (const file of sourceFiles) {
    let content: string;
    try {
      content = readFileSync(file, "utf-8");
    } catch {
      continue;
    }

    let match;
    while ((match = identifierPattern.exec(content)) !== null) {
      const name = match[1];
      if (CAMEL_CASE.test(name)) camelCount++;
      else if (SNAKE_CASE.test(name)) snakeCount++;
      else if (PASCAL_CASE.test(name)) pascalCount++;
    }
  }

  const total = camelCount + snakeCount + pascalCount;
  if (total === 0) {
    return { category: "Naming", value: "unknown", confidence: "low", evidence: "No identifiers found in sampled files" };
  }

  const results: { name: string; count: number }[] = [
    { name: "camelCase", count: camelCount },
    { name: "snake_case", count: snakeCount },
    { name: "PascalCase", count: pascalCount },
  ].sort((a, b) => b.count - a.count);

  const primary = results[0];
  const pct = Math.round((primary.count / total) * 100);
  const confidence: ConfidenceLevel = pct >= 80 ? "high" : pct >= 60 ? "medium" : "low";

  const alternatives = results
    .slice(1)
    .filter((r) => r.count > 0)
    .map((r) => ({
      value: r.name,
      evidence: `${r.count}/${total} identifiers (${Math.round((r.count / total) * 100)}%)`,
    }));

  return {
    category: "Naming",
    value: primary.name,
    confidence,
    evidence: `${primary.count}/${total} identifiers (${pct}%)`,
    ...(alternatives.length > 0 && confidence !== "high" ? { alternatives } : {}),
  };
}

function detectModuleStructure(files: string[]): DetectedConvention {
  const indexFiles = files.filter((f) => {
    const base = f.split("/").pop() ?? "";
    return base === "index.ts" || base === "index.js";
  });

  if (indexFiles.length === 0) {
    return { category: "Module Structure", value: "direct-imports", confidence: "medium", evidence: "No index files found" };
  }

  let barrelCount = 0;
  for (const indexFile of indexFiles) {
    try {
      const content = readFileSync(indexFile, "utf-8");
      if (content.includes("export {") || content.includes("export *")) {
        barrelCount++;
      }
    } catch {
      continue;
    }
  }

  if (barrelCount > 0) {
    return {
      category: "Module Structure",
      value: "barrel-exports",
      confidence: barrelCount >= 3 ? "high" : "medium",
      evidence: `${barrelCount} index files with re-exports`,
    };
  }

  return { category: "Module Structure", value: "direct-imports", confidence: "medium", evidence: `${indexFiles.length} index files without re-exports` };
}

export function scanCodebase(projectPath: string): CodebaseFingerprint {
  const normalizedPath = resolve(projectPath);
  const cached = fingerprintCache.get(normalizedPath);
  if (cached) return cached;

  const files = walkFiles(normalizedPath);
  const pkg = loadPackageJson(normalizedPath);

  const fingerprint: CodebaseFingerprint = {
    projectPath: normalizedPath,
    scannedAt: new Date().toISOString(),
    language: detectLanguage(files),
    framework: detectFramework(normalizedPath, pkg),
    orm: detectOrm(normalizedPath, pkg),
    testFramework: detectTestFramework(normalizedPath, pkg),
    namingConvention: detectNamingConvention(files),
    moduleStructure: detectModuleStructure(files),
    additionalConventions: [],
  };

  fingerprintCache.set(normalizedPath, fingerprint);
  return fingerprint;
}

export function clearFingerprintCache(): void {
  fingerprintCache.clear();
}
