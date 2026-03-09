import type { FactSource } from "./schema.js";

export const VERIFY_THRESHOLD = 0.7;

const DEFAULT_CONFIDENCE: Record<FactSource, number> = {
  sandbox_test: 0.95,
  official_docs: 0.85,
  historical_docs: 0.7,
  community_report: 0.65,
  inferred: 0.5,
};

const DECAY_RATES: Record<FactSource, number> = {
  sandbox_test: 0.05,
  official_docs: 0.05,
  historical_docs: 0.05,
  community_report: 0.1,
  inferred: 0.1,
};

export function defaultConfidence(source: FactSource): number {
  return DEFAULT_CONFIDENCE[source];
}

export function decayRate(source: FactSource): number {
  return DECAY_RATES[source];
}

export function decayedScore(
  original: number,
  source: FactSource,
  ageMonths: number,
): number {
  if (ageMonths <= 0) {
    return Math.max(0, Math.min(1, original));
  }
  const rate = decayRate(source);
  const decayed = original * Math.pow(1.0 - rate, ageMonths);
  return Math.max(0, Math.min(1, decayed));
}

export function needsReverification(
  original: number,
  source: FactSource,
  ageMonths: number,
): boolean {
  return decayedScore(original, source, ageMonths) < VERIFY_THRESHOLD;
}
