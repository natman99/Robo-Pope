//! Discord Components V2 building blocks.
//!
//! Every property that can reasonably be resolved at render time accepts
//! either its ordinary value or a [`dynamic`](crate::dynamic) callback.
//! Fixed child counts (Section text slots, button rows) are part of the
//! API: each slot is a static value, a callback, or `None`.

use serenity::builder::{
    CreateActionRow, CreateButton, CreateComponent, CreateContainer, CreateContainerComponent,
    CreateFile, CreateMediaGallery, CreateMediaGalleryItem, CreateSection, CreateSectionAccessory,
    CreateSectionComponent, CreateSeparator, CreateTextDisplay, CreateThumbnail,
    CreateUnfurledMediaItem,
};
use serenity::model::Colour;
use serenity::model::application::{ButtonStyle, SeparatorSpacingSize};

use crate::callback::{Exec, FrameSlot};
use crate::context::RenderCtx;
use crate::db::custom_id_hash;
use crate::error::{Error, Result};
use crate::select::SelectMenu;
use crate::value::{Slot, SlotValue, Value};

/// Discord's Components V2 limits.
pub mod limits {
    /// Total components in a Components V2 message, including nested.
    pub const MAX_TOTAL_COMPONENTS: usize = 40;
    /// Combined text character budget across Text Displays.
    pub const MAX_TEXT_CHARS: usize = 4000;
    /// Top-level Action Rows.
    pub const MAX_ACTION_ROWS: usize = 5;
    /// Text Displays inside one Section.
    pub const MAX_SECTION_TEXTS: usize = 3;
    /// Buttons inside one Action Row.
    pub const MAX_ROW_BUTTONS: usize = 5;
    /// Options in a String Select.
    pub const MAX_SELECT_OPTIONS: usize = 25;
    /// Items in a Media Gallery.
    pub const MAX_GALLERY_ITEMS: usize = 10;
    /// Direct children of a Container.
    pub const MAX_CONTAINER_CHILDREN: usize = 10;
    /// A Text Display's content.
    pub const MAX_TEXT_DISPLAY: usize = 4000;
    /// A button label.
    pub const MAX_BUTTON_LABEL: usize = 80;
    /// A select placeholder.
    pub const MAX_PLACEHOLDER: usize = 150;
    /// A select option label.
    pub const MAX_OPTION_LABEL: usize = 100;
    /// A select option description.
    pub const MAX_OPTION_DESCRIPTION: usize = 100;
    /// A thumbnail description.
    pub const MAX_THUMBNAIL_DESCRIPTION: usize = 100;
}

pub(crate) fn check_len(
    limit: usize,
    limit_name: &'static str,
    actual: usize,
    context: impl Into<String>,
) -> Result<()> {
    if actual > limit {
        return Err(Error::limit(limit_name, limit, actual, context));
    }
    Ok(())
}

/// A resolved component plus its resource cost, used by pagination.
#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    pub component: CreateComponent<'static>,
    pub cost: Cost,
}

/// Component/character/row cost of a resolved subtree.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Cost {
    pub components: usize,
    pub chars: usize,
    pub rows: usize,
}

impl std::ops::Add for Cost {
    type Output = Cost;
    fn add(self, o: Cost) -> Cost {
        Cost {
            components: self.components + o.components,
            chars: self.chars + o.chars,
            rows: self.rows + o.rows,
        }
    }
}

/// Compute the cost of a resolved component from its serialized form.
pub(crate) fn cost_of(component: &CreateComponent<'static>) -> (Cost, serde_json::Value) {
    let json = serde_json::to_value(component).expect("component serialization is infallible");
    let mut cost = Cost::default();
    walk(&json, true, &mut cost);
    (cost, json)
}

fn walk(json: &serde_json::Value, top_level: bool, cost: &mut Cost) {
    let serde_json::Value::Object(map) = json else {
        return;
    };
    if map.contains_key("type") {
        cost.components += 1;
        if top_level && map.get("type").and_then(|t| t.as_u64()) == Some(1) {
            cost.rows += 1;
        }
    }
    if let Some(content) = map.get("content").and_then(|c| c.as_str()) {
        cost.chars += content.chars().count();
    }
    if let Some(desc) = map.get("description").and_then(|c| c.as_str()) {
        cost.chars += desc.chars().count();
    }
    for key in ["components", "items", "accessory"] {
        match map.get(key) {
            Some(serde_json::Value::Array(items)) => {
                for item in items {
                    walk(item, false, cost);
                }
            }
            Some(serde_json::Value::Object(_)) => walk(&json[key], false, cost),
            _ => {}
        }
    }
}

/// A Markdown text block. Each `add_text` call adds a separate Text Display;
/// text is never silently concatenated.
#[derive(Debug)]
pub struct Text<T, E> {
    pub(crate) content: Value<String, T, E>,
}

impl<T: Clone + Send + Sync + 'static, E: Sync> Text<T, E> {
    pub(crate) async fn resolve(
        &self,
        rctx: &RenderCtx<'_, T, E>,
        data: &T,
    ) -> Result<CreateTextDisplay<'static>> {
        let content = self.content.resolve(rctx, data).await?;
        check_len(
            limits::MAX_TEXT_DISPLAY,
            "MAX_TEXT_DISPLAY",
            content.chars().count(),
            "text display",
        )?;
        Ok(CreateTextDisplay::new(content).into_owned())
    }
}

/// A small image, usually a Section accessory.
#[derive(Debug)]
pub struct Thumbnail<T, E> {
    pub(crate) url: Value<String, T, E>,
    pub(crate) description: Option<Value<String, T, E>>,
    pub(crate) spoiler: bool,
}

impl<T: Clone + Send + Sync + 'static, E: Sync> Thumbnail<T, E> {
    /// A thumbnail displaying the media at `url`.
    pub fn new(url: impl Into<Value<String, T, E>>) -> Self {
        Self {
            url: url.into(),
            description: None,
            spoiler: false,
        }
    }

    /// Set the description (static or dynamic).
    #[must_use]
    pub fn set_description(mut self, description: impl Into<Value<String, T, E>>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Mark the thumbnail as spoiler-tagged.
    #[must_use]
    pub fn set_spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = spoiler;
        self
    }

    pub(crate) async fn resolve(
        &self,
        rctx: &RenderCtx<'_, T, E>,
        data: &T,
    ) -> Result<CreateThumbnail<'static>> {
        let url = self.url.resolve(rctx, data).await?;
        let mut b = CreateThumbnail::new(CreateUnfurledMediaItem::new(url).into_owned());
        if let Some(d) = &self.description {
            let d = d.resolve(rctx, data).await?;
            check_len(
                limits::MAX_THUMBNAIL_DESCRIPTION,
                "MAX_THUMBNAIL_DESCRIPTION",
                d.chars().count(),
                "thumbnail description",
            )?;
            b = b.description(d);
        }
        if self.spoiler {
            b = b.spoiler(true);
        }
        Ok(b.into_owned())
    }
}

/// A visual separator.
#[derive(Debug, Clone, Copy)]
pub struct Separator {
    pub(crate) spacing: SeparatorSpacingSize,
    pub(crate) divider: bool,
}

impl Separator {
    /// A separator with default spacing and no divider.
    pub fn new() -> Self {
        Self {
            spacing: SeparatorSpacingSize::Small,
            divider: false,
        }
    }

    /// Set the spacing size.
    #[must_use]
    pub fn set_spacing(mut self, spacing: SeparatorSpacingSize) -> Self {
        self.spacing = spacing;
        self
    }

    /// Show a divider line.
    #[must_use]
    pub fn set_divider(mut self, divider: bool) -> Self {
        self.divider = divider;
        self
    }

    pub(crate) fn resolve(&self) -> CreateSeparator {
        let mut b = CreateSeparator::new();
        if self.divider {
            b = b.divider(true);
        }
        b.spacing(self.spacing)
    }
}

impl Default for Separator {
    fn default() -> Self {
        Self::new()
    }
}

/// A media gallery of images or videos.
#[derive(Debug)]
pub struct MediaGallery<T, E> {
    pub(crate) items: Vec<MediaItem<T, E>>,
}

/// One item in a [`MediaGallery`].
#[derive(Debug)]
pub struct MediaItem<T, E> {
    pub(crate) url: Value<String, T, E>,
    pub(crate) description: Option<Value<String, T, E>>,
    pub(crate) spoiler: bool,
}

impl<T: Clone + Send + Sync + 'static, E: Sync> MediaItem<T, E> {
    /// A media item at `url`.
    pub fn new(url: impl Into<Value<String, T, E>>) -> Self {
        Self {
            url: url.into(),
            description: None,
            spoiler: false,
        }
    }

    /// Set the item description (static or dynamic).
    #[must_use]
    pub fn set_description(mut self, description: impl Into<Value<String, T, E>>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Mark the item as spoiler-tagged.
    #[must_use]
    pub fn set_spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = spoiler;
        self
    }
}

impl<T: Clone + Send + Sync + 'static, E: Sync> Default for MediaGallery<T, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Send + Sync + 'static, E: Sync> MediaGallery<T, E> {
    /// An empty media gallery.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add a media item by URL (static or dynamic).
    #[must_use]
    pub fn add_media(mut self, url: impl Into<Value<String, T, E>>) -> Self {
        self.items.push(MediaItem::new(url));
        self
    }

    /// Add a fully configured media item.
    #[must_use]
    pub fn add_item(mut self, item: MediaItem<T, E>) -> Self {
        self.items.push(item);
        self
    }

    pub(crate) async fn resolve(
        &self,
        rctx: &RenderCtx<'_, T, E>,
        data: &T,
    ) -> Result<CreateMediaGallery<'static>> {
        if self.items.is_empty() {
            return Err(Error::InvalidComponent {
                kind: "media gallery",
                detail: "a media gallery needs at least one item".into(),
            });
        }
        check_len(
            limits::MAX_GALLERY_ITEMS,
            "MAX_GALLERY_ITEMS",
            self.items.len(),
            "media gallery",
        )?;
        let mut b = CreateMediaGallery::new(Vec::new());
        for item in &self.items {
            let url = item.url.resolve(rctx, data).await?;
            let mut mi =
                CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(url).into_owned());
            if let Some(d) = &item.description {
                mi = mi.description(d.resolve(rctx, data).await?);
            }
            if item.spoiler {
                mi = mi.spoiler(true);
            }
            b = b.add_item(mi.into_owned());
        }
        Ok(b.into_owned())
    }
}

/// A file component referencing an uploaded attachment.
///
/// Uploading a file and displaying it are related but distinct operations:
/// the attachment itself is uploaded with the message; this component
/// displays it.
#[derive(Debug)]
pub struct File {
    pub(crate) name: String,
    pub(crate) spoiler: bool,
}

impl File {
    /// Display the uploaded attachment with this filename.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            spoiler: false,
        }
    }

    /// Mark the file as spoiler-tagged.
    #[must_use]
    pub fn set_spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = spoiler;
        self
    }

    pub(crate) fn resolve(&self) -> CreateFile<'static> {
        let url = format!("attachment://{}", self.name);
        let mut b = CreateFile::new(CreateUnfurledMediaItem::new(url).into_owned());
        if self.spoiler {
            b = b.spoiler(true);
        }
        b.into_owned()
    }
}

/// An interactive button. Its label and style may be static or dynamic; the
/// execution is always an explicit callback.
#[derive(Debug)]
pub struct Button<T, E> {
    pub(crate) exec_id: String,
    pub(crate) label: Value<String, T, E>,
    pub(crate) style: Value<ButtonStyle, T, E>,
    pub(crate) disabled: Value<bool, T, E>,
    pub(crate) exec: Exec<T, E, ()>,
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> Button<T, E> {
    /// A button with a label, a page-unique execution ID, a style (static or
    /// dynamic) and an execution callback.
    pub fn new(
        label: impl Into<Value<String, T, E>>,
        exec_id: impl Into<String>,
        style: impl Into<Value<ButtonStyle, T, E>>,
        exec: impl Into<Exec<T, E, ()>>,
    ) -> Self {
        Self {
            exec_id: exec_id.into(),
            label: label.into(),
            style: style.into(),
            disabled: Value::Static(false),
            exec: exec.into(),
        }
    }

    /// Set the disabled state (static or dynamic).
    #[must_use]
    pub fn set_disabled(mut self, disabled: impl Into<Value<bool, T, E>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    pub(crate) async fn resolve(
        &self,
        rctx: &RenderCtx<'_, T, E>,
        data: &T,
    ) -> Result<CreateButton<'static>> {
        let label = self.label.resolve(rctx, data).await?;
        check_len(
            limits::MAX_BUTTON_LABEL,
            "MAX_BUTTON_LABEL",
            label.chars().count(),
            "button label",
        )?;
        let style = self.style.resolve(rctx, data).await?;
        let disabled = self.disabled.resolve(rctx, data).await?;
        let custom_id = custom_id_hash(rctx.def_id, &self.exec_id, rctx.data_json);
        rctx.record_exec(&custom_id, &self.exec_id);
        Ok(CreateButton::new(custom_id)
            .label(label)
            .style(style)
            .disabled(disabled)
            .into_owned())
    }
}

impl<T: Send + Sync + 'static, E: Send + Sync + 'static> SlotValue for Button<T, E> {}

impl<T: Send + Sync + 'static, E: Send + Sync + 'static> SlotValue for SelectMenu<T, E> {}

/// A link button: opens a URL, produces no interaction, and therefore has no
/// execution.
#[derive(Debug)]
pub struct LinkButton<T, E> {
    pub(crate) label: Value<String, T, E>,
    pub(crate) url: Value<String, T, E>,
    pub(crate) disabled: Value<bool, T, E>,
}

impl<T: Clone + Send + Sync + 'static, E: Sync> LinkButton<T, E> {
    /// A link button with a label and URL, both static or dynamic.
    pub fn new(label: impl Into<Value<String, T, E>>, url: impl Into<Value<String, T, E>>) -> Self {
        Self {
            label: label.into(),
            url: url.into(),
            disabled: Value::Static(false),
        }
    }

    /// Set the disabled state (static or dynamic).
    #[must_use]
    pub fn set_disabled(mut self, disabled: impl Into<Value<bool, T, E>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    pub(crate) async fn resolve(
        &self,
        rctx: &RenderCtx<'_, T, E>,
        data: &T,
    ) -> Result<CreateButton<'static>> {
        let label = self.label.resolve(rctx, data).await?;
        check_len(
            limits::MAX_BUTTON_LABEL,
            "MAX_BUTTON_LABEL",
            label.chars().count(),
            "button label",
        )?;
        let url = self.url.resolve(rctx, data).await?;
        let disabled = self.disabled.resolve(rctx, data).await?;
        Ok(CreateButton::new_link(url)
            .label(label)
            .disabled(disabled)
            .into_owned())
    }
}

/// A Section: one to three Text Displays alongside an accessory.
#[derive(Debug)]
pub struct Section<T, E> {
    pub(crate) texts: [Slot<String, T, E>; 3],
    pub(crate) accessory: Option<Accessory<T, E>>,
}

/// A Section's accessory: a Thumbnail or a Button.
#[derive(Debug)]
pub enum Accessory<T, E> {
    Thumbnail(Thumbnail<T, E>),
    Button(Button<T, E>),
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> From<Thumbnail<T, E>>
    for Accessory<T, E>
{
    fn from(v: Thumbnail<T, E>) -> Self {
        Self::Thumbnail(v)
    }
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> From<Button<T, E>>
    for Accessory<T, E>
{
    fn from(v: Button<T, E>) -> Self {
        Self::Button(v)
    }
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> Default for Section<T, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> Section<T, E> {
    /// A Section with all three text slots empty.
    pub fn new() -> Self {
        Self {
            texts: [Slot::Empty, Slot::Empty, Slot::Empty],
            accessory: None,
        }
    }

    /// Set the three fixed text slots. Each is static, dynamic, or `None`.
    #[must_use]
    pub fn set_text(
        mut self,
        first: impl Into<Slot<String, T, E>>,
        second: impl Into<Slot<String, T, E>>,
        third: impl Into<Slot<String, T, E>>,
    ) -> Self {
        self.texts = [first.into(), second.into(), third.into()];
        self
    }

    /// Set the Section's accessory.
    #[must_use]
    pub fn set_accessory(mut self, accessory: impl Into<Accessory<T, E>>) -> Self {
        self.accessory = Some(accessory.into());
        self
    }

    pub(crate) async fn resolve(
        &self,
        rctx: &RenderCtx<'_, T, E>,
        data: &T,
    ) -> Result<CreateSection<'static>> {
        let mut texts = Vec::new();
        for slot in &self.texts {
            if let Some(text) = slot.resolve(rctx, data).await? {
                check_len(
                    limits::MAX_TEXT_DISPLAY,
                    "MAX_TEXT_DISPLAY",
                    text.chars().count(),
                    "section text",
                )?;
                texts.push(CreateSectionComponent::TextDisplay(
                    CreateTextDisplay::new(text).into_owned(),
                ));
            }
        }
        if texts.is_empty() {
            return Err(Error::InvalidComponent {
                kind: "section",
                detail: "a section needs at least one text slot".into(),
            });
        }
        let accessory = match &self.accessory {
            Some(Accessory::Thumbnail(t)) => {
                CreateSectionAccessory::Thumbnail(t.resolve(rctx, data).await?)
            }
            Some(Accessory::Button(b)) => {
                CreateSectionAccessory::Button(b.resolve(rctx, data).await?)
            }
            None => {
                return Err(Error::InvalidComponent {
                    kind: "section",
                    detail: "a section needs an accessory".into(),
                });
            }
        };
        Ok(CreateSection::new(texts, accessory).into_owned())
    }
}

/// A child of a Container: a component or a Frame.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ContainerChild<T, E> {
    Component(Component<T, E>),
    Frame(FrameSlot<T, E>),
}

/// A Container groups components into a visually distinct block.
#[derive(Debug)]
pub struct Container<T, E> {
    pub(crate) accent: Option<u32>,
    pub(crate) spoiler: bool,
    pub(crate) children: Vec<ContainerChild<T, E>>,
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> Default for Container<T, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> Container<T, E> {
    /// An empty container.
    pub fn new() -> Self {
        Self {
            accent: None,
            spoiler: false,
            children: Vec::new(),
        }
    }

    /// Set the accent color.
    #[must_use]
    pub fn set_accent_color(mut self, color: u32) -> Self {
        self.accent = Some(color);
        self
    }

    /// Mark the container as spoiler-tagged.
    #[must_use]
    pub fn set_spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = spoiler;
        self
    }

    /// Add a Text Display.
    #[must_use]
    pub fn add_text(self, content: impl Into<Value<String, T, E>>) -> Self {
        self.add_component(Component::Text(Text {
            content: content.into(),
        }))
    }

    /// Add a separator.
    #[must_use]
    pub fn add_separator(self) -> Self {
        self.add_component(Component::Separator(Separator::new()))
    }

    /// Add a section.
    #[must_use]
    pub fn add_section(self, section: Section<T, E>) -> Self {
        self.add_component(Component::Section(section))
    }

    /// Add a media gallery.
    #[must_use]
    pub fn add_media_gallery(self, gallery: MediaGallery<T, E>) -> Self {
        self.add_component(Component::MediaGallery(gallery))
    }

    /// Add a file.
    #[must_use]
    pub fn add_file(self, file: File) -> Self {
        self.add_component(Component::File(file))
    }

    /// Add a fixed five-slot button row.
    #[must_use]
    pub fn add_button_row(
        self,
        row: (
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
        ),
    ) -> Self {
        self.add_component(Component::ButtonRow(ButtonRow([
            row.0.into(),
            row.1.into(),
            row.2.into(),
            row.3.into(),
            row.4.into(),
        ])))
    }

    /// Add a selection menu (occupies its own Action Row).
    #[must_use]
    pub fn add_selection_menu(self, menu: impl Into<SelectMenu<T, E>>) -> Self {
        self.add_component(Component::Select(menu.into()))
    }

    /// Add a component.
    #[must_use]
    pub fn add_component(mut self, component: Component<T, E>) -> Self {
        self.children.push(ContainerChild::Component(component));
        self
    }

    /// Add a Frame: a dynamic region resolved at render time.
    #[must_use]
    pub fn add_frame(mut self, frame: impl Into<FrameSlot<T, E>>) -> Self {
        self.children.push(ContainerChild::Frame(frame.into()));
        self
    }
}

/// A fixed five-slot button row; the framework creates the Action Row.
#[derive(Debug)]
pub struct ButtonRow<T, E>(pub(crate) [Slot<Button<T, E>, T, E>; 5]);

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> ButtonRow<T, E> {
    pub(crate) async fn resolve(
        &self,
        rctx: &RenderCtx<'_, T, E>,
        data: &T,
    ) -> Result<CreateActionRow<'static>> {
        let mut buttons = Vec::new();
        for slot in &self.0 {
            match slot {
                Slot::Empty => {}
                Slot::Val(Value::Static(b)) => buttons.push(b.resolve(rctx, data).await?),
                Slot::Val(Value::Dynamic(_)) => {
                    return Err(Error::InvalidComponent {
                        kind: "button row",
                        detail: "dynamic button values are not supported; a button's execution \
                                must be defined with the button"
                            .into(),
                    });
                }
            }
        }
        if buttons.is_empty() {
            return Err(Error::InvalidComponent {
                kind: "button row",
                detail: "a button row needs at least one button".into(),
            });
        }
        Ok(CreateActionRow::buttons(buttons).into_owned())
    }
}

/// Any component usable in a Page or Container.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Component<T, E> {
    Text(Text<T, E>),
    Section(Section<T, E>),
    Container(Container<T, E>),
    Separator(Separator),
    Thumbnail(Thumbnail<T, E>),
    MediaGallery(MediaGallery<T, E>),
    File(File),
    Button(Button<T, E>),
    LinkButton(LinkButton<T, E>),
    Select(SelectMenu<T, E>),
    ButtonRow(ButtonRow<T, E>),
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> Component<T, E> {
    /// Resolve this component into Serenity builders.
    ///
    /// Returns one component, except for a Container containing a Frame,
    /// which expands into one container per pagination slice.
    pub(crate) fn resolve<'a>(
        &'a self,
        rctx: &'a RenderCtx<'a, T, E>,
        data: &'a T,
    ) -> crate::value::BoxFuture<'a, Result<Vec<Resolved>>> {
        Box::pin(async move {
            match self {
                Self::Text(t) => {
                    let c = CreateComponent::TextDisplay(t.resolve(rctx, data).await?);
                    one(c)
                }
                Self::Section(s) => {
                    let c = CreateComponent::Section(s.resolve(rctx, data).await?);
                    one(c)
                }
                Self::Container(c) => c.resolve(rctx, data).await,
                Self::Separator(s) => one(CreateComponent::Separator(s.resolve())),
                Self::Thumbnail(_) => Err(Error::InvalidComponent {
                    kind: "thumbnail",
                    detail: "a thumbnail is only valid as a Section accessory".into(),
                }),
                Self::MediaGallery(g) => {
                    one(CreateComponent::MediaGallery(g.resolve(rctx, data).await?))
                }
                Self::File(f) => one(CreateComponent::File(f.resolve())),
                Self::Button(b) => one(CreateComponent::ActionRow(CreateActionRow::buttons(vec![
                    b.resolve(rctx, data).await?,
                ]))),
                Self::LinkButton(b) => {
                    one(CreateComponent::ActionRow(CreateActionRow::buttons(vec![
                        b.resolve(rctx, data).await?,
                    ])))
                }
                Self::Select(s) => one(CreateComponent::ActionRow(CreateActionRow::select_menu(
                    s.resolve(rctx, data).await?,
                ))),
                Self::ButtonRow(r) => one(CreateComponent::ActionRow(r.resolve(rctx, data).await?)),
            }
        })
    }

    /// Execution IDs declared by this component subtree (for registration
    /// validation).
    pub(crate) fn exec_ids(&self, out: &mut Vec<String>) {
        match self {
            Self::Button(b) => out.push(b.exec_id.clone()),
            Self::Section(s) => {
                if let Some(Accessory::Button(b)) = &s.accessory {
                    out.push(b.exec_id.clone());
                }
            }
            Self::Container(c) => {
                for child in &c.children {
                    if let ContainerChild::Component(comp) = child {
                        comp.exec_ids(out);
                    }
                }
            }
            Self::Select(s) => out.push(s.exec_id().to_string()),
            Self::ButtonRow(row) => {
                for slot in &row.0 {
                    if let Slot::Val(Value::Static(b)) = slot {
                        out.push(b.exec_id.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

impl<T: Clone + serde::de::DeserializeOwned + Send + Sync + 'static, E: Send + Sync + 'static>
    Component<T, E>
{
    /// Execution identities declared by this subtree, as erased callbacks
    /// (for registration and Frame runtime registration).
    pub(crate) fn collect_execs(
        &self,
        out: &mut Vec<(String, std::sync::Arc<dyn crate::callback::ErasedExec<E>>)>,
    ) {
        use std::sync::Arc;
        match self {
            Self::Button(b) => out.push((b.exec_id.clone(), Arc::new(b.exec.clone()))),
            Self::Section(s) => {
                if let Some(Accessory::Button(b)) = &s.accessory {
                    out.push((b.exec_id.clone(), Arc::new(b.exec.clone())));
                }
            }
            Self::Container(c) => {
                for child in &c.children {
                    if let ContainerChild::Component(comp) = child {
                        comp.collect_execs(out);
                    }
                }
            }
            Self::Select(s) => s.collect_execs(out),
            Self::ButtonRow(row) => {
                for slot in &row.0 {
                    if let Slot::Val(Value::Static(b)) = slot {
                        out.push((b.exec_id.clone(), Arc::new(b.exec.clone())));
                    }
                }
            }
            _ => {}
        }
    }
}

fn to_container_component(
    c: CreateComponent<'static>,
) -> Result<CreateContainerComponent<'static>> {
    Ok(match c {
        CreateComponent::ActionRow(a) => CreateContainerComponent::ActionRow(a),
        CreateComponent::Section(s) => CreateContainerComponent::Section(s),
        CreateComponent::TextDisplay(t) => CreateContainerComponent::TextDisplay(t),
        CreateComponent::MediaGallery(m) => CreateContainerComponent::MediaGallery(m),
        CreateComponent::File(f) => CreateContainerComponent::File(f),
        CreateComponent::Separator(s) => CreateContainerComponent::Separator(s),
        _ => {
            return Err(Error::InvalidComponent {
                kind: "container",
                detail: "this component type cannot be a container child".into(),
            });
        }
    })
}

fn one(c: CreateComponent<'static>) -> Result<Vec<Resolved>> {
    let (cost, _) = cost_of(&c);
    Ok(vec![Resolved { component: c, cost }])
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> Container<T, E> {
    pub(crate) fn resolve<'a>(
        &'a self,
        rctx: &'a RenderCtx<'a, T, E>,
        data: &'a T,
    ) -> crate::value::BoxFuture<'a, Result<Vec<Resolved>>> {
        Box::pin(async move {
            let frames: Vec<&FrameSlot<T, E>> = self
                .children
                .iter()
                .filter_map(|c| match c {
                    ContainerChild::Frame(f) => Some(f),
                    _ => None,
                })
                .collect();
            if frames.len() > 1 {
                return Err(Error::MultipleFrames {
                    page_id: rctx.def_id.to_string(),
                });
            }
            // Resolve static children once.
            let mut static_children: Vec<CreateContainerComponent<'static>> = Vec::new();
            let mut static_cost = Cost {
                components: 1,
                chars: 0,
                rows: 0,
            };
            for child in &self.children {
                if let ContainerChild::Component(comp) = child {
                    for r in comp.resolve(rctx, data).await? {
                        static_cost = static_cost
                            + Cost {
                                components: r.cost.components,
                                chars: r.cost.chars,
                                rows: 0,
                            };
                        static_children.push(to_container_component(r.component)?);
                    }
                }
            }
            let build = |extra: Vec<CreateContainerComponent<'static>>| {
                let mut all = static_children.clone();
                all.extend(extra);
                let mut b = CreateContainer::new(all);
                if let Some(accent) = self.accent {
                    b = b.accent_color(Colour::new(accent));
                }
                if self.spoiler {
                    b = b.spoiler(true);
                }
                b.into_owned()
            };
            let Some(frame) = frames.first() else {
                check_len(
                    limits::MAX_CONTAINER_CHILDREN,
                    "MAX_CONTAINER_CHILDREN",
                    static_children.len(),
                    "container",
                )?;
                let c = CreateComponent::Container(build(Vec::new()));
                let (cost, _) = cost_of(&c);
                return Ok(vec![Resolved { component: c, cost }]);
            };
            // Frame inside a container: resolve contents and slice them into
            // one container per slice, respecting the child limit.
            let elements =
                (frame.f)(rctx, data.clone())
                    .await
                    .map_err(|source| Error::FrameFailed {
                        page_id: rctx.def_id.to_string(),
                        source: Box::new(source),
                    })?;
            let mut resolved: Vec<Resolved> = Vec::new();
            for el in elements {
                for r in el.into_component().resolve(rctx, data).await? {
                    resolved.push(r);
                }
            }
            let per_page = limits::MAX_CONTAINER_CHILDREN.saturating_sub(static_children.len());
            if per_page == 0 || resolved.len() <= per_page {
                let extra: Vec<CreateContainerComponent<'static>> = resolved
                    .into_iter()
                    .map(|r| to_container_component(r.component))
                    .collect::<Result<_>>()?;
                let c = CreateComponent::Container(build(extra));
                let (cost, _) = cost_of(&c);
                return Ok(vec![Resolved { component: c, cost }]);
            }
            let mut out = Vec::new();
            for chunk in resolved.chunks(per_page) {
                let extra: Vec<CreateContainerComponent<'static>> = chunk
                    .iter()
                    .map(|r| to_container_component(r.component.clone()))
                    .collect::<Result<_>>()?;
                let c = CreateComponent::Container(build(extra));
                let (cost, _) = cost_of(&c);
                out.push(Resolved { component: c, cost });
            }
            Ok(out)
        })
    }
}

/// A heterogeneous element returned by a Frame.
///
/// Frames compose different component types without manual Serenity
/// conversion; interactive elements keep stable execution identities derived
/// from developer-defined IDs, not list position.
#[derive(Debug)]
pub enum Element<T, E> {
    Component(Component<T, E>),
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> Element<T, E> {
    /// A Text Display element.
    pub fn text(content: impl Into<Value<String, T, E>>) -> Self {
        Self::Component(Component::Text(Text {
            content: content.into(),
        }))
    }

    /// A Separator element.
    #[must_use]
    pub fn separator() -> Self {
        Self::Component(Component::Separator(Separator::new()))
    }

    /// A Separator element with settings.
    #[must_use]
    pub fn separator_with(separator: Separator) -> Self {
        Self::Component(Component::Separator(separator))
    }

    /// A Section element.
    #[must_use]
    pub fn section(section: Section<T, E>) -> Self {
        Self::Component(Component::Section(section))
    }

    /// A Container element.
    #[must_use]
    pub fn container(container: Container<T, E>) -> Self {
        Self::Component(Component::Container(container))
    }

    /// A Media Gallery element.
    #[must_use]
    pub fn media_gallery(gallery: MediaGallery<T, E>) -> Self {
        Self::Component(Component::MediaGallery(gallery))
    }

    /// A File element.
    #[must_use]
    pub fn file(file: File) -> Self {
        Self::Component(Component::File(file))
    }

    /// A standalone button (placed in its own Action Row).
    #[must_use]
    pub fn button(button: Button<T, E>) -> Self {
        Self::Component(Component::Button(button))
    }

    /// A link button.
    #[must_use]
    pub fn link_button(button: LinkButton<T, E>) -> Self {
        Self::Component(Component::LinkButton(button))
    }

    /// A selection menu (placed in its own Action Row).
    #[must_use]
    pub fn selection_menu(menu: impl Into<SelectMenu<T, E>>) -> Self {
        Self::Component(Component::Select(menu.into()))
    }

    /// A fixed five-slot button row.
    #[must_use]
    pub fn button_row(
        row: (
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
            impl Into<Slot<Button<T, E>, T, E>>,
        ),
    ) -> Self {
        Self::Component(Component::ButtonRow(ButtonRow([
            row.0.into(),
            row.1.into(),
            row.2.into(),
            row.3.into(),
            row.4.into(),
        ])))
    }

    pub(crate) fn as_component(&self) -> &Component<T, E> {
        match self {
            Self::Component(c) => c,
        }
    }

    pub(crate) fn into_component(self) -> Component<T, E> {
        match self {
            Self::Component(c) => c,
        }
    }
}
