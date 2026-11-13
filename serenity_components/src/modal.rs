//! Modals: forms opened from an interaction and submitted back as a separate
//! interaction.
//!
//! A Modal is its own blueprint type. Its generic data works like a Page's
//! data: serialized when the Modal is opened, reconstructed on submission.
//! Submitted form values are separate and reach the callback through
//! [`ModalSubmission`].

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serenity::builder::{
    CreateCheckbox, CreateCheckboxGroup, CreateCheckboxGroupOption, CreateFileUpload,
    CreateInputText, CreateLabel, CreateModalComponent, CreateRadioGroup, CreateRadioGroupOption,
    CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption, CreateTextDisplay,
};
use serenity::model::application::{
    InputTextStyle, Label, LabelComponent, ModalComponent, ModalInteraction, ModalInteractionData,
};
use serenity::model::id::{AttachmentId, GenericChannelId, GenericId, RoleId, UserId};

use crate::app::App;
use crate::callback::{ChannelChoice, MentionableChoice, RoleChoice, SubmitSlot, UserChoice};
use crate::component::check_len;
use crate::db::{StoreRecord, custom_id_hash};
use crate::error::{Error, Result};
use crate::render::RenderedModal;
use crate::value::BoxFuture;

/// Modal limits.
pub mod modal_limits {
    pub const MAX_TITLE: usize = 45;
    pub const MAX_COMPONENTS: usize = 5;
    pub const MAX_LABEL: usize = 45;
    pub const MAX_LABEL_DESCRIPTION: usize = 100;
    pub const MAX_TEXT_INPUT: usize = 4000;
    pub const MAX_SELECT_OPTIONS: usize = 25;
}

/// An option for a modal String Select, Radio Group or Checkbox Group.
#[derive(Debug, Clone)]
pub struct ModalOption {
    pub label: String,
    pub value: String,
    pub description: Option<String>,
    pub default: bool,
}

impl ModalOption {
    /// An option with a label and value.
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            description: None,
            default: false,
        }
    }

    /// Set the description.
    #[must_use]
    pub fn set_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Preselect this option.
    #[must_use]
    pub fn set_default(mut self, default: bool) -> Self {
        self.default = default;
        self
    }
}

/// A modal String Select input.
#[derive(Debug, Clone)]
pub struct ModalStringSelect {
    options: Vec<ModalOption>,
    min_values: Option<u8>,
    max_values: Option<u8>,
    required: Option<bool>,
}

impl ModalStringSelect {
    /// A string select with the given options.
    pub fn new(options: impl IntoIterator<Item = ModalOption>) -> Self {
        Self {
            options: options.into_iter().collect(),
            min_values: None,
            max_values: None,
            required: None,
        }
    }

    /// Minimum selections.
    #[must_use]
    pub fn set_min_values(mut self, min: u8) -> Self {
        self.min_values = Some(min);
        self
    }

    /// Maximum selections.
    #[must_use]
    pub fn set_max_values(mut self, max: u8) -> Self {
        self.max_values = Some(max);
        self
    }

    /// Whether a selection is required.
    #[must_use]
    pub fn set_required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }
}

/// A modal select input of any kind.
#[derive(Debug, Clone)]
pub enum ModalSelect {
    String(ModalStringSelect),
    User,
    Role,
    Mentionable,
    Channel,
}

impl From<ModalStringSelect> for ModalSelect {
    fn from(v: ModalStringSelect) -> Self {
        Self::String(v)
    }
}

/// A modal input component. Inputs use stable developer-defined IDs, which
/// retrieve their submitted values. Discord's required Label structures are
/// handled internally.
#[derive(Debug, Clone)]
pub enum ModalInput {
    TextInput {
        id: String,
        label: String,
        style: InputTextStyle,
        placeholder: Option<String>,
        required: bool,
        min_length: Option<u16>,
        max_length: Option<u16>,
    },
    Select {
        id: String,
        label: String,
        select: ModalSelect,
    },
    FileUpload {
        id: String,
        label: String,
        min_values: Option<u8>,
        max_values: Option<u8>,
        required: Option<bool>,
    },
    RadioGroup {
        id: String,
        label: String,
        options: Vec<ModalOption>,
        required: Option<bool>,
    },
    CheckboxGroup {
        id: String,
        label: String,
        options: Vec<ModalOption>,
        min_values: Option<u8>,
        max_values: Option<u8>,
        required: Option<bool>,
    },
    Checkbox {
        id: String,
        label: String,
        default_selected: bool,
    },
    TextDisplay {
        content: String,
    },
}

/// A Modal blueprint: a form Discord displays after an interaction.
#[derive(Debug)]
pub struct Modal<T, E = ()> {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) inputs: Vec<ModalInput>,
    pub(crate) submit: Option<SubmitSlot<T, E>>,
}

impl<T, E> Modal<T, E>
where
    T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    /// A Modal with a globally unique ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: String::new(),
            inputs: Vec::new(),
            submit: None,
        }
    }

    /// Set the modal title (max 45 characters, validated at render).
    #[must_use]
    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Add a Text Input with a stable component ID.
    #[must_use]
    pub fn add_text_input(
        mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        style: InputTextStyle,
    ) -> Self {
        self.inputs.push(ModalInput::TextInput {
            id: id.into(),
            label: label.into(),
            style,
            placeholder: None,
            required: true,
            min_length: None,
            max_length: None,
        });
        self
    }

    /// Add a select input of any kind.
    #[must_use]
    pub fn add_select(
        mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        select: impl Into<ModalSelect>,
    ) -> Self {
        self.inputs.push(ModalInput::Select {
            id: id.into(),
            label: label.into(),
            select: select.into(),
        });
        self
    }

    /// Add a File Upload input.
    #[must_use]
    pub fn add_file_upload(mut self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.inputs.push(ModalInput::FileUpload {
            id: id.into(),
            label: label.into(),
            min_values: None,
            max_values: None,
            required: None,
        });
        self
    }

    /// Add a Radio Group input.
    #[must_use]
    pub fn add_radio_group(
        mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        options: impl IntoIterator<Item = ModalOption>,
    ) -> Self {
        self.inputs.push(ModalInput::RadioGroup {
            id: id.into(),
            label: label.into(),
            options: options.into_iter().collect(),
            required: None,
        });
        self
    }

    /// Add a Checkbox Group input.
    #[must_use]
    pub fn add_checkbox_group(
        mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        options: impl IntoIterator<Item = ModalOption>,
    ) -> Self {
        self.inputs.push(ModalInput::CheckboxGroup {
            id: id.into(),
            label: label.into(),
            options: options.into_iter().collect(),
            min_values: None,
            max_values: None,
            required: None,
        });
        self
    }

    /// Add a Checkbox input.
    #[must_use]
    pub fn add_checkbox(mut self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.inputs.push(ModalInput::Checkbox {
            id: id.into(),
            label: label.into(),
            default_selected: false,
        });
        self
    }

    /// Add a read-only Text Display block.
    #[must_use]
    pub fn add_text_display(mut self, content: impl Into<String>) -> Self {
        self.inputs.push(ModalInput::TextDisplay {
            content: content.into(),
        });
        self
    }

    /// Set the submission execution:
    /// `|ctx, data, submission| Box::pin(async move { ... })`.
    #[must_use]
    pub fn on_submit(mut self, submit: impl Into<SubmitSlot<T, E>>) -> Self {
        self.submit = Some(submit.into());
        self
    }

    /// Register this Modal with the App.
    pub fn register(self, app: &App<E>) -> Result<RegisteredModal<T, E>> {
        let submit = self
            .submit
            .ok_or_else(|| Error::msg(format!("modal '{}' has no on_submit execution", self.id)))?;
        let def = Arc::new(ModalDef {
            id: self.id,
            title: self.title,
            inputs: self.inputs,
            submit,
        });
        app.modals.insert(&def.id, def.clone())?;
        Ok(RegisteredModal { def })
    }
}

/// A registered, immutable Modal definition.
pub struct ModalDef<T, E = ()> {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) inputs: Vec<ModalInput>,
    pub(crate) submit: SubmitSlot<T, E>,
}

/// Type-erased Modal submission routing.
pub trait ModalErased<E>: Send + Sync {
    /// The globally unique Modal ID.
    fn modal_id(&self) -> &str;

    /// Resolve and invoke the submission execution with persisted data.
    fn invoke_submit<'a>(
        &'a self,
        app: &'a App<E>,
        interaction: &'a ModalInteraction,
        data_json: &'a [u8],
    ) -> BoxFuture<'a, Result<()>>;
}

impl<T, E> ModalDef<T, E>
where
    T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    /// Render the Modal for the given data: validate limits, compute the
    /// deterministic custom ID, and persist the execution reference so the
    /// submission routes back to this callback.
    pub async fn render<'a>(&'a self, app: &'a App<E>, data: T) -> Result<RenderedModal> {
        check_len(
            modal_limits::MAX_TITLE,
            "MAX_MODAL_TITLE",
            self.title.chars().count(),
            format!("modal '{}' title", self.id),
        )?;
        check_len(
            modal_limits::MAX_COMPONENTS,
            "MAX_MODAL_COMPONENTS",
            self.inputs.len(),
            format!("modal '{}'", self.id),
        )?;
        let mut components = Vec::new();
        for input in &self.inputs {
            components.push(self.build_input(input)?);
        }
        let data_json = serde_json::to_vec(&data)?;
        let custom_id = custom_id_hash(&self.id, "submit", &data_json);
        app.db
            .store
            .insert(
                &custom_id,
                StoreRecord {
                    def_id: self.id.clone(),
                    exec_id: "submit".to_string(),
                    data_json,
                },
            )
            .await?;
        Ok(RenderedModal {
            custom_id,
            title: self.title.clone(),
            components,
        })
    }

    fn build_input(&self, input: &ModalInput) -> Result<CreateModalComponent<'static>> {
        let label_check = |label: &str| -> Result<()> {
            check_len(
                modal_limits::MAX_LABEL,
                "MAX_LABEL",
                label.chars().count(),
                format!("modal '{}' input label", self.id),
            )
        };
        match input {
            ModalInput::TextDisplay { content } => Ok(CreateModalComponent::TextDisplay(
                CreateTextDisplay::new(content.clone()).into_owned(),
            )),
            ModalInput::TextInput {
                id,
                label,
                style,
                placeholder,
                required,
                min_length,
                max_length,
            } => {
                label_check(label)?;
                let mut t = CreateInputText::new(*style, id.clone());
                if let Some(p) = placeholder {
                    t = t.placeholder(p.clone());
                }
                t = t.required(*required);
                if let Some(min) = min_length {
                    t = t.min_length(*min);
                }
                if let Some(max) = max_length {
                    t = t.max_length(*max);
                }
                Ok(CreateModalComponent::Label(
                    CreateLabel::input_text(label.clone(), t.into_owned()).into_owned(),
                ))
            }
            ModalInput::Select { id, label, select } => {
                label_check(label)?;
                let kind = match select {
                    ModalSelect::String(s) => {
                        check_len(
                            modal_limits::MAX_SELECT_OPTIONS,
                            "MAX_SELECT_OPTIONS",
                            s.options.len(),
                            format!("modal '{}' select '{}'", self.id, id),
                        )?;
                        let options = s
                            .options
                            .iter()
                            .map(|o| {
                                let mut opt =
                                    CreateSelectMenuOption::new(o.label.clone(), o.value.clone());
                                if let Some(d) = &o.description {
                                    opt = opt.description(d.clone());
                                }
                                if o.default {
                                    opt = opt.default_selection(true);
                                }
                                opt.into_owned()
                            })
                            .collect::<Vec<_>>();
                        CreateSelectMenuKind::String {
                            options: options.into(),
                        }
                    }
                    ModalSelect::User => CreateSelectMenuKind::User {
                        default_users: None,
                    },
                    ModalSelect::Role => CreateSelectMenuKind::Role {
                        default_roles: None,
                    },
                    ModalSelect::Mentionable => CreateSelectMenuKind::Mentionable {
                        default_users: None,
                        default_roles: None,
                    },
                    ModalSelect::Channel => CreateSelectMenuKind::Channel {
                        channel_types: None,
                        default_channels: None,
                    },
                };
                let mut menu = CreateSelectMenu::new(id.clone(), kind);
                if let ModalSelect::String(s) = select {
                    if let Some(min) = s.min_values {
                        menu = menu.min_values(min);
                    }
                    if let Some(max) = s.max_values {
                        menu = menu.max_values(max);
                    }
                    if let Some(req) = s.required {
                        menu = menu.required(req);
                    }
                }
                Ok(CreateModalComponent::Label(
                    CreateLabel::select_menu(label.clone(), menu.into_owned()).into_owned(),
                ))
            }
            ModalInput::FileUpload {
                id,
                label,
                min_values,
                max_values,
                required,
            } => {
                label_check(label)?;
                let mut f = CreateFileUpload::new(id.clone());
                if let Some(min) = min_values {
                    f = f.min_values(*min);
                }
                if let Some(max) = max_values {
                    f = f.max_values(*max);
                }
                if let Some(req) = required {
                    f = f.required(*req);
                }
                Ok(CreateModalComponent::Label(
                    CreateLabel::file_upload(label.clone(), f.into_owned()).into_owned(),
                ))
            }
            ModalInput::RadioGroup {
                id,
                label,
                options,
                required,
            } => {
                label_check(label)?;
                let opts = options
                    .iter()
                    .map(|o| {
                        let mut opt = CreateRadioGroupOption::new(o.label.clone(), o.value.clone());
                        if let Some(d) = &o.description {
                            opt = opt.description(d.clone());
                        }
                        if o.default {
                            opt = opt.default_selection(true);
                        }
                        opt.into_owned()
                    })
                    .collect::<Vec<_>>();
                let mut r = CreateRadioGroup::new(id.clone(), opts);
                if let Some(req) = required {
                    r = r.required(*req);
                }
                Ok(CreateModalComponent::Label(
                    CreateLabel::radio_group(label.clone(), r.into_owned()).into_owned(),
                ))
            }
            ModalInput::CheckboxGroup {
                id,
                label,
                options,
                min_values,
                max_values,
                required,
            } => {
                label_check(label)?;
                let opts = options
                    .iter()
                    .map(|o| {
                        let mut opt =
                            CreateCheckboxGroupOption::new(o.label.clone(), o.value.clone());
                        if let Some(d) = &o.description {
                            opt = opt.description(d.clone());
                        }
                        if o.default {
                            opt = opt.default_selection(true);
                        }
                        opt.into_owned()
                    })
                    .collect::<Vec<_>>();
                let mut c = CreateCheckboxGroup::new(id.clone(), opts);
                if let Some(min) = min_values {
                    c = c.min_values(*min);
                }
                if let Some(max) = max_values {
                    c = c.max_values(*max);
                }
                if let Some(req) = required {
                    c = c.required(*req);
                }
                Ok(CreateModalComponent::Label(
                    CreateLabel::checkbox_group(label.clone(), c.into_owned()).into_owned(),
                ))
            }
            ModalInput::Checkbox {
                id,
                label,
                default_selected,
            } => {
                label_check(label)?;
                let mut c = CreateCheckbox::new(id.clone());
                if *default_selected {
                    c = c.default_selected(true);
                }
                Ok(CreateModalComponent::Label(
                    CreateLabel::checkbox(label.clone(), c.into_owned()).into_owned(),
                ))
            }
        }
    }
}

impl<T, E> ModalErased<E> for ModalDef<T, E>
where
    T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    fn modal_id(&self) -> &str {
        &self.id
    }

    fn invoke_submit<'a>(
        &'a self,
        app: &'a App<E>,
        interaction: &'a ModalInteraction,
        data_json: &'a [u8],
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let data: T =
                serde_json::from_slice(data_json).map_err(|source| Error::Deserialize {
                    def_id: self.id.clone(),
                    source,
                })?;
            let wrapped = serenity::model::application::Interaction::Modal(interaction.clone());
            let ctx = crate::context::Ctx::new(app, &wrapped);
            let submission = ModalSubmission {
                modal_id: self.id.clone(),
                data: interaction.data.clone(),
            };
            (self.submit.f)(&ctx, data, submission).await
        })
    }
}

/// A submitted Modal form. Values are retrieved by the stable component IDs
/// used when the Modal was defined.
#[derive(Debug, Clone)]
pub struct ModalSubmission {
    pub(crate) modal_id: String,
    pub(crate) data: ModalInteractionData,
}

impl ModalSubmission {
    /// The Modal's globally unique ID.
    pub fn modal_id(&self) -> &str {
        &self.modal_id
    }

    fn parse_id<T>(&self, v: &str) -> T
    where
        T: std::str::FromStr + From<u64>,
        T::Err: std::fmt::Debug,
    {
        v.parse::<T>().unwrap_or_else(|_| T::from(0u64))
    }

    fn find(&self, component_id: &str) -> Result<LabelComponent> {
        for component in &self.data.components {
            if let ModalComponent::Label(Label { component, .. }) = component {
                let id = match component {
                    LabelComponent::SelectMenu(s) => s.custom_id.as_str(),
                    LabelComponent::InputText(t) => t.custom_id.as_str(),
                    LabelComponent::FileUpload(f) => f.custom_id.as_str(),
                    LabelComponent::RadioGroup(r) => r.custom_id.as_str(),
                    LabelComponent::CheckboxGroup(c) => c.custom_id.as_str(),
                    LabelComponent::Checkbox(c) => c.custom_id.as_str(),
                    _ => continue,
                };
                if id == component_id {
                    return Ok(component.clone());
                }
            }
        }
        Err(Error::MissingSubmissionValue {
            modal_id: self.modal_id.clone(),
            component_id: component_id.to_string(),
        })
    }

    fn invalid(&self, component_id: &str, expected: &'static str) -> Error {
        Error::InvalidSubmissionValue {
            modal_id: self.modal_id.clone(),
            component_id: component_id.to_string(),
            expected,
        }
    }

    /// The submitted text of a Text Input.
    pub fn text(&self, component_id: &str) -> Result<String> {
        match self.find(component_id)? {
            LabelComponent::InputText(t) => Ok(t.value.to_string()),
            _ => Err(self.invalid(component_id, "a text input")),
        }
    }

    /// The selected values of a String Select.
    pub fn string_select(&self, component_id: &str) -> Result<Vec<String>> {
        match self.find(component_id)? {
            LabelComponent::SelectMenu(s) => Ok(s.values.iter().map(|v| v.to_string()).collect()),
            _ => Err(self.invalid(component_id, "a select menu")),
        }
    }

    /// The selected users of a User Select, with resolved records.
    pub fn user_select(&self, component_id: &str) -> Result<Vec<UserChoice>> {
        match self.find(component_id)? {
            LabelComponent::SelectMenu(s) => Ok(s
                .values
                .iter()
                .map(|v| {
                    let id = self.parse_id::<UserId>(v.as_str());
                    UserChoice {
                        id,
                        user: self.data.resolved.users.get(&id).cloned(),
                    }
                })
                .collect()),
            _ => Err(self.invalid(component_id, "a select menu")),
        }
    }

    /// The selected roles of a Role Select.
    pub fn role_select(&self, component_id: &str) -> Result<Vec<RoleChoice>> {
        match self.find(component_id)? {
            LabelComponent::SelectMenu(s) => Ok(s
                .values
                .iter()
                .map(|v| {
                    let id = self.parse_id::<RoleId>(v.as_str());
                    RoleChoice {
                        id,
                        role: self.data.resolved.roles.get(&id).cloned(),
                    }
                })
                .collect()),
            _ => Err(self.invalid(component_id, "a select menu")),
        }
    }

    /// The selected channels of a Channel Select.
    pub fn channel_select(&self, component_id: &str) -> Result<Vec<ChannelChoice>> {
        match self.find(component_id)? {
            LabelComponent::SelectMenu(s) => Ok(s
                .values
                .iter()
                .map(|v| {
                    let id = GenericChannelId::new(self.parse_id::<u64>(v.as_str()));
                    ChannelChoice {
                        id,
                        channel: self.data.resolved.channels.get(&id).cloned(),
                    }
                })
                .collect()),
            _ => Err(self.invalid(component_id, "a select menu")),
        }
    }

    /// The selected users or roles of a Mentionable Select.
    pub fn mentionable_select(&self, component_id: &str) -> Result<Vec<MentionableChoice>> {
        match self.find(component_id)? {
            LabelComponent::SelectMenu(s) => Ok(s
                .values
                .iter()
                .map(|v| {
                    let raw: u64 = v.parse().unwrap_or(0);
                    let uid = self.parse_id::<UserId>(v.as_str());
                    let rid = self.parse_id::<RoleId>(v.as_str());
                    MentionableChoice {
                        id: GenericId::new(raw),
                        user: self.data.resolved.users.get(&uid).cloned(),
                        role: self.data.resolved.roles.get(&rid).cloned(),
                    }
                })
                .collect()),
            _ => Err(self.invalid(component_id, "a select menu")),
        }
    }

    /// The uploaded attachments of a File Upload input.
    pub fn files(&self, component_id: &str) -> Result<Vec<AttachmentId>> {
        match self.find(component_id)? {
            LabelComponent::FileUpload(f) => Ok(f.values.iter().copied().collect()),
            _ => Err(self.invalid(component_id, "a file upload")),
        }
    }

    /// The selected value of a Radio Group.
    pub fn radio(&self, component_id: &str) -> Result<String> {
        match self.find(component_id)? {
            LabelComponent::RadioGroup(r) => r
                .value
                .as_ref()
                .map(|v| v.to_string())
                .ok_or_else(|| self.invalid(component_id, "a radio selection")),
            _ => Err(self.invalid(component_id, "a radio group")),
        }
    }

    /// The selected values of a Checkbox Group.
    pub fn checkbox_group(&self, component_id: &str) -> Result<Vec<String>> {
        match self.find(component_id)? {
            LabelComponent::CheckboxGroup(c) => {
                Ok(c.values.iter().map(|v| v.to_string()).collect())
            }
            _ => Err(self.invalid(component_id, "a checkbox group")),
        }
    }

    /// The state of a Checkbox.
    pub fn checkbox(&self, component_id: &str) -> Result<bool> {
        match self.find(component_id)? {
            LabelComponent::Checkbox(c) => Ok(c.value),
            _ => Err(self.invalid(component_id, "a checkbox")),
        }
    }

    /// Deserialize the whole submission into a developer-defined struct.
    ///
    /// Component IDs map to struct fields; submitted values are converted to
    /// the field types. Missing or invalid values produce errors. Built on
    /// Serde, not a second serialization system.
    pub fn deserialize<F: serde::de::DeserializeOwned>(&self) -> Result<F> {
        let mut map = serde_json::Map::new();
        for component in &self.data.components {
            let ModalComponent::Label(Label { component, .. }) = component else {
                continue;
            };
            let (id, value) = match component {
                LabelComponent::InputText(t) => (
                    t.custom_id.to_string(),
                    serde_json::Value::String(t.value.to_string()),
                ),
                LabelComponent::SelectMenu(s) => (
                    s.custom_id.to_string(),
                    serde_json::json!(s.values.iter().map(|v| v.as_str()).collect::<Vec<_>>()),
                ),
                LabelComponent::FileUpload(f) => (
                    f.custom_id.to_string(),
                    serde_json::json!(
                        f.values
                            .iter()
                            .map(|a| a.get().to_string())
                            .collect::<Vec<_>>()
                    ),
                ),
                LabelComponent::RadioGroup(r) => (
                    r.custom_id.to_string(),
                    r.value
                        .as_ref()
                        .map(|v| serde_json::Value::String(v.to_string()))
                        .unwrap_or(serde_json::Value::Null),
                ),
                LabelComponent::CheckboxGroup(c) => (
                    c.custom_id.to_string(),
                    serde_json::json!(c.values.iter().map(|v| v.as_str()).collect::<Vec<_>>()),
                ),
                LabelComponent::Checkbox(c) => {
                    (c.custom_id.to_string(), serde_json::Value::Bool(c.value))
                }
                _ => continue,
            };
            map.insert(id, value);
        }
        serde_json::from_value(serde_json::Value::Object(map)).map_err(|source| {
            Error::Deserialize {
                def_id: self.modal_id.clone(),
                source,
            }
        })
    }
}

/// A registered Modal handle.
#[derive(Clone)]
pub struct RegisteredModal<T, E = ()> {
    pub(crate) def: Arc<ModalDef<T, E>>,
}

impl<T, E> RegisteredModal<T, E>
where
    T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    /// The globally unique Modal ID.
    pub fn id(&self) -> &str {
        &self.def.id
    }

    /// Render the Modal for the given data, producing a [`RenderedModal`]
    /// to open with `Ctx::open_modal`.
    pub fn render<'a>(
        &'a self,
        app: &'a App<E>,
        data: T,
    ) -> impl std::future::Future<Output = Result<RenderedModal>> + Send + 'a {
        self.def.render(app, data)
    }
}

/// The registry of registered Modals.
pub struct ModalRegistry<E> {
    pub(crate) entries: Mutex<HashMap<String, ModalEntry<E>>>,
}

pub(crate) struct ModalEntry<E> {
    pub erased: Arc<dyn ModalErased<E>>,
    pub any: Arc<dyn Any + Send + Sync>,
}

impl<E: Send + Sync + 'static> ModalRegistry<E> {
    pub(crate) fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn insert<T>(&self, id: &str, def: Arc<ModalDef<T, E>>) -> Result<()>
    where
        T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
        ModalDef<T, E>: ModalErased<E>,
    {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| Error::msg("registry poisoned"))?;
        if entries.contains_key(id) {
            return Err(Error::DuplicateModalId {
                modal_id: id.to_string(),
            });
        }
        entries.insert(
            id.to_string(),
            ModalEntry {
                erased: def.clone(),
                any: def,
            },
        );
        Ok(())
    }

    /// Look up a registered Modal by ID, preserving its concrete data type.
    pub fn query<T>(&self, id: &str) -> Result<RegisteredModal<T, E>>
    where
        T: Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
    {
        let entries = self
            .entries
            .lock()
            .map_err(|_| Error::msg("registry poisoned"))?;
        let entry = entries.get(id).ok_or_else(|| Error::UnknownModal {
            modal_id: id.to_string(),
        })?;
        let def = entry
            .any
            .clone()
            .downcast::<ModalDef<T, E>>()
            .map_err(|_| Error::DataMismatch {
                def_id: id.to_string(),
            })?;
        Ok(RegisteredModal { def })
    }

    pub(crate) fn erased(&self, id: &str) -> Result<Arc<dyn ModalErased<E>>> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| Error::msg("registry poisoned"))?;
        entries
            .get(id)
            .map(|e| e.erased.clone())
            .ok_or_else(|| Error::UnknownModal {
                modal_id: id.to_string(),
            })
    }
}
