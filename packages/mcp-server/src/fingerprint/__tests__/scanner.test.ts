import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { scanCodebase, clearFingerprintCache } from "../scanner.js";
import type { CodebaseFingerprint } from "../conventions.js";

const TEST_DIR = join(import.meta.dirname, "__fixtures__", "projects");

function createTsNextPrismaProject(name: string): string {
  const dir = join(TEST_DIR, name);
  mkdirSync(join(dir, "src"), { recursive: true });
  mkdirSync(join(dir, "prisma"), { recursive: true });

  writeFileSync(
    join(dir, "package.json"),
    JSON.stringify({
      name,
      dependencies: { next: "14.0.0", react: "18.0.0", "@prisma/client": "5.0.0" },
      devDependencies: { vitest: "1.0.0", typescript: "5.0.0", prisma: "5.0.0" },
    }),
  );
  writeFileSync(join(dir, "next.config.js"), "module.exports = {};\n");
  writeFileSync(join(dir, "tsconfig.json"), '{"compilerOptions": {"strict": true}}\n');
  writeFileSync(join(dir, "prisma", "schema.prisma"), "generator client {\n  provider = \"prisma-client-js\"\n}\n");

  // Source files with camelCase naming
  writeFileSync(join(dir, "src", "userService.ts"), "export function getUserById(id: string) { return id; }\nexport const maxRetries = 3;\n");
  writeFileSync(join(dir, "src", "paymentHandler.ts"), "export function processPayment(amount: number) { return amount; }\nconst defaultCurrency = 'USD';\n");
  writeFileSync(join(dir, "src", "index.ts"), "export { getUserById } from './userService';\nexport { processPayment } from './paymentHandler';\n");

  return dir;
}

function createExpressJsProject(name: string): string {
  const dir = join(TEST_DIR, name);
  mkdirSync(join(dir, "src"), { recursive: true });

  writeFileSync(
    join(dir, "package.json"),
    JSON.stringify({
      name,
      dependencies: { express: "4.18.0" },
      devDependencies: { jest: "29.0.0" },
    }),
  );
  writeFileSync(join(dir, "jest.config.js"), "module.exports = { testEnvironment: 'node' };\n");
  writeFileSync(join(dir, "src", "app.js"), "const express = require('express');\nconst get_users = () => {};\nconst create_order = () => {};\n");
  writeFileSync(join(dir, "src", "routes.js"), "const handle_request = () => {};\nconst validate_input = () => {};\n");

  return dir;
}

function createRustProject(name: string): string {
  const dir = join(TEST_DIR, name);
  mkdirSync(join(dir, "src"), { recursive: true });

  writeFileSync(
    join(dir, "Cargo.toml"),
    '[package]\nname = "test"\nversion = "0.1.0"\nedition = "2024"\n',
  );
  writeFileSync(join(dir, "src", "main.rs"), "fn main() {\n    println!(\"Hello, world!\");\n}\n");
  writeFileSync(join(dir, "src", "lib.rs"), "pub fn calculate_total() -> u64 { 0 }\n");

  return dir;
}

function createMixedProject(name: string): string {
  const dir = join(TEST_DIR, name);
  mkdirSync(join(dir, "src"), { recursive: true });

  writeFileSync(
    join(dir, "package.json"),
    JSON.stringify({ name, dependencies: {}, devDependencies: { typescript: "5.0.0" } }),
  );
  writeFileSync(join(dir, "tsconfig.json"), '{"compilerOptions": {}}\n');
  // Mix of naming conventions
  writeFileSync(join(dir, "src", "userService.ts"), "export function getUserById() {}\nconst maxRetries = 3;\n");
  writeFileSync(join(dir, "src", "payment_handler.ts"), "export function process_payment() {}\nconst default_currency = 'USD';\n");
  writeFileSync(join(dir, "src", "OrderManager.ts"), "export function CreateOrder() {}\nconst OrderLimit = 100;\n");

  return dir;
}

function createEmptyProject(name: string): string {
  const dir = join(TEST_DIR, name);
  mkdirSync(dir, { recursive: true });
  return dir;
}

describe("scanCodebase", () => {
  beforeEach(() => {
    clearFingerprintCache();
    rmSync(TEST_DIR, { recursive: true, force: true });
  });

  afterEach(() => {
    clearFingerprintCache();
    rmSync(TEST_DIR, { recursive: true, force: true });
  });

  describe("language detection (AC #1)", () => {
    it("detects TypeScript as primary language", () => {
      const dir = createTsNextPrismaProject("ts-project");
      const fp = scanCodebase(dir);

      expect(fp.language.value).toBe("TypeScript");
      expect(fp.language.confidence).toBe("high");
    });

    it("detects JavaScript as primary language", () => {
      const dir = createExpressJsProject("js-project");
      const fp = scanCodebase(dir);

      expect(fp.language.value).toBe("JavaScript");
    });

    it("detects Rust as primary language", () => {
      const dir = createRustProject("rust-project");
      const fp = scanCodebase(dir);

      expect(fp.language.value).toBe("Rust");
    });

    it("returns 'unknown' for empty project", () => {
      const dir = createEmptyProject("empty-project");
      const fp = scanCodebase(dir);

      expect(fp.language.value).toBe("unknown");
      expect(fp.language.confidence).toBe("low");
    });
  });

  describe("framework detection (AC #1)", () => {
    it("detects Next.js from package.json", () => {
      const dir = createTsNextPrismaProject("nextjs-project");
      const fp = scanCodebase(dir);

      expect(fp.framework.value).toBe("Next.js");
      expect(fp.framework.confidence).toBe("high");
    });

    it("detects Express from package.json", () => {
      const dir = createExpressJsProject("express-project");
      const fp = scanCodebase(dir);

      expect(fp.framework.value).toBe("Express");
    });

    it("returns 'none' when no framework found", () => {
      const dir = createRustProject("no-framework");
      const fp = scanCodebase(dir);

      expect(fp.framework.value).toBe("none");
    });
  });

  describe("ORM detection (AC #1)", () => {
    it("detects Prisma from package.json and schema.prisma", () => {
      const dir = createTsNextPrismaProject("prisma-project");
      const fp = scanCodebase(dir);

      expect(fp.orm.value).toBe("Prisma");
      expect(fp.orm.confidence).toBe("high");
    });

    it("returns 'none' when no ORM found", () => {
      const dir = createExpressJsProject("no-orm");
      const fp = scanCodebase(dir);

      expect(fp.orm.value).toBe("none");
    });
  });

  describe("test framework detection (AC #1)", () => {
    it("detects vitest from package.json", () => {
      const dir = createTsNextPrismaProject("vitest-project");
      const fp = scanCodebase(dir);

      expect(fp.testFramework.value).toBe("vitest");
    });

    it("detects jest from package.json", () => {
      const dir = createExpressJsProject("jest-project");
      const fp = scanCodebase(dir);

      expect(fp.testFramework.value).toBe("jest");
    });
  });

  describe("naming convention detection (AC #1, #2, #5)", () => {
    it("detects camelCase in TS project", () => {
      const dir = createTsNextPrismaProject("camel-project");
      const fp = scanCodebase(dir);

      expect(fp.namingConvention.value).toBe("camelCase");
      expect(fp.namingConvention.confidence).toMatch(/high|medium/);
    });

    it("detects snake_case in JS project", () => {
      const dir = createExpressJsProject("snake-project");
      const fp = scanCodebase(dir);

      expect(fp.namingConvention.value).toBe("snake_case");
    });

    it("reports ambiguity for mixed conventions (AC #5)", () => {
      const dir = createMixedProject("mixed-project");
      const fp = scanCodebase(dir);

      expect(fp.namingConvention.confidence).toBe("low");
      expect(fp.namingConvention.alternatives).toBeDefined();
      expect(fp.namingConvention.alternatives!.length).toBeGreaterThan(0);
    });
  });

  describe("module structure detection (AC #1)", () => {
    it("detects barrel exports when index.ts re-exports", () => {
      const dir = createTsNextPrismaProject("barrel-project");
      const fp = scanCodebase(dir);

      expect(fp.moduleStructure.value).toBe("barrel-exports");
    });
  });

  describe("gitignore and exclusions (AC #4)", () => {
    it("excludes node_modules from scan", () => {
      const dir = createTsNextPrismaProject("exclude-project");
      mkdirSync(join(dir, "node_modules", "some-dep"), { recursive: true });
      writeFileSync(join(dir, "node_modules", "some-dep", "index.js"), "module.exports = {};\n");

      const fp = scanCodebase(dir);
      // If node_modules were counted, JS would overtake TS
      expect(fp.language.value).toBe("TypeScript");
    });

    it("respects .gitignore directory exclusions", () => {
      const dir = createTsNextPrismaProject("gitignore-project");
      // Create a .gitignore that excludes "generated/"
      writeFileSync(join(dir, ".gitignore"), "generated/\nvendor\n");
      // Add JS files in the gitignored dirs
      mkdirSync(join(dir, "generated"), { recursive: true });
      mkdirSync(join(dir, "vendor"), { recursive: true });
      writeFileSync(join(dir, "generated", "output.js"), "const a = 1;\n");
      writeFileSync(join(dir, "generated", "types.js"), "const b = 2;\n");
      writeFileSync(join(dir, "generated", "util.js"), "const c = 3;\n");
      writeFileSync(join(dir, "vendor", "lib.js"), "const d = 4;\n");

      const fp = scanCodebase(dir);
      // If gitignored dirs were counted, JS would compete with TS
      expect(fp.language.value).toBe("TypeScript");
    });
  });

  describe("session caching (AC #4)", () => {
    it("returns cached fingerprint for same path", () => {
      const dir = createTsNextPrismaProject("cache-project");
      const first = scanCodebase(dir);
      const second = scanCodebase(dir);

      expect(first).toBe(second); // Same object reference
    });

    it("scans fresh for different path", () => {
      const dir1 = createTsNextPrismaProject("cache-project-1");
      const dir2 = createExpressJsProject("cache-project-2");
      const first = scanCodebase(dir1);
      const second = scanCodebase(dir2);

      expect(first).not.toBe(second);
      expect(first.language.value).toBe("TypeScript");
      expect(second.language.value).toBe("JavaScript");
    });

    it("returns fresh data after cache clear", () => {
      const dir = createTsNextPrismaProject("clear-project");
      const first = scanCodebase(dir);
      clearFingerprintCache();
      const second = scanCodebase(dir);

      expect(first).not.toBe(second);
    });
  });

  describe("confidence levels (AC #2)", () => {
    it("reports confidence for each detected convention", () => {
      const dir = createTsNextPrismaProject("confidence-project");
      const fp = scanCodebase(dir);

      for (const conv of [fp.language, fp.framework, fp.orm, fp.testFramework, fp.namingConvention, fp.moduleStructure]) {
        expect(["high", "medium", "low"]).toContain(conv.confidence);
        expect(conv.evidence.length).toBeGreaterThan(0);
      }
    });
  });

  describe("fingerprint format (AC #2)", () => {
    it("includes all required fields in CodebaseFingerprint", () => {
      const dir = createTsNextPrismaProject("format-project");
      const fp = scanCodebase(dir);

      expect(fp.projectPath).toBe(dir);
      expect(fp.scannedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);
      expect(fp.language).toBeDefined();
      expect(fp.framework).toBeDefined();
      expect(fp.orm).toBeDefined();
      expect(fp.testFramework).toBeDefined();
      expect(fp.namingConvention).toBeDefined();
      expect(fp.moduleStructure).toBeDefined();
    });
  });
});
