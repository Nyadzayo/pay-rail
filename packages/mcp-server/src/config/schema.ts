import { z } from "zod";

export const ConfidenceThresholdsSchema = z.object({
  generate: z.number().min(0).max(1).default(0.9),
  verify_min: z.number().min(0).max(1).default(0.7),
});

export const PayRailConfigSchema = z.object({
  confidence: ConfidenceThresholdsSchema.default({}),
  token_budget: z.number().int().positive().default(14000),
  knowledge_packs_path: z.string().optional(),
});

export type PayRailConfig = z.infer<typeof PayRailConfigSchema>;
export type ConfidenceThresholds = z.infer<typeof ConfidenceThresholdsSchema>;
