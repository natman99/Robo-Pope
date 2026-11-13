//! # serenity_components
//!
//! A declarative framework for building Discord messages and interactions
//! using Components V2, on top of Serenity.
//!
//! A [`Page`] describes a message: its component hierarchy, the values its
//! components display, and the executions attached to interactive
//! components. An [`App`] manages registered Pages, database persistence,
//! rendering and interaction routing. Building a Discord interface should
//! feel like composing ordinary Rust values; Serenity's component builders,
//! custom IDs, serialization and routing are implementation details.
//!
//! Four main concepts:
//!
//! - [`App`]: owns application state, the database, registered Pages and
//!   interaction routing.
//! - [`Page<T, E>`]: a blueprint for a Discord message, parameterized by
//!   its data.
//! - [`Modal<T, E>`]: a blueprint for a form submitted through an
//!   interaction.
//! - Frame ([`frame`]): a dynamic region inside a Page that resolves its
//!   own component hierarchy at render time and can paginate its contents.
//!
//! ## Example
//!
//! ```ignore
//! use serenity_components::prelude::*;
//!
//! #[derive(Clone, serde::Serialize, serde::Deserialize)]
//! pub struct Dataless;
//!
//! let app = App::memory(http);
//!
//! let page: RegisteredPage<Dataless> = Page::<Dataless>::new("Hello")
//!     .add_text("Hello, world!")
//!     .add_button_row((
//!         Some(Button::new("Show Level", "ButtonId", ButtonStyle::Secondary,
//!             exec(|ctx, data| Box::pin(async move {
//!                 ctx.reply("clicked").await
//!             })))),
//!         None, None, None, None,
//!     ))
//!     .register(&app)?;
//!
//! let message = page.render(&app, Dataless).await?;
//! ```

#![allow(clippy::type_complexity)]

pub mod app;
pub mod callback;
pub mod component;
pub mod context;
pub mod db;
pub mod error;
pub mod modal;
pub mod page;
pub mod render;
pub mod select;
pub mod value;

pub use app::{App, AppInner};
pub use callback::{
    ChannelChoice, Exec, MentionableChoice, RoleChoice, SelectValues, Selection, SubmitFn,
    SubmittedFiles, UserChoice, exec, frame, select_exec, submit,
};
pub use component::{
    Accessory, Button, ButtonRow, Component, Container, ContainerChild, Element, File, LinkButton,
    MediaGallery, MediaItem, Section, Separator, Text, Thumbnail, limits,
};
pub use context::{Ctx, RenderCtx};
pub use db::{Database, MemoryStore, Store, StoreRecord, custom_id_hash};
pub use error::{Error, Result};
pub use modal::{
    Modal, ModalErased, ModalInput, ModalOption, ModalRegistry, ModalSelect, ModalSubmission,
    RegisteredModal, modal_limits,
};
pub use page::{Page, PageData, PageDef, PageErased, PageItem, PageRegistry, RegisteredPage};
pub use render::{RenderedMessage, RenderedModal};
pub use select::{
    ChannelSelect, MentionableSelect, RoleSelect, SelectMenu, SelectOption, StringSelect,
    UserSelect,
};
pub use value::{BoxFuture, DynFn, Slot, SlotValue, Value, dynamic};

/// Serenity types the framework surfaces in its public API.
pub mod serenity_types {
    pub use serenity::model::application::{
        ButtonStyle, InputTextStyle, Interaction, SeparatorSpacingSize,
    };
    pub use serenity::model::channel::ChannelType;
    pub use serenity::model::id::{
        AttachmentId, ChannelId, GenericChannelId, GenericId, GuildId, MessageId, RoleId, UserId,
    };
    pub use serenity::model::user::User;
}

/// Convenient imports.
pub mod prelude {
    pub use crate::app::{App, AppInner};
    pub use crate::callback::{
        ChannelChoice, Exec, MentionableChoice, RoleChoice, Selection, UserChoice, exec, frame,
        select_exec, submit,
    };
    pub use crate::component::{
        Accessory, Button, ButtonRow, Component, Container, Element, File, LinkButton,
        MediaGallery, MediaItem, Section, Separator, Thumbnail,
    };
    pub use crate::context::{Ctx, RenderCtx};
    pub use crate::db::{Database, Store, StoreRecord};
    pub use crate::error::{Error, Result};
    pub use crate::modal::{
        Modal, ModalInput, ModalOption, ModalSelect, ModalSubmission, RegisteredModal,
    };
    pub use crate::page::{Page, PageData, RegisteredPage};
    pub use crate::render::{RenderedMessage, RenderedModal};
    pub use crate::select::{
        ChannelSelect, MentionableSelect, RoleSelect, SelectMenu, SelectOption, StringSelect,
        UserSelect,
    };
    pub use crate::serenity_types::*;
    pub use crate::value::{Slot, Value, dynamic};
}
