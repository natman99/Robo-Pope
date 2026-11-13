//! Selection menus.
//!
//! A Page exposes one method for adding a selection menu; the menu itself
//! determines its type. Selected values are interaction data delivered to
//! the execution callback, not part of the Page's serialized data. Entity
//! selects expose the resolved Discord entities, not raw IDs only.

use serenity::builder::{CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption};
use serenity::model::channel::ChannelType;
use serenity::model::id::{GenericChannelId, RoleId, UserId};

use crate::callback::{ChannelChoice, Exec, MentionableChoice, RoleChoice, UserChoice};
use crate::component::{check_len, limits};
use crate::context::RenderCtx;
use crate::db::custom_id_hash;
use crate::error::{Error, Result};
use crate::value::Value;

/// An option in a String Select. Labels and descriptions accept static or
/// dynamic values.
#[derive(Debug)]
pub struct SelectOption<T, E> {
    pub(crate) label: Value<String, T, E>,
    pub(crate) value: String,
    pub(crate) description: Option<Value<String, T, E>>,
    pub(crate) default: bool,
}

impl<T: Clone + Send + Sync + 'static, E> SelectOption<T, E> {
    /// An option with a label (static or dynamic) and a value.
    pub fn new(label: impl Into<Value<String, T, E>>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            description: None,
            default: false,
        }
    }

    /// Set the option description (static or dynamic).
    #[must_use]
    pub fn set_description(mut self, description: impl Into<Value<String, T, E>>) -> Self {
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

/// Shared settings across all select kinds.
#[derive(Debug)]
pub(crate) struct SelectSettings<T, E> {
    pub placeholder: Option<Value<String, T, E>>,
    pub min_values: Option<u8>,
    pub max_values: Option<u8>,
    pub required: Option<bool>,
    pub disabled: Option<Value<bool, T, E>>,
}

impl<T, E> Default for SelectSettings<T, E> {
    fn default() -> Self {
        Self {
            placeholder: None,
            min_values: None,
            max_values: None,
            required: None,
            disabled: None,
        }
    }
}

macro_rules! select_builder {
    (
        $(#[$meta:meta])*
        $name:ident, $exec_ty:ty $(, $field:ident : $fty:ty)*
    ) => {
        $(#[$meta])*
        #[derive(Debug)]
        pub struct $name<T, E> {
            pub(crate) exec_id: String,
            pub(crate) settings: SelectSettings<T, E>,
            pub(crate) exec: Exec<T, E, $exec_ty>,
            $(pub(crate) $field: $fty,)*
        }

        impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> $name<T, E> {
            /// A select with a placeholder (static or dynamic), a page-unique
            /// execution ID and an execution callback.
            pub fn new(
                placeholder: impl Into<Value<String, T, E>>,
                exec_id: impl Into<String>,
                exec: impl Into<Exec<T, E, $exec_ty>>,
            ) -> Self {
                Self {
                    exec_id: exec_id.into(),
                    settings: SelectSettings {
                        placeholder: Some(placeholder.into()),
                        ..Default::default()
                    },
                    exec: exec.into(),
                    $($field: Default::default(),)*
                }
            }

            /// Minimum selections.
            #[must_use]
            pub fn set_min_values(mut self, min: u8) -> Self {
                self.settings.min_values = Some(min);
                self
            }

            /// Maximum selections.
            #[must_use]
            pub fn set_max_values(mut self, max: u8) -> Self {
                self.settings.max_values = Some(max);
                self
            }

            /// Whether a selection is required (modals).
            #[must_use]
            pub fn set_required(mut self, required: bool) -> Self {
                self.settings.required = Some(required);
                self
            }

            /// Whether the menu is disabled (static or dynamic).
            #[must_use]
            pub fn set_disabled(mut self, disabled: impl Into<Value<bool, T, E>>) -> Self {
                self.settings.disabled = Some(disabled.into());
                self
            }

            pub(crate) fn exec_id(&self) -> &str {
                &self.exec_id
            }
        }
    };
}

select_builder!(
    /// A user select menu. The callback receives the selected users with
    /// their resolved records.
    UserSelect, Vec<UserChoice>, defaults: Vec<UserId>
);
select_builder!(
    /// A role select menu.
    RoleSelect, Vec<RoleChoice>, defaults: Vec<RoleId>
);
select_builder!(
    /// A mentionable (user or role) select menu.
    MentionableSelect, Vec<MentionableChoice>,
    default_users: Vec<UserId>, default_roles: Vec<RoleId>
);
select_builder!(
    /// A channel select menu.
    ChannelSelect, Vec<ChannelChoice>,
    channel_types: Vec<ChannelType>, defaults: Vec<GenericChannelId>
);

macro_rules! entity_defaults {
    ($name:ident, $($method:ident => $field:ident : $ty:ty),+) => {
        impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> $name<T, E> {
            $(
                /// Preselect entities.
                #[must_use]
                pub fn $method(mut self, ids: impl IntoIterator<Item = $ty>) -> Self {
                    self.$field = ids.into_iter().collect();
                    self
                }
            )+
        }
    };
}

entity_defaults!(UserSelect, set_defaults => defaults: UserId);
entity_defaults!(RoleSelect, set_defaults => defaults: RoleId);
entity_defaults!(MentionableSelect, set_user_defaults => default_users: UserId,
                 set_role_defaults => default_roles: RoleId);
entity_defaults!(ChannelSelect, set_channel_types => channel_types: ChannelType,
                 set_defaults => defaults: GenericChannelId);

/// A String Select menu with up to 25 options.
#[derive(Debug)]
pub struct StringSelect<T, E> {
    pub(crate) exec_id: String,
    pub(crate) settings: SelectSettings<T, E>,
    pub(crate) options: Vec<SelectOption<T, E>>,
    pub(crate) exec: Exec<T, E, Vec<String>>,
}

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> StringSelect<T, E> {
    /// A string select: placeholder (static or dynamic), page-unique
    /// execution ID, options, and an execution callback receiving the
    /// selected values.
    pub fn new(
        placeholder: impl Into<Value<String, T, E>>,
        exec_id: impl Into<String>,
        options: impl IntoIterator<Item = SelectOption<T, E>>,
        exec: impl Into<Exec<T, E, Vec<String>>>,
    ) -> Self {
        Self {
            exec_id: exec_id.into(),
            settings: SelectSettings {
                placeholder: Some(placeholder.into()),
                ..Default::default()
            },
            options: options.into_iter().collect(),
            exec: exec.into(),
        }
    }

    /// Minimum selections.
    #[must_use]
    pub fn set_min_values(mut self, min: u8) -> Self {
        self.settings.min_values = Some(min);
        self
    }

    /// Maximum selections.
    #[must_use]
    pub fn set_max_values(mut self, max: u8) -> Self {
        self.settings.max_values = Some(max);
        self
    }

    /// Whether a selection is required (modals).
    #[must_use]
    pub fn set_required(mut self, required: bool) -> Self {
        self.settings.required = Some(required);
        self
    }

    /// Whether the menu is disabled (static or dynamic).
    #[must_use]
    pub fn set_disabled(mut self, disabled: impl Into<Value<bool, T, E>>) -> Self {
        self.settings.disabled = Some(disabled.into());
        self
    }

    pub(crate) fn exec_id(&self) -> &str {
        &self.exec_id
    }
}

/// One selection menu of any kind; the Page API accepts this single
/// abstraction.
#[derive(Debug)]
pub enum SelectMenu<T, E> {
    String(StringSelect<T, E>),
    User(UserSelect<T, E>),
    Role(RoleSelect<T, E>),
    Mentionable(MentionableSelect<T, E>),
    Channel(ChannelSelect<T, E>),
}

macro_rules! menu_from {
    ($($variant:ident : $ty:ident),+) => {
        $(
            impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> From<$ty<T, E>>
                for SelectMenu<T, E>
            {
                fn from(v: $ty<T, E>) -> Self {
                    Self::$variant(v)
                }
            }
        )+
    };
}

menu_from!(String: StringSelect, User: UserSelect, Role: RoleSelect,
           Mentionable: MentionableSelect, Channel: ChannelSelect);

impl<T: Clone + Send + Sync + 'static, E: Send + Sync + 'static> SelectMenu<T, E> {
    pub(crate) fn exec_id(&self) -> &str {
        match self {
            Self::String(s) => s.exec_id(),
            Self::User(s) => s.exec_id(),
            Self::Role(s) => s.exec_id(),
            Self::Mentionable(s) => s.exec_id(),
            Self::Channel(s) => s.exec_id(),
        }
    }

    pub(crate) async fn resolve(
        &self,
        rctx: &RenderCtx<'_, T, E>,
        data: &T,
    ) -> Result<CreateSelectMenu<'static>> {
        let exec_id = self.exec_id().to_string();
        let custom_id = custom_id_hash(rctx.def_id, &exec_id, rctx.data_json);
        rctx.record_exec(&custom_id, &exec_id);

        let (settings, kind) = match self {
            Self::String(s) => {
                check_len(
                    limits::MAX_SELECT_OPTIONS,
                    "MAX_SELECT_OPTIONS",
                    s.options.len(),
                    "string select",
                )?;
                if s.options.is_empty() {
                    return Err(Error::InvalidComponent {
                        kind: "string select",
                        detail: "a string select needs at least one option".into(),
                    });
                }
                let mut options = Vec::new();
                for opt in &s.options {
                    let label = opt.label.resolve(rctx, data).await?;
                    check_len(
                        limits::MAX_OPTION_LABEL,
                        "MAX_OPTION_LABEL",
                        label.chars().count(),
                        "select option label",
                    )?;
                    let mut o = CreateSelectMenuOption::new(label, opt.value.clone());
                    if let Some(d) = &opt.description {
                        let d = d.resolve(rctx, data).await?;
                        check_len(
                            limits::MAX_OPTION_DESCRIPTION,
                            "MAX_OPTION_DESCRIPTION",
                            d.chars().count(),
                            "select option description",
                        )?;
                        o = o.description(d);
                    }
                    if opt.default {
                        o = o.default_selection(true);
                    }
                    options.push(o.into_owned());
                }
                (
                    &s.settings,
                    CreateSelectMenuKind::String {
                        options: options.into(),
                    },
                )
            }
            Self::User(s) => (
                &s.settings,
                CreateSelectMenuKind::User {
                    default_users: if s.defaults.is_empty() {
                        None
                    } else {
                        Some(s.defaults.clone().into())
                    },
                },
            ),
            Self::Role(s) => (
                &s.settings,
                CreateSelectMenuKind::Role {
                    default_roles: if s.defaults.is_empty() {
                        None
                    } else {
                        Some(s.defaults.clone().into())
                    },
                },
            ),
            Self::Mentionable(s) => (
                &s.settings,
                CreateSelectMenuKind::Mentionable {
                    default_users: if s.default_users.is_empty() {
                        None
                    } else {
                        Some(s.default_users.clone().into())
                    },
                    default_roles: if s.default_roles.is_empty() {
                        None
                    } else {
                        Some(s.default_roles.clone().into())
                    },
                },
            ),
            Self::Channel(s) => (
                &s.settings,
                CreateSelectMenuKind::Channel {
                    channel_types: if s.channel_types.is_empty() {
                        None
                    } else {
                        Some(s.channel_types.clone().into())
                    },
                    default_channels: if s.defaults.is_empty() {
                        None
                    } else {
                        Some(s.defaults.clone().into())
                    },
                },
            ),
        };

        let mut b = CreateSelectMenu::new(custom_id, kind);
        if let Some(p) = &settings.placeholder {
            let p = p.resolve(rctx, data).await?;
            check_len(
                limits::MAX_PLACEHOLDER,
                "MAX_PLACEHOLDER",
                p.chars().count(),
                "placeholder",
            )?;
            b = b.placeholder(p);
        }
        if let Some(min) = settings.min_values {
            b = b.min_values(min);
        }
        if let Some(max) = settings.max_values {
            b = b.max_values(max);
        }
        if let Some(required) = settings.required {
            b = b.required(required);
        }
        if let Some(disabled) = &settings.disabled {
            b = b.disabled(disabled.resolve(rctx, data).await?);
        }
        Ok(b.into_owned())
    }
}

impl<T: Clone + serde::de::DeserializeOwned + Send + Sync + 'static, E: Send + Sync + 'static>
    SelectMenu<T, E>
{
    pub(crate) fn collect_execs(
        &self,
        out: &mut Vec<(String, std::sync::Arc<dyn crate::callback::ErasedExec<E>>)>,
    ) {
        use std::sync::Arc;
        match self {
            Self::String(s) => out.push((s.exec_id.clone(), Arc::new(s.exec.clone()))),
            Self::User(s) => out.push((s.exec_id.clone(), Arc::new(s.exec.clone()))),
            Self::Role(s) => out.push((s.exec_id.clone(), Arc::new(s.exec.clone()))),
            Self::Mentionable(s) => out.push((s.exec_id.clone(), Arc::new(s.exec.clone()))),
            Self::Channel(s) => out.push((s.exec_id.clone(), Arc::new(s.exec.clone()))),
        }
    }
}
