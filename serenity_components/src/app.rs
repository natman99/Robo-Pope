//! The App: application state, database, registries and interaction routing.

use std::sync::Arc;

use serenity::http::Http;
use serenity::model::application::Interaction;

use crate::db::Database;
use crate::error::{Error, Result};
use crate::modal::ModalRegistry;
use crate::page::PageRegistry;
use crate::value::BoxFuture;

/// The application. Owns the registered Pages and Modals, the database and
/// the interaction router.
///
/// `App` is a cheap handle over shared state: clone it or pass it to
/// registration and rendering wherever application services are required.
/// The App is never stored inside a Page.
pub struct App<E = ()> {
    pub(crate) inner: Arc<AppInner<E>>,
}

/// The App's shared state, reachable through [`App`]'s [`std::ops::Deref`]:
/// `app.db`, `app.pages`, `app.modals`, `app.http`.
pub struct AppInner<E> {
    /// Discord HTTP client.
    pub http: Arc<Http>,
    /// The database: execution-reference store plus the developer's own
    /// data access types (reachable via `Deref`, so `app.db.users` finds
    /// the extension's `users` field).
    pub db: Database<E>,
    /// Registered Pages.
    pub pages: PageRegistry<E>,
    /// Registered Modals.
    pub modals: ModalRegistry<E>,
}

impl<E> Clone for App<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<E> std::ops::Deref for App<E> {
    type Target = AppInner<E>;
    fn deref(&self) -> &AppInner<E> {
        &self.inner
    }
}

impl<E: Send + Sync + 'static> App<E> {
    /// Build an App with a custom database (store backend + extension).
    pub fn new(http: Arc<Http>, db: Database<E>) -> Self {
        Self {
            inner: Arc::new(AppInner {
                http,
                db,
                pages: PageRegistry::new(),
                modals: ModalRegistry::new(),
            }),
        }
    }
}

impl App<()> {
    /// Build an App with in-memory execution storage and no database
    /// extension.
    pub fn memory(http: Arc<Http>) -> Self {
        Self::new(http, Database::memory())
    }
}

impl<E: Send + Sync + 'static> App<E> {
    /// Route a Discord interaction to its execution.
    ///
    /// 1. Read the custom ID.
    /// 2. Resolve the hash to its stored record.
    /// 3. Retrieve the registered Page or Modal.
    /// 4. Resolve the execution ID, deserialize the persisted data, and
    ///    invoke the callback with the interaction context.
    ///
    /// A valid custom ID does not grant permission to perform an operation;
    /// authorization must be checked inside the execution.
    pub fn handle_interaction<'a>(
        &'a self,
        interaction: &'a Interaction,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            match interaction {
                Interaction::Component(i) => {
                    let custom_id = i.data.custom_id.as_str();
                    let record = self.db.store.lookup(custom_id).await?.ok_or_else(|| {
                        Error::UnknownCustomId {
                            custom_id: custom_id.to_string(),
                        }
                    })?;
                    let page = self.pages.erased(&record.def_id)?;
                    page.invoke(self, interaction, &record.exec_id, &record.data_json)
                        .await
                }
                Interaction::Modal(m) => {
                    let custom_id = m.data.custom_id.as_str();
                    let record = self.db.store.lookup(custom_id).await?.ok_or_else(|| {
                        Error::UnknownCustomId {
                            custom_id: custom_id.to_string(),
                        }
                    })?;
                    let modal = self.modals.erased(&record.def_id)?;
                    modal.invoke_submit(self, m, &record.data_json).await
                }
                other => Err(Error::msg(format!(
                    "unsupported interaction type {:?}",
                    other.kind()
                ))),
            }
        })
    }
}
