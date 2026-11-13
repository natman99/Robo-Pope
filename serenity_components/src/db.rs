//! Persistence of execution references and the user-facing database handle.
//!
//! The framework stores one record per interactive execution: the
//! deterministic BLAKE3 hash used as the Discord custom ID, the globally
//! unique Page/Modal ID, the execution ID unique within it, and the
//! serialized data of the rendered instance. Closures are never serialized;
//! the registered definition holds the callbacks and the database only holds
//! what is needed to locate them.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::error::{Error, Result};
use crate::value::BoxFuture;

/// A stored execution reference.
#[derive(Debug, Clone)]
pub struct StoreRecord {
    /// Globally unique Page or Modal ID.
    pub def_id: String,
    /// Execution ID, unique within that definition.
    pub exec_id: String,
    /// Serialized data associated with the rendered instance.
    pub data_json: Vec<u8>,
}

/// Storage backend for execution references, keyed by custom ID hash.
pub trait Store: Send + Sync + 'static {
    /// Insert or replace the record for `custom_id`.
    ///
    /// Repeated renders of the same definition and data produce the same
    /// hash, so this is expected to be idempotent.
    fn insert<'a>(&'a self, custom_id: &'a str, record: StoreRecord) -> BoxFuture<'a, Result<()>>;

    /// Resolve a custom ID to its record.
    fn lookup<'a>(&'a self, custom_id: &'a str) -> BoxFuture<'a, Result<Option<StoreRecord>>>;

    /// Remove the record for `custom_id`.
    fn delete<'a>(&'a self, custom_id: &'a str) -> BoxFuture<'a, Result<()>>;
}

/// In-memory [`Store`] implementation.
#[derive(Debug, Default)]
pub struct MemoryStore {
    map: RwLock<HashMap<String, StoreRecord>>,
}

impl Store for MemoryStore {
    fn insert<'a>(&'a self, custom_id: &'a str, record: StoreRecord) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.map
                .write()
                .map_err(|_| Error::msg("store lock poisoned"))?
                .insert(custom_id.to_string(), record);
            Ok(())
        })
    }

    fn lookup<'a>(&'a self, custom_id: &'a str) -> BoxFuture<'a, Result<Option<StoreRecord>>> {
        Box::pin(async move {
            Ok(self
                .map
                .read()
                .map_err(|_| Error::msg("store lock poisoned"))?
                .get(custom_id)
                .cloned())
        })
    }

    fn delete<'a>(&'a self, custom_id: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.map
                .write()
                .map_err(|_| Error::msg("store lock poisoned"))?
                .remove(custom_id);
            Ok(())
        })
    }
}

/// The deterministic custom ID for an execution: `BLAKE3(def_id || 0xff ||
/// exec_id || 0xff || data_json)` as lowercase hex (64 chars, within
/// Discord's 100-char custom ID limit).
///
/// The same definition, execution and data always produce the same hash.
#[must_use]
pub fn custom_id_hash(def_id: &str, exec_id: &str, data_json: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(def_id.as_bytes());
    hasher.update(&[0xff]);
    hasher.update(exec_id.as_bytes());
    hasher.update(&[0xff]);
    hasher.update(data_json);
    hasher.finalize().to_hex().to_string()
}

/// The application database: the framework's execution-reference store plus
/// the developer's own data access types.
///
/// `E` is the developer's extension type; it is reachable through
/// [`std::ops::Deref`], so `app.db.users.query(..)` resolves to the `users` field of
/// the extension.
pub struct Database<E> {
    /// Framework execution-reference storage.
    pub store: Arc<dyn Store>,
    /// Developer data access types.
    pub ext: E,
}

impl<E> std::ops::Deref for Database<E> {
    type Target = E;
    fn deref(&self) -> &E {
        &self.ext
    }
}

impl<E> Database<E> {
    /// Build a database with a custom store backend.
    pub fn new(store: Arc<dyn Store>, ext: E) -> Self {
        Self { store, ext }
    }
}

impl Database<()> {
    /// A database with in-memory execution storage and no extension.
    #[must_use]
    pub fn memory() -> Self {
        Self {
            store: Arc::new(MemoryStore::default()),
            ext: (),
        }
    }
}

impl Database<()> {
    /// A database with in-memory execution storage and the given extension.
    pub fn with_ext<E>(ext: E) -> Database<E> {
        Database {
            store: Arc::new(MemoryStore::default()),
            ext,
        }
    }
}
