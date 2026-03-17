# Peach Payments API Reference (Fetched 2026-03-09)

## Base URLs
- **Live:** `https://api-v2.peachpayments.com`
- **Sandbox:** `https://testapi-v2.peachpayments.com`
- **Checkout Live:** `https://secure.peachpayments.com`
- **Checkout Sandbox:** `https://testsecure.peachpayments.com`

## Authentication
HTTP Basic Auth with three credentials from Dashboard:
- Entity ID (`entityId`)
- Username (`userId`)
- Password (`password`)

Sent in the request body (not headers) for Payments API.

## Payment Types

| Code | Name | Description |
|------|------|-------------|
| DB | Debit | Debits customer, credits merchant |
| CD | Credit | Credits customer, debits merchant |
| RF | Refund | Credits customer referencing prior debit/capture |
| RV | Reversal | Reverses a processed preauthorisation |
| RC | Receipt | Confirms funds from pre-payment/invoice/bank transfer |
| CB | Chargeback | Negative charge on merchant account |
| CR | Chargeback Reversal | Reverses a chargeback |
| RB | Rebill | Debits customer referencing prior debit |
| PA | Preauthorisation | Stand-alone authorization |
| CP | Capture | Captures a preauthorised amount |
| RG | Registration | Registers card for recurring/one-click |
| DR | Deregistration | Removes card from recurring |
| CF | Confirmation | Confirms a token |
| TE | Token Extension | Extends a token |
| SD | Schedule | Schedules future payment |
| RE | Fraud Check | Performs fraud verification |
| 3D | 3-D Secure | Performs 3-D Secure authentication |

## API Endpoints

### POST /payments
Create a payment (debit, pre-authorization, or credit).

**Parameters:**
- `entityId` (required) - Merchant entity ID
- `amount` (required) - Transaction amount as string (e.g., "92.00")
- `currency` (required) - ISO 4217 currency code (e.g., "ZAR")
- `paymentBrand` (required) - e.g., "VISA", "MASTER", "AMEX"
- `paymentType` (required) - "DB", "PA", "CD"
- `card.number` - Card PAN
- `card.holder` - Cardholder name
- `card.expiryMonth` - 2-digit expiry month
- `card.expiryYear` - 4-digit expiry year
- `card.cvv` - Card CVV

**Response:**
```json
{
  "id": "8ac7a4a2...",
  "result": {
    "code": "000.000.000",
    "description": "Transaction succeeded"
  },
  "buildNumber": "..."
}
```

### GET /payments/{id}
Query payment status.

**Parameters:**
- `entityId` (required) - Merchant entity ID

**Rate Limit:** 2 requests per minute per transaction

### POST /payments/{id}
Capture, refund, or reversal on existing payment.

**Parameters:**
- `entityId` (required)
- `amount` (required for CP/RF)
- `currency` (required for CP/RF)
- `paymentType` (required) - "CP", "RF", or "RV"

### POST /checkouts
Create checkout for hosted payment page.

**Parameters:**
- `entityId` (required)
- `amount` (required)
- `currency` (required)
- `paymentType` (required) - "DB" or "PA"

## Payment Flows

### 1. Direct Debit (DB)
```
POST /payments (paymentType=DB) → 000.000.000 (success) or 000.200.000 (pending)
```
Single-step: authorize + capture atomically.

### 2. Pre-Authorization + Capture (PA → CP)
```
POST /payments (paymentType=PA) → 000.000.000 (authorized)
POST /payments/{id} (paymentType=CP) → 000.000.000 (captured)
```

### 3. Pre-Authorization + Reversal/Void (PA → RV)
```
POST /payments (paymentType=PA) → 000.000.000 (authorized)
POST /payments/{id} (paymentType=RV) → 000.000.000 (voided)
```

### 4. Capture + Refund (PA → CP → RF)
```
POST /payments (paymentType=PA) → authorized
POST /payments/{id} (paymentType=CP) → captured
POST /payments/{id} (paymentType=RF) → refunded
```

### 5. 3D Secure Flow
```
POST /payments (paymentType=DB) → 000.200.000 (pending/redirect)
Customer completes 3DS → redirected to shopperResultUrl
GET /payments/{id} → final status
```

### 6. Async Flow (non-card)
```
POST /payments → 000.200.000 (pending)
Customer completes payment externally
Webhook → final status
```

## Checkout Webhook Event Types

| Event | Description |
|-------|-------------|
| Created | Checkout session initiated |
| Pending | Payment awaiting customer action |
| Successful | Payment completed or refund processed |
| Uncertain | Customer may have cancelled or timed out (30 min) |
| Cancelled | Customer explicitly cancelled |

### State Transitions
- Created → Pending
- Pending → Successful, Cancelled, or Uncertain
- Uncertain or Cancelled → Successful (late settlement)

### Webhook Payload (Successful)
Fields include: `id`, `amount`, `currency`, `paymentBrand`, `paymentType`, `result.code`, `result.description`, `card.bin`, `card.last4Digits`, `card.holder`, `card.expiryMonth`, `card.expiryYear`, `customer.email`, `customer.givenName`, `customer.surname`, `merchantTransactionId`, `checkoutId`, `signature`, `timestamp`

### Supported Payment Brands
VISA, MASTER, DINERS, AMEX, MASTERPASS, MOBICRED, MPESA, 1FORYOU, APLUS, PAYPAL, ZEROPAY, PAYFLEX, BLINKBYEMTEL, CAPITECPAY, MCBJUICE, PEACHEFT, RCS, GOOGLEPAY, FLOAT, SAMSUNGPAY, HAPPYPAY, MAUCAS, MONEYBADGER, PAYSHAP, NEDBANKDIRECTEFT

## Webhook Security (HMAC SHA256)

Activated via support ticket.

**Headers:**
- `x-webhook-signature-algorithm`
- `x-webhook-timestamp`
- `x-webhook-id`
- `x-webhook-signature`

**Message construction:**
```
{timestamp}.{webhookId}.{url}.{payload}
```

**Verification:** HMAC-SHA256 with merchant secret key.

**Retry Policy:** Exponential backoff — 1, 2, 4, 8, 15, 30 min, 1 hr, then 6 hrs daily for 7 days.

## Result Codes (Key Mappings)

### Success (000.xxx.xxx)
| Code | Description | Canonical State |
|------|-------------|-----------------|
| 000.000.000 | Transaction succeeded | captured (DB) / authorized (PA) |
| 000.000.100 | Successful request | success |
| 000.100.110 | Request processed in test mode | authorized |
| 000.100.112 | Processed, requires 3DS redirect | pending_3ds |
| 000.200.000 | Transaction pending | pending |
| 000.200.100 | Checkout created | pending |
| 000.300.000 | Two-step transaction succeeded | captured |
| 000.400.000 | Succeeded but flagged for review | authorized (review) |
| 000.600.000 | Succeeded via external update | captured |

### Failure — External/Bank (800.xxx.xxx)
| Code | Description | Canonical State |
|------|-------------|-----------------|
| 800.100.100 | Declined for unknown reason | failed |
| 800.100.151 | Invalid card | failed |
| 800.100.152 | Declined by authorization system | failed |
| 800.100.153 | Invalid CVV | failed |
| 800.100.155 | Amount exceeds credit | failed |
| 800.100.157 | Wrong expiry date | failed |
| 800.100.158 | Suspecting manipulation | failed |
| 800.100.159 | Stolen card | failed |
| 800.100.160 | Card blocked | failed |
| 800.100.171 | Pick up card | failed |

### Failure — Timeout/Communication (900.xxx.xxx)
| Code | Description | Canonical State |
|------|-------------|-----------------|
| 900.100.100 | Communication error | failed |
| 900.100.300 | Timeout, uncertain result | expired |
| 900.100.301 | Timed out and reversed | voided |
| 900.100.400 | Timeout at connector | expired |
| 900.100.500 | Timeout, try later | expired |
| 900.100.600 | Connector currently down | failed |

### Failure — Validation (100.xxx.xxx, 200.xxx.xxx)
| Code | Description | Canonical State |
|------|-------------|-----------------|
| 100.100.101 | Invalid credit card number | failed |
| 100.100.303 | Expired card | failed |
| 100.100.600 | Empty CVV | failed |
| 100.390.100 | 3D Secure rejected | failed |
| 100.396.101 | Cancelled by user | failed |
| 100.396.103 | Pending transaction timed out | expired |
| 200.100.101 | Invalid XML/request structure | failed |

### Failure — Risk (800.1xx-8xx)
| Code | Description | Canonical State |
|------|-------------|-----------------|
| 800.110.100 | Duplicate transaction | failed |
| 800.120.100 | Rejected by throttling | failed |
| 800.200.159 | Blacklisted (stolen card) | failed |
| 800.200.160 | Blacklisted (blocked) | failed |
| 800.300.401 | BIN blacklisted | failed |
| 800.400.500 | Waiting for non-instant payment | pending |

### Failure — Reference (700.xxx.xxx)
| Code | Description | Canonical State |
|------|-------------|-----------------|
| 700.100.100 | Referenced transaction doesn't exist | failed |
| 700.400.200 | Cannot capture, invalid reference | failed |
| 700.400.530 | Refund exceeds original amount | failed |

### Soft Decline
| Code | Description | Canonical State |
|------|-------------|-----------------|
| 300.100.100 | Additional authentication required (SCA) | pending_3ds |

## HTTP Status Codes
| Code | Meaning |
|------|---------|
| 200 | OK — check result code for outcome |
| 400 | Bad request / payment declined |
| 401 | Wrong authentication |
| 403 | Insufficient permissions |
| 404 | Not found |
| 429 | Rate limited |
| 500 | Server error |

## Sources
- https://developer.peachpayments.com/docs/payments-api-overview
- https://developer.peachpayments.com/docs/payments-api-flows
- https://developer.peachpayments.com/docs/checkout-webhooks
- https://developer.peachpayments.com/docs/reference-payments
- https://developer.peachpayments.com/docs/reference-webhooks
- https://developer.peachpayments.com/docs/dashboard-response-codes
