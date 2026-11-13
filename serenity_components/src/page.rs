//! Pages: blueprints for Discord messages, with registration, rendering and
//! frame pagination.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serenity::builder::{CreateActionRow, CreateButton, CreateComponent};
use serenity::model::application::{ButtonStyle, ComponentInteractionDataKind, Interaction};
use serenity::model::id::{GenericChannelId, GenericId, RoleId, UserId};

use crate::app::App;
use crate::callback::{
    ChannelChoice, ErasedExec, FrameSlot, MentionableChoice, RoleChoice, UserChoice,
};
use crate::component::{
    Button, ButtonRow, Component, Container, Cost, File, MediaGallery, Resolved, Section,
    Separator, Text, limits,
};
use crate::context::{Ctx, RenderCtx};
use crate::db::{StoreRecord, custom_id_hash};
use crate::error::{Error, Result};
use crate::render::RenderedMessage;
use crate::select::SelectMenu;
use crate::value::{BoxFuture, Slot, Value};

/// Reserved framework execution IDs for pagination controls.
pub(crate) const NAV_PREV: &str = "__frame_prev";
pub(crate) const NAV_NEXT: &str = "__frame_next";

/// One entry in a Page's fixed structure: a component or a Frame.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum PageItem<T, E> {
    Comp(Component<T, E>),
    Frame(FrameSlot<T, E>),
}

/// The persisted render state of a Page instance: the Page's original data
/// plus the Frame's current pagination position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageData<T> {
    /// The Page's original data.
    pub data: T,
    /// The current Frame page index (0 when the Page has no Frame).
    pub frame_page: usize,
}

/// A Page: a reusable blueprint for a Discord message, parameterized by its
/// data. Pages are built before they are registered; registration consumes
/// the builder and produces a [`RegisteredPage`].
#[derive(Debug)]
pub struct Page<T, E = ()> {
    pub(crate) id: String,
    pub(crate) items: Vec<PageItem<T, E>>,
    pub(crate) paginate: bool,
}

impl<T, E> Page<T, E>
where
    T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    /// A Page with a globally unique ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            paginate: false,
        }
    }

    /// Add a Text Display (static or dynamic). Each call adds a separate
    /// Text Display; text is never concatenated.
    #[must_use]
    pub fn add_text(mut self, content: impl Into<Value<String, T, E>>) -> Self {
        self.items.push(PageItem::Comp(Component::Text(Text {
            content: content.into(),
        })));
        self
    }

    /// Add a Section.
    #[must_use]
    pub fn add_section(mut self, section: Section<T, E>) -> Self {
        self.items.push(PageItem::Comp(Component::Section(section)));
        self
    }

    /// Add a Container.
    #[must_use]
    pub fn add_container(mut self, container: Container<T, E>) -> Self {
        self.items
            .push(PageItem::Comp(Component::Container(container)));
        self
    }

    /// Add a separator.
    #[must_use]
    pub fn add_separator(mut self) -> Self {
        self.items
            .push(PageItem::Comp(Component::Separator(Separator::new())));
        self
    }

    /// Add a media gallery.
    #[must_use]
    pub fn add_media_gallery(mut self, gallery: MediaGallery<T, E>) -> Self {
        self.items
            .push(PageItem::Comp(Component::MediaGallery(gallery)));
        self
    }

    /// Add a file component.
    #[must_use]
    pub fn add_file(mut self, file: File) -> Self {
        self.items.push(PageItem::Comp(Component::File(file)));
        self
    }

    /// Add a fixed five-slot button row; the framework creates the Action
    /// Row. Empty slots are `None`.
    #[must_use]
    pub fn add_button_row(
        mut self,
        row: (
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
        ),
    ) -> Self {
        self.items
            .push(PageItem::Comp(Component::ButtonRow(ButtonRow([
                row.0.into(),
                row.1.into(),
                row.2.into(),
                row.3.into(),
                row.4.into(),
            ]))));
        self
    }

    /// Add a selection menu; the menu determines its own type.
    #[must_use]
    pub fn add_selection_menu(mut self, menu: impl Into<SelectMenu<T, E>>) -> Self {
        self.items
            .push(PageItem::Comp(Component::Select(menu.into())));
        self
    }

    /// Add a standalone button (placed in its own Action Row).
    #[must_use]
    pub fn add_button(mut self, button: Button<T, E>) -> Self {
        self.items.push(PageItem::Comp(Component::Button(button)));
        self
    }

    /// Add a Frame: a dynamic region resolved at render time.
    #[must_use]
    pub fn add_frame(mut self, frame: impl Into<FrameSlot<T, E>>) -> Self {
        self.items.push(PageItem::Frame(frame.into()));
        self
    }

    /// Enable automatic pagination of Frame contents, including framework-
    /// provided navigation controls.
    #[must_use]
    pub fn paginate(mut self) -> Self {
        self.paginate = true;
        self
    }

    /// Register this Page with the App. Registration consumes the builder;
    /// the registered definition is immutable from here on.
    pub fn register(self, app: &App<E>) -> Result<RegisteredPage<T, E>> {
        // Validate execution identities.
        let mut ids = Vec::new();
        for item in &self.items {
            if let PageItem::Comp(c) = item {
                c.exec_ids(&mut ids);
            }
        }
        for id in &ids {
            if id.starts_with("__") {
                return Err(Error::ReservedExecId {
                    page_id: self.id.clone(),
                    exec_id: id.clone(),
                });
            }
        }
        let mut execs: HashMap<String, Arc<dyn ErasedExec<E>>> = HashMap::new();
        for item in &self.items {
            if let PageItem::Comp(c) = item {
                let mut pairs = Vec::new();
                c.collect_execs(&mut pairs);
                for (exec_id, exec) in pairs {
                    if execs.contains_key(&exec_id) {
                        return Err(Error::DuplicateExecId {
                            page_id: self.id.clone(),
                            exec_id,
                        });
                    }
                    execs.insert(exec_id, exec);
                }
            }
        }
        let def = Arc::new(PageDef {
            id: self.id.clone(),
            paginate: self.paginate,
            items: self.items,
            execs: Mutex::new(execs),
        });
        app.pages.insert(&def.id, def.clone())?;
        Ok(RegisteredPage { def })
    }
}

/// A registered, immutable Page definition.
pub struct PageDef<T, E = ()> {
    pub(crate) id: String,
    pub(crate) paginate: bool,
    pub(crate) items: Vec<PageItem<T, E>>,
    pub(crate) execs: Mutex<HashMap<String, Arc<dyn ErasedExec<E>>>>,
}

/// Type-erased Page operations used for rendering from persisted data and
/// for interaction routing. The App does not need to know the generic data
/// type; the registered Page handles deserialization internally.
pub trait PageErased<E>: Send + Sync {
    /// The globally unique Page ID.
    fn page_id(&self) -> &str;

    /// Resolve and invoke the execution identified by `exec_id` with the
    /// persisted `data_json`.
    fn invoke<'a>(
        &'a self,
        app: &'a App<E>,
        interaction: &'a Interaction,
        exec_id: &'a str,
        data_json: &'a [u8],
    ) -> BoxFuture<'a, Result<()>>;

    /// Render every page from persisted [`PageData`] JSON.
    fn render_json<'a>(
        &'a self,
        app: &'a App<E>,
        data_json: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<RenderedMessage>>>;
}

impl<T, E> PageDef<T, E>
where
    T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn register_runtime_execs(&self, pairs: Vec<(String, Arc<dyn ErasedExec<E>>)>) {
        if let Ok(mut execs) = self.execs.lock() {
            for (id, exec) in pairs {
                execs.insert(id, exec);
            }
        }
    }

    /// The full render pipeline: resolve dynamic values, expand Frames,
    /// paginate, persist execution references, and produce one rendered
    /// message per generated page.
    pub async fn render_all<'a>(
        &'a self,
        app: &'a App<E>,
        data: T,
        frame_page: usize,
    ) -> Result<Vec<RenderedMessage>> {
        let pd = PageData { data, frame_page };
        let data_json = serde_json::to_vec(&pd)?;
        let rctx = RenderCtx::new(app, &self.id, &data_json);

        // Pass 1: resolve every item into one or more page variants.
        let mut variants: Vec<Vec<Resolved>> = Vec::new();
        let mut frame_items = 0usize;
        for item in &self.items {
            match item {
                PageItem::Comp(c) => variants.push(c.resolve(&rctx, &pd.data).await?),
                PageItem::Frame(f) => {
                    frame_items += 1;
                    if frame_items > 1 {
                        return Err(Error::MultipleFrames {
                            page_id: self.id.clone(),
                        });
                    }
                    let elements = (f.f)(&rctx, pd.data.clone()).await.map_err(|source| {
                        Error::FrameFailed {
                            page_id: self.id.clone(),
                            source: Box::new(source),
                        }
                    })?;
                    // Frame executions keep developer-defined identities,
                    // stable across pagination and rerendering.
                    let mut pairs = Vec::new();
                    for el in &elements {
                        el.as_component().collect_execs(&mut pairs);
                    }
                    self.register_runtime_execs(pairs);
                    let mut resolved = Vec::new();
                    for el in elements {
                        resolved.extend(el.into_component().resolve(&rctx, &pd.data).await?);
                    }
                    variants.push(resolved);
                }
            }
        }

        let multi_count = variants.iter().filter(|v| v.len() > 1).count();
        if multi_count > 1 {
            return Err(Error::MultipleFrames {
                page_id: self.id.clone(),
            });
        }

        // Static cost: items that contribute the same components to every
        // page.
        let mut static_cost = Cost::default();
        for v in &variants {
            if v.len() == 1 {
                static_cost = static_cost + v[0].cost;
            }
        }

        let has_frame = frame_items > 0;
        let nav_cost = Cost {
            components: 3,
            chars: 0,
            rows: 1,
        };

        // Single-page case.
        if !has_frame || multi_count == 0 {
            let mut components = Vec::new();
            for v in &variants {
                components.extend(v.iter().map(|r| r.component.clone()));
            }
            let total = static_cost;
            self.validate(total, false)?;
            self.persist(&app.db, &rctx, &data_json).await?;
            return Ok(vec![RenderedMessage::new(components)]);
        }

        // Paginate: reserve static content and navigation, then pack the
        // Frame variants greedily.
        let multi_index = variants
            .iter()
            .position(|v| v.len() > 1)
            .expect("multi exists");
        let reserve_nav = self.paginate;
        let nav_reserve_cost = if reserve_nav {
            nav_cost
        } else {
            Cost::default()
        };
        let Some(budget_components) = limits::MAX_TOTAL_COMPONENTS
            .checked_sub(static_cost.components + nav_reserve_cost.components)
        else {
            return Err(self.static_overflow(static_cost + nav_reserve_cost));
        };
        let Some(budget_rows) =
            limits::MAX_ACTION_ROWS.checked_sub(static_cost.rows + nav_reserve_cost.rows)
        else {
            return Err(self.static_overflow(static_cost + nav_reserve_cost));
        };
        let budget = Cost {
            components: budget_components,
            chars: limits::MAX_TEXT_CHARS.saturating_sub(static_cost.chars),
            rows: budget_rows,
        };
        self.validate(static_cost + nav_reserve_cost, true)?;

        let mut pages: Vec<Vec<Resolved>> = Vec::new();
        let mut current: Vec<Resolved> = Vec::new();
        let mut current_cost = Cost::default();
        for v in &variants[multi_index] {
            let fits = current_cost.components + v.cost.components <= budget.components
                && current_cost.chars + v.cost.chars <= budget.chars;
            if !fits && !current.is_empty() {
                pages.push(std::mem::take(&mut current));
                current_cost = Cost::default();
            }
            if v.cost.components > budget.components || v.cost.chars > budget.chars {
                return Err(Error::ComponentTooLarge {
                    kind: "frame element",
                    detail: format!(
                        "element costs {} components / {} chars, page budget is {} components / {} chars",
                        v.cost.components, v.cost.chars, budget.components, budget.chars
                    ),
                });
            }
            current_cost = current_cost + v.cost;
            current.push(v.clone());
        }
        if !current.is_empty() {
            pages.push(current);
        }
        if pages.is_empty() {
            pages.push(Vec::new());
        }
        if pages.len() > 1 && !self.paginate {
            return Err(Error::FrameNeedsPagination {
                page_id: self.id.clone(),
            });
        }

        // Assemble each page: static structure stays on every page.
        let mut out = Vec::new();
        for (i, page_slice) in pages.iter().enumerate() {
            let mut components = Vec::new();
            let mut cost = static_cost;
            for (idx, v) in variants.iter().enumerate() {
                if idx == multi_index {
                    components.extend(page_slice.iter().map(|r| r.component.clone()));
                    for r in page_slice {
                        cost = cost + r.cost;
                    }
                } else {
                    components.extend(v.iter().map(|r| r.component.clone()));
                }
            }
            // Framework navigation controls; the current position is part
            // of the rendered Page state and is persisted per page.
            if pages.len() > 1 {
                let nav_json = serde_json::to_vec(&PageData {
                    data: pd.data.clone(),
                    frame_page: i,
                })?;
                let prev_id = custom_id_hash(&self.id, NAV_PREV, &nav_json);
                let next_id = custom_id_hash(&self.id, NAV_NEXT, &nav_json);
                app.db
                    .store
                    .insert(
                        &prev_id,
                        StoreRecord {
                            def_id: self.id.clone(),
                            exec_id: NAV_PREV.to_string(),
                            data_json: nav_json.clone(),
                        },
                    )
                    .await?;
                app.db
                    .store
                    .insert(
                        &next_id,
                        StoreRecord {
                            def_id: self.id.clone(),
                            exec_id: NAV_NEXT.to_string(),
                            data_json: nav_json,
                        },
                    )
                    .await?;
                let row = CreateActionRow::buttons(vec![
                    CreateButton::new(prev_id)
                        .label("Previous")
                        .style(ButtonStyle::Secondary)
                        .disabled(i == 0)
                        .into_owned(),
                    CreateButton::new(next_id)
                        .label("Next")
                        .style(ButtonStyle::Secondary)
                        .disabled(i + 1 == pages.len())
                        .into_owned(),
                ])
                .into_owned();
                components.push(CreateComponent::ActionRow(row));
                cost = cost + nav_cost;
            }
            self.validate(cost, false)?;
            out.push(RenderedMessage::new(components));
        }

        self.persist(&app.db, &rctx, &data_json).await?;
        Ok(out)
    }

    fn static_overflow(&self, static_cost: Cost) -> Error {
        Error::DiscordLimit {
            limit: "MAX_TOTAL_COMPONENTS",
            limit_value: limits::MAX_TOTAL_COMPONENTS,
            actual: static_cost.components,
            context: format!("static components of page '{}'", self.id),
        }
    }

    fn validate(&self, cost: Cost, _pre: bool) -> Result<()> {
        if cost.components > limits::MAX_TOTAL_COMPONENTS {
            return Err(self.static_overflow(cost));
        }
        if cost.chars > limits::MAX_TEXT_CHARS {
            return Err(Error::limit(
                "MAX_TEXT_CHARS",
                limits::MAX_TEXT_CHARS,
                cost.chars,
                format!("page '{}' text", self.id),
            ));
        }
        if cost.rows > limits::MAX_ACTION_ROWS {
            return Err(Error::limit(
                "MAX_ACTION_ROWS",
                limits::MAX_ACTION_ROWS,
                cost.rows,
                format!("page '{}' action rows", self.id),
            ));
        }
        Ok(())
    }

    async fn persist<'a>(
        &'a self,
        db: &'a crate::db::Database<E>,
        rctx: &'a RenderCtx<'a, T, E>,
        data_json: &'a [u8],
    ) -> Result<()> {
        for (custom_id, exec_id) in rctx.take_used() {
            db.store
                .insert(
                    &custom_id,
                    StoreRecord {
                        def_id: self.id.clone(),
                        exec_id,
                        data_json: data_json.to_vec(),
                    },
                )
                .await?;
        }
        Ok(())
    }
}

impl<T, E> PageErased<E> for PageDef<T, E>
where
    T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn page_id(&self) -> &str {
        &self.id
    }

    fn invoke<'a>(
        &'a self,
        app: &'a App<E>,
        interaction: &'a Interaction,
        exec_id: &'a str,
        data_json: &'a [u8],
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if exec_id == NAV_PREV || exec_id == NAV_NEXT {
                let pd: PageData<T> =
                    serde_json::from_slice(data_json).map_err(|source| Error::Deserialize {
                        def_id: self.id.clone(),
                        source,
                    })?;
                let delta: isize = if exec_id == NAV_PREV { -1 } else { 1 };
                let target = pd.frame_page as isize + delta;
                if target < 0 {
                    return Err(Error::msg("navigation before the first page"));
                }
                let target = target as usize;
                let pages = self.render_all(app, pd.data.clone(), pd.frame_page).await?;
                let page = pages
                    .get(target)
                    .ok_or_else(|| Error::msg("navigation past the last page"))?;
                let ctx = Ctx::new(app, interaction);
                ctx.edit_message(page).await?;
                return Ok(());
            }
            let exec = {
                let execs = self
                    .execs
                    .lock()
                    .map_err(|_| Error::msg("exec lock poisoned"))?;
                execs
                    .get(exec_id)
                    .cloned()
                    .ok_or_else(|| Error::UnknownExec {
                        def_id: self.id.clone(),
                        exec_id: exec_id.to_string(),
                    })?
            };
            // The persisted record holds PageData; executions receive the
            // original data payload.
            let pd: PageData<T> =
                serde_json::from_slice(data_json).map_err(|source| Error::Deserialize {
                    def_id: self.id.clone(),
                    source,
                })?;
            let exec_data_json = serde_json::to_vec(&pd.data)?;
            let mut ctx = Ctx::new(app, interaction);
            ctx.selection_json = extract_selection_json(interaction);
            exec.invoke_erased(&ctx, &exec_data_json).await
        })
    }

    fn render_json<'a>(
        &'a self,
        app: &'a App<E>,
        data_json: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<RenderedMessage>>> {
        Box::pin(async move {
            let pd: PageData<T> =
                serde_json::from_slice(data_json).map_err(|source| Error::Deserialize {
                    def_id: self.id.clone(),
                    source,
                })?;
            self.render_all(app, pd.data, pd.frame_page).await
        })
    }
}

/// Build the serialized `Selection<S>` payload for a component interaction,
/// pairing selected IDs with Discord's resolved entities.
pub(crate) fn extract_selection_json(interaction: &Interaction) -> Option<serde_json::Value> {
    let Interaction::Component(i) = interaction else {
        return None;
    };
    let resolved = &i.data.resolved;
    let values = match &i.data.kind {
        ComponentInteractionDataKind::Button => return None,
        ComponentInteractionDataKind::StringSelect { values } => {
            serde_json::json!(values.iter().collect::<Vec<_>>())
        }
        ComponentInteractionDataKind::UserSelect { values } => serde_json::json!(
            values
                .iter()
                .map(|id| UserChoice {
                    id: *id,
                    user: resolved.users.get(id).cloned()
                })
                .collect::<Vec<_>>()
        ),
        ComponentInteractionDataKind::RoleSelect { values } => serde_json::json!(
            values
                .iter()
                .map(|id| RoleChoice {
                    id: *id,
                    role: resolved.roles.get(id).cloned()
                })
                .collect::<Vec<_>>()
        ),
        ComponentInteractionDataKind::ChannelSelect { values } => serde_json::json!(
            values
                .iter()
                .map(|id| ChannelChoice {
                    id: GenericChannelId::new(id.get()),
                    channel: resolved
                        .channels
                        .get(&GenericChannelId::new(id.get()))
                        .cloned(),
                })
                .collect::<Vec<_>>()
        ),
        ComponentInteractionDataKind::MentionableSelect { values } => serde_json::json!(
            values
                .iter()
                .map(|id: &GenericId| {
                    let uid = UserId::new(id.get());
                    let rid = RoleId::new(id.get());
                    MentionableChoice {
                        id: *id,
                        user: resolved.users.get(&uid).cloned(),
                        role: resolved.roles.get(&rid).cloned(),
                    }
                })
                .collect::<Vec<_>>()
        ),
        ComponentInteractionDataKind::Unknown(_) => return None,
    };
    Some(serde_json::json!({ "values": values, "resolved": resolved }))
}

/// A registered Page handle. Rendering is only available on registered
/// Pages; the definition itself is immutable.
#[derive(Clone)]
pub struct RegisteredPage<T, E = ()> {
    pub(crate) def: Arc<PageDef<T, E>>,
}

impl<T, E> RegisteredPage<T, E>
where
    T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    /// The globally unique Page ID.
    pub fn id(&self) -> &str {
        &self.def.id
    }

    /// Render the Page. Returns the first generated message; a Page with a
    /// paginated Frame produces more via [`render_all`](Self::render_all).
    pub async fn render<'a>(&'a self, app: &'a App<E>, data: T) -> Result<RenderedMessage> {
        Ok(self.def.render_all(app, data, 0).await?.remove(0))
    }

    /// Render every generated page.
    pub fn render_all<'a>(
        &'a self,
        app: &'a App<E>,
        data: T,
    ) -> impl std::future::Future<Output = Result<Vec<RenderedMessage>>> + Send + 'a {
        self.def.render_all(app, data, 0)
    }

    /// Render starting at a specific Frame page.
    pub async fn render_page<'a>(
        &'a self,
        app: &'a App<E>,
        data: T,
        frame_page: usize,
    ) -> Result<RenderedMessage> {
        let pages = self.def.render_all(app, data, frame_page).await?;
        pages
            .into_iter()
            .nth(frame_page)
            .ok_or_else(|| Error::msg("frame page out of range"))
    }
}

/// The registry of registered Pages. It holds Pages of different generic
/// data types; the public API preserves concrete types and type erasure
/// stays an internal detail.
pub struct PageRegistry<E> {
    pub(crate) entries: Mutex<HashMap<String, PageEntry<E>>>,
}

pub(crate) struct PageEntry<E> {
    pub erased: Arc<dyn PageErased<E>>,
    pub any: Arc<dyn Any + Send + Sync>,
}

impl<E: Send + Sync + 'static> PageRegistry<E> {
    pub(crate) fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn insert<T>(&self, id: &str, def: Arc<PageDef<T, E>>) -> Result<()>
    where
        T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
        PageDef<T, E>: PageErased<E>,
    {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| Error::msg("registry poisoned"))?;
        if entries.contains_key(id) {
            return Err(Error::DuplicatePageId {
                page_id: id.to_string(),
            });
        }
        entries.insert(
            id.to_string(),
            PageEntry {
                erased: def.clone(),
                any: def,
            },
        );
        Ok(())
    }

    /// Look up a registered Page by ID, preserving its concrete data type.
    pub fn query<T>(&self, id: &str) -> Result<RegisteredPage<T, E>>
    where
        T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    {
        let entries = self
            .entries
            .lock()
            .map_err(|_| Error::msg("registry poisoned"))?;
        let entry = entries.get(id).ok_or_else(|| Error::UnknownPage {
            page_id: id.to_string(),
        })?;
        let def =
            entry
                .any
                .clone()
                .downcast::<PageDef<T, E>>()
                .map_err(|_| Error::DataMismatch {
                    def_id: id.to_string(),
                })?;
        Ok(RegisteredPage { def })
    }

    pub(crate) fn erased(&self, id: &str) -> Result<Arc<dyn PageErased<E>>> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| Error::msg("registry poisoned"))?;
        entries
            .get(id)
            .map(|e| e.erased.clone())
            .ok_or_else(|| Error::UnknownPage {
                page_id: id.to_string(),
            })
    }
}

// Silence unused-import warnings for items used only in some configurations.
