import { describe, expect, it } from "vitest";
import { parseDocumentation } from "../parser.js";

const SAMPLE_DOCS = `
# Peach Payments API Documentation

## Endpoints

### Create Payment
POST /v1/payments
Parameters:
- \`amount\`: Payment amount in cents
- \`currency\`: Three-letter currency code
- \`merchantTransactionId\`: Unique merchant reference

Response: Returns a PaymentResponse JSON object with id and status

### Get Payment
GET /v1/payments/{id}
Returns: The payment object with current status

## Webhook Events

### charge.succeeded
Fired when a charge is successfully processed.
Payload: JSON with resultCode, id, amount

### charge.failed
Triggered when a charge fails due to insufficient funds or card decline.

### refund.completed
Sent when a refund has been processed successfully.

## Status Codes

| Code | Description |
|------|-------------|
| 000.100.110 | Request successfully processed |
| 000.100.112 | Request successfully processed - pending review |
| 800.100.151 | Card expired |
| 800.100.152 | Insufficient funds |
| 800.100.153 | Card declined by issuer |
| 100.400.000 | Transaction timed out |

## Error Codes

| Code | Description |
|------|-------------|
| 800.100.151 | Card expired - cannot process payment |
| 800.100.152 | Insufficient funds on card |
| 800.100.153 | Card declined by issuing bank |
| 800.100.171 | Invalid card number format |
`;

describe("parseDocumentation", () => {
  describe("endpoint extraction", () => {
    it("extracts endpoints from documentation", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      expect(result.endpoints.length).toBeGreaterThanOrEqual(2);
    });

    it("captures HTTP method and URL", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      const post = result.endpoints.find(
        (e) => e.value.method === "POST" && e.value.url === "/v1/payments",
      );
      expect(post).toBeDefined();
      const get = result.endpoints.find(
        (e) =>
          e.value.method === "GET" &&
          e.value.url.includes("/v1/payments"),
      );
      expect(get).toBeDefined();
    });

    it("extracts parameters from nearby context", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      const post = result.endpoints.find(
        (e) => e.value.method === "POST" && e.value.url === "/v1/payments",
      );
      expect(post?.value.parameters).toContain("amount");
      expect(post?.value.parameters).toContain("currency");
    });

    it("assigns confidence from source type", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      for (const ep of result.endpoints) {
        expect(ep.confidence_score).toBe(0.85);
        expect(ep.source).toBe("official_docs");
        expect(ep.decay_rate).toBe(0.05);
      }
    });

    it("assigns higher confidence for sandbox_test source", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "sandbox_test");
      for (const ep of result.endpoints) {
        expect(ep.confidence_score).toBe(0.95);
      }
    });
  });

  describe("webhook event extraction", () => {
    it("extracts webhook events", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      expect(result.webhooks.length).toBeGreaterThanOrEqual(3);
    });

    it("captures event names", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      const eventNames = result.webhooks.map((w) => w.value.event_name);
      expect(eventNames).toContain("charge.succeeded");
      expect(eventNames).toContain("charge.failed");
      expect(eventNames).toContain("refund.completed");
    });

    it("assigns confidence metadata", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      for (const wh of result.webhooks) {
        expect(wh.confidence_score).toBe(0.85);
        expect(wh.source).toBe("official_docs");
      }
    });
  });

  describe("status code extraction", () => {
    it("extracts status codes", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      expect(result.status_codes.length).toBeGreaterThanOrEqual(3);
    });

    it("maps success codes to Captured", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      const success = result.status_codes.find(
        (sc) => sc.value.provider_code === "000.100.110",
      );
      expect(success).toBeDefined();
      expect(success?.value.canonical_state).toBe("Captured");
    });

    it("maps timeout codes to TimedOut", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      const timeout = result.status_codes.find(
        (sc) => sc.value.provider_code === "100.400.000",
      );
      expect(timeout).toBeDefined();
      expect(timeout?.value.canonical_state).toBe("TimedOut");
    });
  });

  describe("error code extraction", () => {
    it("extracts error codes", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      expect(result.errors.length).toBeGreaterThanOrEqual(3);
    });

    it("captures error code and description", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      const expired = result.errors.find(
        (e) => e.value.code === "800.100.151",
      );
      expect(expired).toBeDefined();
      expect(expired?.value.description).toContain("expired");
    });

    it("infers recovery actions", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      const expired = result.errors.find(
        (e) => e.value.code === "800.100.151",
      );
      expect(expired?.value.recovery_action).toContain("card");

      const insufficient = result.errors.find(
        (e) => e.value.code === "800.100.152",
      );
      expect(insufficient?.value.recovery_action).toBeTruthy();
    });
  });

  describe("source type handling", () => {
    it("uses community_report confidence for community sources", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "community_report");
      for (const ep of result.endpoints) {
        expect(ep.confidence_score).toBe(0.65);
        expect(ep.decay_rate).toBe(0.1);
      }
    });
  });

  describe("flow extraction", () => {
    it("extracts flows from headings with steps", () => {
      const text = `
# Payment Flow

## Standard Payment Flow

1. Customer submits payment details
2. System creates a checkout
3. Provider processes the transaction
4. Webhook notifies of result
5. System updates payment status
`;
      const result = parseDocumentation(text, "official_docs");
      expect(result.flows.length).toBeGreaterThanOrEqual(1);
      expect(result.flows[0].value.steps.length).toBeGreaterThanOrEqual(4);
    });

    it("returns no flows for text without flow headings", () => {
      const result = parseDocumentation(SAMPLE_DOCS, "official_docs");
      expect(result.flows).toHaveLength(0);
    });
  });

  describe("empty documentation", () => {
    it("returns empty results for empty text", () => {
      const result = parseDocumentation("", "official_docs");
      expect(result.endpoints).toHaveLength(0);
      expect(result.webhooks).toHaveLength(0);
      expect(result.status_codes).toHaveLength(0);
      expect(result.errors).toHaveLength(0);
      expect(result.flows).toHaveLength(0);
    });

    it("returns empty results for non-API text", () => {
      const result = parseDocumentation(
        "This is a general description of the payment platform with no API details.",
        "official_docs",
      );
      expect(result.endpoints).toHaveLength(0);
    });
  });
});
