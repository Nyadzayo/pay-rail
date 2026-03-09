import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts", "src/cli/serve.ts", "src/cli/ingest.ts", "src/cli/validate.ts", "src/cli/compile.ts"],
  format: ["esm", "cjs"],
  dts: true,
  sourcemap: true,
  clean: true,
});
