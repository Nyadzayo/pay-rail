import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import type { CompiledPack, CompilationMeta } from "./compiler.js";

export interface LoadedPack {
  pack: CompiledPack;
  meta: CompilationMeta;
}

const packCache = new Map<string, LoadedPack>();

export function loadCompiledPack(
  provider: string,
  basePath: string,
): LoadedPack | null {
  const cacheKey = `${basePath}:${provider}`;
  const cached = packCache.get(cacheKey);
  if (cached) return cached;

  const compiledDir = join(basePath, provider, "compiled");
  const packPath = join(compiledDir, "pack.json");
  const metaPath = join(compiledDir, "meta.json");

  if (!existsSync(packPath)) return null;

  try {
    const packData = JSON.parse(readFileSync(packPath, "utf-8")) as CompiledPack;
    const metaData = existsSync(metaPath)
      ? (JSON.parse(readFileSync(metaPath, "utf-8")) as CompilationMeta)
      : {
          version: packData.version,
          token_count: 0,
          coverage_pct: 0,
          confidence_summary: { generate: 0, verify: 0, refuse_excluded: 0 },
          compiled_at: new Date().toISOString(),
        };

    const loaded: LoadedPack = { pack: packData, meta: metaData };
    packCache.set(cacheKey, loaded);
    return loaded;
  } catch (err) {
    console.warn(
      `[KNOWLEDGE_PACK_LOAD] Failed to load pack for "${provider}" from ${packPath}: ${err instanceof Error ? err.message : String(err)}`,
    );
    return null;
  }
}

export function clearPackCache(): void {
  packCache.clear();
}
