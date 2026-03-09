use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};

use crate::idempotency::key::IdempotencyKey;

/// Errors from idempotency store operations.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum IdempotencyError {
    /// SQLite operation failed.
    #[error(
        "[IDEMPOTENCY_SQLITE_ERROR] SQLite operation failed: {0} [Check database file permissions and path]"
    )]
    Sqlite(String),
    /// Serialization or deserialization failed.
    #[error(
        "[IDEMPOTENCY_SERIALIZATION_ERROR] Failed to serialize/deserialize data: {0} [Ensure record fields are valid]"
    )]
    Serialization(String),
    /// Store is unreachable — caller must defer (fail-closed).
    #[error(
        "[IDEMPOTENCY_STORE_UNAVAILABLE] Store unreachable: {0} [Defer event and retry with exponential backoff]"
    )]
    StoreUnavailable(String),
}

impl From<rusqlite::Error> for IdempotencyError {
    fn from(err: rusqlite::Error) -> Self {
        IdempotencyError::Sqlite(err.to_string())
    }
}

/// A stored idempotency record.
#[derive(Debug, Clone, PartialEq)]
pub struct IdempotencyRecord {
    /// The idempotency key string.
    pub key: String,
    /// When this record was created.
    pub created_at: DateTime<Utc>,
    /// When this record expires and becomes eligible for cleanup.
    pub expires_at: DateTime<Utc>,
    /// The stored processing result (opaque JSON string).
    pub result: String,
}

/// Outcome of a `check_and_store` operation.
#[derive(Debug, Clone, PartialEq)]
pub enum IdempotencyOutcome {
    /// First time seeing this key — it has been stored.
    New,
    /// Key already existed — here is the stored record.
    Duplicate(IdempotencyRecord),
}

/// Pluggable idempotency store for deduplicating payment operations.
///
/// This trait uses return-position `impl Trait` (RPITIT) for async methods,
/// which supports **static dispatch** via generics (`T: IdempotencyStore`).
/// Dynamic dispatch (`dyn IdempotencyStore`) is not supported with this
/// signature. Runtime-selectable backends are deferred to Phase 2.
pub trait IdempotencyStore: Send + Sync {
    /// Check if a key exists and is not expired. Returns `None` if absent or expired.
    fn check(
        &self,
        key: &IdempotencyKey,
    ) -> impl std::future::Future<Output = Result<Option<IdempotencyRecord>, IdempotencyError>> + Send;

    /// Store a key with a result and TTL. Idempotent: storing an existing key is a no-op.
    fn store(
        &self,
        key: &IdempotencyKey,
        result: &str,
        ttl: Duration,
    ) -> impl std::future::Future<Output = Result<(), IdempotencyError>> + Send;

    /// Atomically check for an existing key and store if absent.
    /// Returns `New` if the key was stored, `Duplicate(record)` if it already existed.
    fn check_and_store(
        &self,
        key: &IdempotencyKey,
        result: &str,
        ttl: Duration,
    ) -> impl std::future::Future<Output = Result<IdempotencyOutcome, IdempotencyError>> + Send;

    /// Remove expired keys. Returns the number of keys removed.
    /// This is the ONE exception to append-only: TTL expiry cleanup.
    fn cleanup_expired(
        &self,
    ) -> impl std::future::Future<Output = Result<u64, IdempotencyError>> + Send;
}

/// SQLite-backed idempotency store.
///
/// Uses `Arc<std::sync::Mutex<Connection>>` with `tokio::task::spawn_blocking`
/// to avoid blocking the async runtime during synchronous SQLite I/O.
pub struct SqliteIdempotencyStore {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl SqliteIdempotencyStore {
    /// Open or create an idempotency store at the given path.
    pub fn new(path: &Path) -> Result<Self, IdempotencyError> {
        let conn = rusqlite::Connection::open(path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Create an in-memory idempotency store (for testing).
    pub fn new_in_memory() -> Result<Self, IdempotencyError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), IdempotencyError> {
        let conn = self.lock_conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS idempotency_keys (
                key TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                result TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_idempotency_keys_expires_at
                ON idempotency_keys(expires_at);",
        )?;
        Ok(())
    }

    fn lock_conn(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, IdempotencyError> {
        self.conn
            .lock()
            .map_err(|_| IdempotencyError::StoreUnavailable("Mutex poisoned".to_owned()))
    }
}

impl IdempotencyStore for SqliteIdempotencyStore {
    fn check(
        &self,
        key: &IdempotencyKey,
    ) -> impl std::future::Future<Output = Result<Option<IdempotencyRecord>, IdempotencyError>> + Send
    {
        let conn = self.conn.clone();
        let key_str = key.to_string();
        async move {
            tokio::task::spawn_blocking(move || {
                let conn = conn
                    .lock()
                    .map_err(|_| IdempotencyError::StoreUnavailable("Mutex poisoned".to_owned()))?;
                let now = Utc::now().to_rfc3339();
                let mut stmt = conn.prepare(
                    "SELECT key, created_at, expires_at, result FROM idempotency_keys
                     WHERE key = ?1 AND expires_at > ?2",
                )?;
                let record = stmt
                    .query_row(rusqlite::params![key_str, now], |row| {
                        Ok(IdempotencyRecord {
                            key: row.get(0)?,
                            created_at: parse_datetime(row.get::<_, String>(1)?),
                            expires_at: parse_datetime(row.get::<_, String>(2)?),
                            result: row.get(3)?,
                        })
                    })
                    .optional()?;
                Ok(record)
            })
            .await
            .map_err(|e| IdempotencyError::StoreUnavailable(e.to_string()))?
        }
    }

    fn store(
        &self,
        key: &IdempotencyKey,
        result: &str,
        ttl: Duration,
    ) -> impl std::future::Future<Output = Result<(), IdempotencyError>> + Send {
        let conn = self.conn.clone();
        let key_str = key.to_string();
        let result_str = result.to_owned();
        let now = Utc::now();
        let created_at = now.to_rfc3339();
        let expires_at = (now + ttl).to_rfc3339();
        async move {
            tokio::task::spawn_blocking(move || {
                let conn = conn
                    .lock()
                    .map_err(|_| IdempotencyError::StoreUnavailable("Mutex poisoned".to_owned()))?;
                conn.execute(
                    "INSERT OR IGNORE INTO idempotency_keys (key, created_at, expires_at, result)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![key_str, created_at, expires_at, result_str],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| IdempotencyError::StoreUnavailable(e.to_string()))?
        }
    }

    fn check_and_store(
        &self,
        key: &IdempotencyKey,
        result: &str,
        ttl: Duration,
    ) -> impl std::future::Future<Output = Result<IdempotencyOutcome, IdempotencyError>> + Send
    {
        let conn = self.conn.clone();
        let key_str = key.to_string();
        let result_str = result.to_owned();
        let now = Utc::now();
        let created_at = now.to_rfc3339();
        let expires_at = (now + ttl).to_rfc3339();
        let now_str = now.to_rfc3339();
        async move {
            tokio::task::spawn_blocking(move || {
                let conn = conn
                    .lock()
                    .map_err(|_| IdempotencyError::StoreUnavailable("Mutex poisoned".to_owned()))?;

                // Check for existing non-expired key first
                let existing: Option<IdempotencyRecord> = conn
                    .prepare(
                        "SELECT key, created_at, expires_at, result FROM idempotency_keys
                         WHERE key = ?1 AND expires_at > ?2",
                    )?
                    .query_row(rusqlite::params![key_str, now_str], |row| {
                        Ok(IdempotencyRecord {
                            key: row.get(0)?,
                            created_at: parse_datetime(row.get::<_, String>(1)?),
                            expires_at: parse_datetime(row.get::<_, String>(2)?),
                            result: row.get(3)?,
                        })
                    })
                    .optional()?;

                if let Some(record) = existing {
                    return Ok(IdempotencyOutcome::Duplicate(record));
                }

                // No existing non-expired key — insert (OR IGNORE handles race with expired key still in table)
                conn.execute(
                    "DELETE FROM idempotency_keys WHERE key = ?1 AND expires_at <= ?2",
                    rusqlite::params![key_str, now_str],
                )?;
                conn.execute(
                    "INSERT INTO idempotency_keys (key, created_at, expires_at, result)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![key_str, created_at, expires_at, result_str],
                )?;

                Ok(IdempotencyOutcome::New)
            })
            .await
            .map_err(|e| IdempotencyError::StoreUnavailable(e.to_string()))?
        }
    }

    fn cleanup_expired(
        &self,
    ) -> impl std::future::Future<Output = Result<u64, IdempotencyError>> + Send {
        let conn = self.conn.clone();
        async move {
            tokio::task::spawn_blocking(move || {
                let conn = conn
                    .lock()
                    .map_err(|_| IdempotencyError::StoreUnavailable("Mutex poisoned".to_owned()))?;
                let now = Utc::now().to_rfc3339();
                let count = conn.execute(
                    "DELETE FROM idempotency_keys WHERE expires_at <= ?1",
                    rusqlite::params![now],
                )?;
                Ok(count as u64)
            })
            .await
            .map_err(|e| IdempotencyError::StoreUnavailable(e.to_string()))?
        }
    }
}

/// Parse an RFC 3339 datetime string, falling back to epoch on failure.
fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_default()
}

/// Extension trait for optional query results.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(suffix: &str) -> IdempotencyKey {
        IdempotencyKey::generate("test", "m1", "webhook", suffix).unwrap()
    }

    #[tokio::test]
    async fn store_init_creates_table() {
        let store = SqliteIdempotencyStore::new_in_memory().unwrap();
        let conn = store.lock_conn().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='idempotency_keys'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn check_nonexistent_returns_none() {
        let store = SqliteIdempotencyStore::new_in_memory().unwrap();
        let key = make_key("nonexistent");
        let result = store.check(&key).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn store_and_check_returns_record() {
        let store = SqliteIdempotencyStore::new_in_memory().unwrap();
        let key = make_key("evt_001");
        let ttl = Duration::hours(72);
        store.store(&key, r#"{"status":"ok"}"#, ttl).await.unwrap();
        let record = store.check(&key).await.unwrap().expect("should exist");
        assert_eq!(record.key, "test:m1:webhook:evt_001");
        assert_eq!(record.result, r#"{"status":"ok"}"#);
        assert!(record.expires_at > Utc::now());
    }

    #[tokio::test]
    async fn check_and_store_new_returns_new() {
        let store = SqliteIdempotencyStore::new_in_memory().unwrap();
        let key = make_key("evt_002");
        let ttl = Duration::hours(72);
        let outcome = store
            .check_and_store(&key, r#"{"ok":true}"#, ttl)
            .await
            .unwrap();
        assert_eq!(outcome, IdempotencyOutcome::New);
    }

    #[tokio::test]
    async fn check_and_store_duplicate_returns_duplicate() {
        let store = SqliteIdempotencyStore::new_in_memory().unwrap();
        let key = make_key("evt_003");
        let ttl = Duration::hours(72);
        store
            .check_and_store(&key, r#"{"first":true}"#, ttl)
            .await
            .unwrap();
        let outcome = store
            .check_and_store(&key, r#"{"second":true}"#, ttl)
            .await
            .unwrap();
        match outcome {
            IdempotencyOutcome::Duplicate(record) => {
                assert_eq!(record.result, r#"{"first":true}"#);
            }
            IdempotencyOutcome::New => panic!("expected Duplicate"),
        }
    }

    #[tokio::test]
    async fn expired_key_treated_as_absent() {
        let store = SqliteIdempotencyStore::new_in_memory().unwrap();
        let key = make_key("evt_expired");
        // Insert directly with an already-expired timestamp
        {
            let conn = store.lock_conn().unwrap();
            let past = (Utc::now() - Duration::hours(1)).to_rfc3339();
            conn.execute(
                "INSERT INTO idempotency_keys (key, created_at, expires_at, result) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![key.to_string(), past, past, r#"{"expired":true}"#],
            )
            .unwrap();
        }
        let result = store.check(&key).await.unwrap();
        assert!(result.is_none(), "expired key should be treated as absent");
    }

    #[tokio::test]
    async fn cleanup_expired_removes_old_keys() {
        let store = SqliteIdempotencyStore::new_in_memory().unwrap();
        let key = make_key("evt_cleanup");
        // Insert expired key
        {
            let conn = store.lock_conn().unwrap();
            let past = (Utc::now() - Duration::hours(1)).to_rfc3339();
            conn.execute(
                "INSERT INTO idempotency_keys (key, created_at, expires_at, result) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![key.to_string(), past, past, r#"{"old":true}"#],
            )
            .unwrap();
        }
        let removed = store.cleanup_expired().await.unwrap();
        assert_eq!(removed, 1);
    }

    #[tokio::test]
    async fn cleanup_preserves_unexpired_keys() {
        let store = SqliteIdempotencyStore::new_in_memory().unwrap();
        let key = make_key("evt_fresh");
        store
            .store(&key, r#"{"fresh":true}"#, Duration::hours(72))
            .await
            .unwrap();
        let removed = store.cleanup_expired().await.unwrap();
        assert_eq!(removed, 0);
        let record = store.check(&key).await.unwrap();
        assert!(record.is_some(), "unexpired key should still exist");
    }

    #[tokio::test]
    async fn record_fields_round_trip() {
        let store = SqliteIdempotencyStore::new_in_memory().unwrap();
        let key = make_key("evt_rt");
        let ttl = Duration::hours(48);
        let before = Utc::now();
        store.store(&key, r#"{"round":"trip"}"#, ttl).await.unwrap();
        let record = store.check(&key).await.unwrap().expect("should exist");
        assert_eq!(record.key, "test:m1:webhook:evt_rt");
        assert_eq!(record.result, r#"{"round":"trip"}"#);
        assert!(record.created_at >= before - Duration::seconds(1));
        assert!(record.expires_at > before);
        assert!(record.expires_at <= before + ttl + Duration::seconds(1));
    }

    #[tokio::test]
    async fn store_idempotent_on_existing_key() {
        let store = SqliteIdempotencyStore::new_in_memory().unwrap();
        let key = make_key("evt_idem");
        let ttl = Duration::hours(72);
        store.store(&key, r#"{"first":true}"#, ttl).await.unwrap();
        // Second store with different result is a no-op (INSERT OR IGNORE)
        store.store(&key, r#"{"second":true}"#, ttl).await.unwrap();
        let record = store.check(&key).await.unwrap().expect("should exist");
        assert_eq!(record.result, r#"{"first":true}"#, "first result preserved");
    }
}
