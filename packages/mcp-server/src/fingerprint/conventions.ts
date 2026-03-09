export type ConfidenceLevel = "high" | "medium" | "low";

export interface DetectedConvention {
  category: string;
  value: string;
  confidence: ConfidenceLevel;
  evidence: string;
  alternatives?: { value: string; evidence: string }[];
}

export interface CodebaseFingerprint {
  projectPath: string;
  scannedAt: string;
  language: DetectedConvention;
  framework: DetectedConvention;
  orm: DetectedConvention;
  testFramework: DetectedConvention;
  namingConvention: DetectedConvention;
  moduleStructure: DetectedConvention;
  additionalConventions: DetectedConvention[];
}

function escapeCell(text: string): string {
  return String(text ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
}

export function formatFingerprintAsMarkdown(fp: CodebaseFingerprint): string {
  const rows = [
    fp.language,
    fp.framework,
    fp.orm,
    fp.testFramework,
    fp.namingConvention,
    fp.moduleStructure,
    ...fp.additionalConventions,
  ];

  const lines = [
    `**Codebase Fingerprint** (scanned: ${fp.scannedAt.slice(0, 10)})\n`,
    "| Category | Detected | Confidence | Evidence |",
    "|----------|----------|------------|----------|",
  ];

  for (const r of rows) {
    lines.push(`| ${escapeCell(r.category)} | ${escapeCell(r.value)} | ${r.confidence} | ${escapeCell(r.evidence)} |`);
    if (r.alternatives && r.alternatives.length > 0) {
      for (const alt of r.alternatives) {
        lines.push(`| | _alt:_ ${escapeCell(alt.value)} | | ${escapeCell(alt.evidence)} |`);
      }
    }
  }

  return lines.join("\n");
}
