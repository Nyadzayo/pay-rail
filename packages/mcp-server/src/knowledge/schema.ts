import { z } from "zod";

export const FactSourceSchema = z.enum([
  "sandbox_test",
  "official_docs",
  "historical_docs",
  "community_report",
  "inferred",
]);
export type FactSource = z.infer<typeof FactSourceSchema>;

export const ConfidenceScoreSchema = z
  .number()
  .min(0.0)
  .max(1.0)
  .describe("Confidence score between 0.0 and 1.0");
export type ConfidenceScore = z.infer<typeof ConfidenceScoreSchema>;

export const EndpointFactSchema = z.object({
  url: z.string(),
  method: z.string(),
  parameters: z.array(z.string()),
  response_schema: z.string(),
  description: z.string(),
});
export type EndpointFact = z.infer<typeof EndpointFactSchema>;

export const WebhookEventFactSchema = z.object({
  event_name: z.string(),
  payload_schema: z.string(),
  trigger_conditions: z.string(),
  description: z.string(),
});
export type WebhookEventFact = z.infer<typeof WebhookEventFactSchema>;

export const StatusCodeMappingSchema = z.object({
  provider_code: z.string(),
  canonical_state: z.string(),
  description: z.string(),
});
export type StatusCodeMapping = z.infer<typeof StatusCodeMappingSchema>;

export const ErrorCodeFactSchema = z.object({
  code: z.string(),
  description: z.string(),
  recovery_action: z.string(),
});
export type ErrorCodeFact = z.infer<typeof ErrorCodeFactSchema>;

export const PaymentFlowSequenceSchema = z.object({
  name: z.string(),
  steps: z.array(z.string()),
  description: z.string(),
});
export type PaymentFlowSequence = z.infer<typeof PaymentFlowSequenceSchema>;

function factEntrySchema<T extends z.ZodTypeAny>(valueSchema: T) {
  return z.object({
    value: valueSchema,
    confidence_score: ConfidenceScoreSchema,
    source: FactSourceSchema,
    verification_date: z.string().datetime(),
    decay_rate: z.number().min(0.0).max(1.0),
  });
}

export const EndpointFactEntrySchema = factEntrySchema(EndpointFactSchema);
export type EndpointFactEntry = z.infer<typeof EndpointFactEntrySchema>;

export const WebhookEventFactEntrySchema = factEntrySchema(
  WebhookEventFactSchema,
);
export type WebhookEventFactEntry = z.infer<
  typeof WebhookEventFactEntrySchema
>;

export const StatusCodeMappingEntrySchema = factEntrySchema(
  StatusCodeMappingSchema,
);
export type StatusCodeMappingEntry = z.infer<
  typeof StatusCodeMappingEntrySchema
>;

export const ErrorCodeFactEntrySchema = factEntrySchema(ErrorCodeFactSchema);
export type ErrorCodeFactEntry = z.infer<typeof ErrorCodeFactEntrySchema>;

export const PaymentFlowSequenceEntrySchema = factEntrySchema(
  PaymentFlowSequenceSchema,
);
export type PaymentFlowSequenceEntry = z.infer<
  typeof PaymentFlowSequenceEntrySchema
>;

export const ProviderMetadataSchema = z.object({
  name: z.string(),
  display_name: z.string(),
  version: z.string(),
  base_url: z.string(),
  sandbox_url: z.string(),
  documentation_url: z.string(),
});
export type ProviderMetadata = z.infer<typeof ProviderMetadataSchema>;

export const KnowledgePackSchema = z.object({
  metadata: ProviderMetadataSchema,
  endpoints: z.array(EndpointFactEntrySchema),
  webhooks: z.array(WebhookEventFactEntrySchema),
  status_codes: z.array(StatusCodeMappingEntrySchema),
  errors: z.array(ErrorCodeFactEntrySchema),
  flows: z.array(PaymentFlowSequenceEntrySchema),
});
export type KnowledgePack = z.infer<typeof KnowledgePackSchema>;

export type FactCategory =
  | "endpoints"
  | "webhooks"
  | "status_codes"
  | "errors"
  | "flows";

export const FACT_CATEGORIES: readonly FactCategory[] = [
  "endpoints",
  "webhooks",
  "status_codes",
  "errors",
  "flows",
] as const;
