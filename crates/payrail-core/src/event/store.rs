use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::event::types::CanonicalEvent;
use crate::id::{EventId, PaymentId};
use crate::payment::types::{Currency, Money, PaymentState};

/// Errors from event store operations.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EventStoreError {
    /// SQLite operation failed.
    #[error(
        "[EVENT_STORE_SQLITE_ERROR] SQLite operation failed: {0} [Check database file permissions and path]"
    )]
    Sqlite(String),
    /// Serialization or deserialization failed.
    #[error(
        "[EVENT_STORE_SERIALIZATION_ERROR] Failed to serialize/deserialize event data: {0} [Ensure event fields are valid]"
    )]
    Serialization(String),
    /// Duplicate event detected (by event_id or idempotency_key).
    #[error(
        "[EVENT_STORE_DUPLICATE] Event already exists with this event_id or idempotency_key [Check for duplicate events before appending]"
    )]
    DuplicateEvent,
}

impl From<rusqlite::Error> for EventStoreError {
    fn from(err: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(ref sqlite_err, _) = err
            && sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation
        {
            return EventStoreError::DuplicateEvent;
        }
        EventStoreError::Sqlite(err.to_string())
    }
}

/// Append-only event store for immutable payment audit trails.
///
/// This trait intentionally omits update and delete methods — append-only
/// semantics are enforced by design. Future backends (PostgreSQL) implement
/// this same trait.
///
/// # Polymorphism
///
/// This trait uses return-position `impl Trait` (RPITIT) for async methods,
/// which supports **static dispatch** via generics (`T: EventStore`).
/// Dynamic dispatch (`dyn EventStore`) is not supported with this signature.
/// Runtime-selectable backends are deferred to Phase 2 when PostgreSQL support
/// is added.
pub trait EventStore: Send + Sync {
    /// Append a canonical event to the store. Returns `DuplicateEvent` if
    /// the event_id or idempotency_key already exists.
    fn append(
        &self,
        event: &CanonicalEvent,
    ) -> impl std::future::Future<Output = Result<(), EventStoreError>> + Send;

    /// Query all events for a payment, ordered chronologically (oldest first).
    fn query_by_payment_id(
        &self,
        id: &PaymentId,
    ) -> impl std::future::Future<Output = Result<Vec<CanonicalEvent>, EventStoreError>> + Send;

    /// Query a single event by its unique event_id.
    fn query_by_event_id(
        &self,
        id: &EventId,
    ) -> impl std::future::Future<Output = Result<Option<CanonicalEvent>, EventStoreError>> + Send;

    /// Returns the optimistic state: `state_after` from the latest event
    /// (by timestamp) for a given payment. This reflects the app's
    /// understanding of the current state.
    fn optimistic_state(
        &self,
        payment_id: &PaymentId,
    ) -> impl std::future::Future<Output = Result<Option<PaymentState>, EventStoreError>> + Send;

    /// Returns the reconciled state: `state_after` from the latest event
    /// where `provider != "app"` (i.e., provider-confirmed transitions only).
    /// Both views are derived from the same immutable event stream.
    fn reconciled_state(
        &self,
        payment_id: &PaymentId,
    ) -> impl std::future::Future<Output = Result<Option<PaymentState>, EventStoreError>> + Send;
}

/// SQLite-backed event store with append-only semantics.
///
/// Uses `std::sync::Mutex` with `tokio::task::spawn_blocking` to avoid
/// blocking the async runtime during synchronous SQLite I/O.
/// Use `new_in_memory()` for testing.
pub struct SqliteEventStore {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl SqliteEventStore {
    const INIT_SQL: &'static str = "
        CREATE TABLE IF NOT EXISTS payment_events (
            event_id TEXT PRIMARY KEY,
            event_type TEXT NOT NULL,
            payment_id TEXT NOT NULL,
            provider TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            state_before TEXT NOT NULL,
            state_after TEXT NOT NULL,
            amount_value INTEGER NOT NULL,
            amount_currency TEXT NOT NULL,
            idempotency_key TEXT NOT NULL,
            raw_provider_payload TEXT NOT NULL,
            metadata TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_payment_events_payment_id ON payment_events(payment_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_payment_events_idempotency_key ON payment_events(idempotency_key);
    ";

    /// Open or create an event store at the given file path.
    pub fn new(path: &Path) -> Result<Self, EventStoreError> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(Self::INIT_SQL)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create an in-memory event store (for testing).
    pub fn new_in_memory() -> Result<Self, EventStoreError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        conn.execute_batch(Self::INIT_SQL)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn lock_conn(
        conn: &Mutex<rusqlite::Connection>,
    ) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, EventStoreError> {
        conn.lock()
            .map_err(|_| EventStoreError::Sqlite("Mutex poisoned".to_owned()))
    }

    fn row_to_event(row: &rusqlite::Row<'_>) -> Result<CanonicalEvent, EventStoreError> {
        let event_id_str: String = row.get(0)?;
        let event_type_str: String = row.get(1)?;
        let payment_id_str: String = row.get(2)?;
        let provider: String = row.get(3)?;
        let timestamp_str: String = row.get(4)?;
        let state_before_str: String = row.get(5)?;
        let state_after_str: String = row.get(6)?;
        let amount_value: i64 = row.get(7)?;
        let amount_currency_str: String = row.get(8)?;
        let idempotency_key: String = row.get(9)?;
        let raw_payload_str: String = row.get(10)?;
        let metadata_str: String = row.get(11)?;

        let event_id: EventId = event_id_str
            .parse()
            .map_err(|e| EventStoreError::Serialization(format!("{e}")))?;
        let event_type = crate::event::types::EventType::new(&event_type_str)
            .map_err(|e| EventStoreError::Serialization(format!("{e}")))?;
        let payment_id: PaymentId = payment_id_str
            .parse()
            .map_err(|e| EventStoreError::Serialization(format!("{e}")))?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .map_err(|e| {
                EventStoreError::Serialization(format!("Invalid timestamp '{timestamp_str}': {e}"))
            })?;
        let state_before: PaymentState = serde_json::from_value(serde_json::Value::String(
            state_before_str.clone(),
        ))
        .map_err(|e| {
            EventStoreError::Serialization(format!(
                "Invalid state_before '{state_before_str}': {e}"
            ))
        })?;
        let state_after: PaymentState = serde_json::from_value(serde_json::Value::String(
            state_after_str.clone(),
        ))
        .map_err(|e| {
            EventStoreError::Serialization(format!("Invalid state_after '{state_after_str}': {e}"))
        })?;
        let currency: Currency =
            serde_json::from_value(serde_json::Value::String(amount_currency_str.clone()))
                .map_err(|e| {
                    EventStoreError::Serialization(format!(
                        "Invalid currency '{amount_currency_str}': {e}"
                    ))
                })?;
        let amount = Money::new(amount_value, currency);
        let raw_provider_payload: serde_json::Value = serde_json::from_str(&raw_payload_str)
            .map_err(|e| {
                EventStoreError::Serialization(format!("Invalid raw_provider_payload JSON: {e}"))
            })?;
        let metadata: serde_json::Value = serde_json::from_str(&metadata_str)
            .map_err(|e| EventStoreError::Serialization(format!("Invalid metadata JSON: {e}")))?;

        Ok(CanonicalEvent {
            event_id,
            event_type,
            payment_id,
            provider,
            timestamp,
            state_before,
            state_after,
            amount,
            idempotency_key,
            raw_provider_payload,
            metadata,
        })
    }

    fn parse_state(state_str: &str) -> Result<PaymentState, EventStoreError> {
        serde_json::from_value(serde_json::Value::String(state_str.to_owned())).map_err(|e| {
            EventStoreError::Serialization(format!("Failed to parse state '{state_str}': {e}"))
        })
    }
}

impl EventStore for SqliteEventStore {
    async fn append(&self, event: &CanonicalEvent) -> Result<(), EventStoreError> {
        let conn = Arc::clone(&self.conn);
        let event_id = event.event_id.as_str().to_owned();
        let event_type = event.event_type.as_str().to_owned();
        let payment_id = event.payment_id.as_str().to_owned();
        let provider = event.provider.clone();
        let timestamp_str = event
            .timestamp
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let state_before_str = event.state_before.to_string();
        let state_after_str = event.state_after.to_string();
        let amount_value = event.amount.value;
        let amount_currency = event.amount.currency.to_string();
        let idempotency_key = event.idempotency_key.clone();
        let raw_payload_str = serde_json::to_string(&event.raw_provider_payload)
            .map_err(|e| EventStoreError::Serialization(e.to_string()))?;
        let metadata_str = serde_json::to_string(&event.metadata)
            .map_err(|e| EventStoreError::Serialization(e.to_string()))?;

        tokio::task::spawn_blocking(move || {
            let conn = Self::lock_conn(&conn)?;
            conn.execute(
                "INSERT INTO payment_events (event_id, event_type, payment_id, provider, timestamp, state_before, state_after, amount_value, amount_currency, idempotency_key, raw_provider_payload, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    &event_id,
                    &event_type,
                    &payment_id,
                    &provider,
                    &timestamp_str,
                    &state_before_str,
                    &state_after_str,
                    amount_value,
                    &amount_currency,
                    &idempotency_key,
                    &raw_payload_str,
                    &metadata_str,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| EventStoreError::Sqlite(format!("spawn_blocking failed: {e}")))?
    }

    async fn query_by_payment_id(
        &self,
        id: &PaymentId,
    ) -> Result<Vec<CanonicalEvent>, EventStoreError> {
        let conn = Arc::clone(&self.conn);
        let id_str = id.as_str().to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = Self::lock_conn(&conn)?;
            let mut stmt = conn.prepare(
                "SELECT event_id, event_type, payment_id, provider, timestamp, state_before, state_after, amount_value, amount_currency, idempotency_key, raw_provider_payload, metadata FROM payment_events WHERE payment_id = ?1 ORDER BY timestamp ASC",
            )?;
            let mut rows = stmt.query([&id_str])?;
            let mut events = Vec::new();
            while let Some(row) = rows.next()? {
                events.push(Self::row_to_event(row)?);
            }
            Ok(events)
        })
        .await
        .map_err(|e| EventStoreError::Sqlite(format!("spawn_blocking failed: {e}")))?
    }

    async fn query_by_event_id(
        &self,
        id: &EventId,
    ) -> Result<Option<CanonicalEvent>, EventStoreError> {
        let conn = Arc::clone(&self.conn);
        let id_str = id.as_str().to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = Self::lock_conn(&conn)?;
            let mut stmt = conn.prepare(
                "SELECT event_id, event_type, payment_id, provider, timestamp, state_before, state_after, amount_value, amount_currency, idempotency_key, raw_provider_payload, metadata FROM payment_events WHERE event_id = ?1",
            )?;
            let mut rows = stmt.query([&id_str])?;
            match rows.next()? {
                Some(row) => Ok(Some(Self::row_to_event(row)?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| EventStoreError::Sqlite(format!("spawn_blocking failed: {e}")))?
    }

    async fn optimistic_state(
        &self,
        payment_id: &PaymentId,
    ) -> Result<Option<PaymentState>, EventStoreError> {
        let conn = Arc::clone(&self.conn);
        let id_str = payment_id.as_str().to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = Self::lock_conn(&conn)?;
            let result = conn.query_row(
                "SELECT state_after FROM payment_events WHERE payment_id = ?1 ORDER BY timestamp DESC LIMIT 1",
                [&id_str],
                |row| row.get::<_, String>(0),
            );
            match result {
                Ok(state_str) => Ok(Some(Self::parse_state(&state_str)?)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await
        .map_err(|e| EventStoreError::Sqlite(format!("spawn_blocking failed: {e}")))?
    }

    async fn reconciled_state(
        &self,
        payment_id: &PaymentId,
    ) -> Result<Option<PaymentState>, EventStoreError> {
        let conn = Arc::clone(&self.conn);
        let id_str = payment_id.as_str().to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = Self::lock_conn(&conn)?;
            let result = conn.query_row(
                "SELECT state_after FROM payment_events WHERE payment_id = ?1 AND provider != 'app' ORDER BY timestamp DESC LIMIT 1",
                [&id_str],
                |row| row.get::<_, String>(0),
            );
            match result {
                Ok(state_str) => Ok(Some(Self::parse_state(&state_str)?)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await
        .map_err(|e| EventStoreError::Sqlite(format!("spawn_blocking failed: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::types::EventType;
    use chrono::{Duration, Utc};

    fn sample_event() -> CanonicalEvent {
        CanonicalEvent {
            event_id: EventId::new(),
            event_type: EventType::new("payment.charge.created").unwrap(),
            payment_id: PaymentId::new(),
            provider: "peach_payments".to_owned(),
            timestamp: Utc::now(),
            state_before: PaymentState::Created,
            state_after: PaymentState::Authorized,
            amount: Money::new(15000, Currency::ZAR),
            idempotency_key: format!("peach:m1:webhook:{}", EventId::new()),
            raw_provider_payload: serde_json::json!({"status": "approved"}),
            metadata: serde_json::json!({"order_id": "ORD-001"}),
        }
    }

    fn sample_event_with(
        payment_id: &PaymentId,
        state_before: PaymentState,
        state_after: PaymentState,
        provider: &str,
        timestamp: chrono::DateTime<Utc>,
    ) -> CanonicalEvent {
        CanonicalEvent {
            event_id: EventId::new(),
            event_type: EventType::new("payment.charge.captured").unwrap(),
            payment_id: payment_id.clone(),
            provider: provider.to_owned(),
            timestamp,
            state_before,
            state_after,
            amount: Money::new(15000, Currency::ZAR),
            idempotency_key: format!("test:m1:webhook:{}", EventId::new()),
            raw_provider_payload: serde_json::json!({}),
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn event_store_init_creates_table() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let conn = store.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='payment_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn append_and_query_by_event_id() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let event = sample_event();
        let event_id = event.event_id.clone();

        store.append(&event).await.unwrap();
        let result = store.query_by_event_id(&event_id).await.unwrap();

        assert!(result.is_some());
        let found = result.unwrap();
        assert_eq!(found.event_id, event_id);
    }

    #[tokio::test]
    async fn append_and_query_by_payment_id() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = PaymentId::new();
        let now = Utc::now();

        let e1 = sample_event_with(
            &pid,
            PaymentState::Created,
            PaymentState::Authorized,
            "peach",
            now,
        );
        let e2 = sample_event_with(
            &pid,
            PaymentState::Authorized,
            PaymentState::Captured,
            "peach",
            now + Duration::seconds(1),
        );

        store.append(&e1).await.unwrap();
        store.append(&e2).await.unwrap();

        let events = store.query_by_payment_id(&pid).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].state_after, PaymentState::Authorized);
        assert_eq!(events[1].state_after, PaymentState::Captured);
    }

    #[tokio::test]
    async fn query_nonexistent_event_returns_none() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let result = store.query_by_event_id(&EventId::new()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn query_nonexistent_payment_returns_empty() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let result = store.query_by_payment_id(&PaymentId::new()).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn duplicate_event_id_rejected() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let event = sample_event();

        store.append(&event).await.unwrap();
        let result = store.append(&event).await;

        assert_eq!(result, Err(EventStoreError::DuplicateEvent));
    }

    #[tokio::test]
    async fn duplicate_idempotency_key_rejected() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let e1 = sample_event();
        let mut e2 = sample_event();
        e2.event_id = EventId::new(); // Different event_id
        e2.idempotency_key = e1.idempotency_key.clone(); // Same idempotency_key

        store.append(&e1).await.unwrap();
        let result = store.append(&e2).await;

        assert_eq!(result, Err(EventStoreError::DuplicateEvent));
    }

    #[tokio::test]
    async fn optimistic_state_returns_latest() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = PaymentId::new();
        let now = Utc::now();

        let e1 = sample_event_with(
            &pid,
            PaymentState::Created,
            PaymentState::Authorized,
            "peach",
            now,
        );
        let e2 = sample_event_with(
            &pid,
            PaymentState::Authorized,
            PaymentState::Captured,
            "app",
            now + Duration::seconds(1),
        );

        store.append(&e1).await.unwrap();
        store.append(&e2).await.unwrap();

        let state = store.optimistic_state(&pid).await.unwrap();
        assert_eq!(state, Some(PaymentState::Captured));
    }

    #[tokio::test]
    async fn reconciled_state_excludes_app_events() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = PaymentId::new();
        let now = Utc::now();

        let e1 = sample_event_with(
            &pid,
            PaymentState::Created,
            PaymentState::Authorized,
            "peach",
            now,
        );
        let e2 = sample_event_with(
            &pid,
            PaymentState::Authorized,
            PaymentState::Captured,
            "app",
            now + Duration::seconds(1),
        );

        store.append(&e1).await.unwrap();
        store.append(&e2).await.unwrap();

        // Reconciled ignores "app" events, so latest provider-confirmed = Authorized
        let state = store.reconciled_state(&pid).await.unwrap();
        assert_eq!(state, Some(PaymentState::Authorized));
    }

    #[tokio::test]
    async fn event_fields_round_trip() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let event = sample_event();
        let eid = event.event_id.clone();

        store.append(&event).await.unwrap();
        let found = store.query_by_event_id(&eid).await.unwrap().unwrap();

        assert_eq!(found.event_id, event.event_id);
        assert_eq!(found.event_type, event.event_type);
        assert_eq!(found.payment_id, event.payment_id);
        assert_eq!(found.provider, event.provider);
        // Timestamps may lose sub-millisecond precision due to RFC3339 millis format
        assert_eq!(
            found.timestamp.timestamp_millis(),
            event.timestamp.timestamp_millis()
        );
        assert_eq!(found.state_before, event.state_before);
        assert_eq!(found.state_after, event.state_after);
        assert_eq!(found.amount, event.amount);
        assert_eq!(found.idempotency_key, event.idempotency_key);
        assert_eq!(found.raw_provider_payload, event.raw_provider_payload);
        assert_eq!(found.metadata, event.metadata);
    }

    #[tokio::test]
    async fn ordering_is_chronological() {
        let store = SqliteEventStore::new_in_memory().unwrap();
        let pid = PaymentId::new();
        let now = Utc::now();

        // Insert out of order: t3, t1, t2
        let e3 = sample_event_with(
            &pid,
            PaymentState::Captured,
            PaymentState::Refunded,
            "peach",
            now + Duration::seconds(3),
        );
        let e1 = sample_event_with(
            &pid,
            PaymentState::Created,
            PaymentState::Authorized,
            "peach",
            now + Duration::seconds(1),
        );
        let e2 = sample_event_with(
            &pid,
            PaymentState::Authorized,
            PaymentState::Captured,
            "peach",
            now + Duration::seconds(2),
        );

        store.append(&e3).await.unwrap();
        store.append(&e1).await.unwrap();
        store.append(&e2).await.unwrap();

        let events = store.query_by_payment_id(&pid).await.unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].state_after, PaymentState::Authorized);
        assert_eq!(events[1].state_after, PaymentState::Captured);
        assert_eq!(events[2].state_after, PaymentState::Refunded);
    }
}
