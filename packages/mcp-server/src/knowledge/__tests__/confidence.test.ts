import { describe, expect, it } from "vitest";
import {
  VERIFY_THRESHOLD,
  decayRate,
  decayedScore,
  defaultConfidence,
  needsReverification,
} from "../confidence.js";

describe("defaultConfidence", () => {
  it("returns correct defaults for each source", () => {
    expect(defaultConfidence("sandbox_test")).toBe(0.95);
    expect(defaultConfidence("official_docs")).toBe(0.85);
    expect(defaultConfidence("historical_docs")).toBe(0.7);
    expect(defaultConfidence("community_report")).toBe(0.65);
    expect(defaultConfidence("inferred")).toBe(0.5);
  });
});

describe("decayRate", () => {
  it("returns 5% for official sources", () => {
    expect(decayRate("sandbox_test")).toBe(0.05);
    expect(decayRate("official_docs")).toBe(0.05);
    expect(decayRate("historical_docs")).toBe(0.05);
  });

  it("returns 10% for community sources", () => {
    expect(decayRate("community_report")).toBe(0.1);
    expect(decayRate("inferred")).toBe(0.1);
  });
});

describe("decayedScore", () => {
  it("returns original at zero age", () => {
    expect(decayedScore(0.85, "official_docs", 0)).toBe(0.85);
    expect(decayedScore(0.95, "sandbox_test", 0)).toBe(0.95);
  });

  it("returns original at negative age", () => {
    expect(decayedScore(0.85, "official_docs", -1)).toBe(0.85);
  });

  it("decays official docs at 5%/month", () => {
    const result = decayedScore(0.85, "official_docs", 1);
    expect(result).toBeCloseTo(0.85 * 0.95, 10);
  });

  it("decays community sources at 10%/month", () => {
    const result = decayedScore(0.65, "community_report", 1);
    expect(result).toBeCloseTo(0.65 * 0.9, 10);
  });

  it("handles 6 month decay", () => {
    const result = decayedScore(0.85, "official_docs", 6);
    expect(result).toBeCloseTo(0.85 * Math.pow(0.95, 6), 10);
  });

  it("handles 12 month decay", () => {
    const result = decayedScore(0.95, "sandbox_test", 12);
    expect(result).toBeCloseTo(0.95 * Math.pow(0.95, 12), 10);
  });

  it("clamps to zero for extreme age", () => {
    const result = decayedScore(0.5, "inferred", 1000);
    expect(result).toBeGreaterThanOrEqual(0);
  });

  it("community decays faster than official", () => {
    const official = decayedScore(0.85, "official_docs", 6);
    const community = decayedScore(0.85, "community_report", 6);
    expect(official).toBeGreaterThan(community);
  });
});

describe("needsReverification", () => {
  it("fresh official docs do not need reverification", () => {
    expect(needsReverification(0.85, "official_docs", 0)).toBe(false);
  });

  it("aged official docs need reverification", () => {
    expect(needsReverification(0.85, "official_docs", 4)).toBe(true);
  });

  it("community sources start below threshold", () => {
    expect(needsReverification(0.65, "community_report", 0)).toBe(true);
  });

  it("sandbox test stays above threshold initially", () => {
    expect(needsReverification(0.95, "sandbox_test", 0)).toBe(false);
    expect(needsReverification(0.95, "sandbox_test", 3)).toBe(false);
  });

  it("at exact threshold does not need reverification", () => {
    expect(needsReverification(0.7, "official_docs", 0)).toBe(false);
  });

  it("just below threshold needs reverification", () => {
    expect(needsReverification(0.699, "official_docs", 0)).toBe(true);
  });
});

describe("VERIFY_THRESHOLD", () => {
  it("is 0.7", () => {
    expect(VERIFY_THRESHOLD).toBe(0.7);
  });
});
