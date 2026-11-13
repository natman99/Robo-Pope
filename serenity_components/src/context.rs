//! Render-time and interaction-time contexts.

use std::marker::PhantomData;

use serenity::builder::{
    CreateInteractionResponse, CreateInteractionResponseFollowup, CreateInteractionResponseMessage,
    CreateModal, EditInteractionResponse, EditMessage,
};
use serenity::model::application::Interaction;
use serenity::model::channel::Message;
use serenity::model::id::{GenericChannelId, GuildId, InteractionId};
use serenity::model::user::User;

use crate::app::App;
use crate::error::{Error, Result};
use crate::render::{RenderedMessage, RenderedModal};

/// Context handed to dynamic value callbacks and Frame callbacks while a Page
/// is being rendered. It is temporary and never persisted.
pub struct RenderCtx<'a, T, E> {
    /// The application services.
    pub app: &'a App<E>,
    pub(crate) def_id: &'a str,
    pub(crate) data_json: &'a [u8],
    /// Custom IDs collected during rendering: (custom_id, exec_id).
    pub(crate) used: std::sync::Mutex<Vec<(String, String)>>,
    pub(crate) _t: PhantomData<fn() -> T>,
}

impl<'a, T, E> RenderCtx<'a, T, E> {
    pub(crate) fn new(app: &'a App<E>, def_id: &'a str, data_json: &'a [u8]) -> Self {
        Self {
            app,
            def_id,
            data_json,
            used: std::sync::Mutex::new(Vec::new()),
            _t: PhantomData,
        }
    }

    /// Record an interactive component's identity for persistence.
    pub(crate) fn record_exec(&self, custom_id: &str, exec_id: &str) {
        if let Ok(mut used) = self.used.lock() {
            used.push((custom_id.to_string(), exec_id.to_string()));
        }
    }

    /// Take the collected (custom_id, exec_id) pairs.
    pub(crate) fn take_used(&self) -> Vec<(String, String)> {
        self.used
            .lock()
            .map(|mut v| std::mem::take(&mut *v))
            .unwrap_or_default()
    }
}

/// The interaction context every execution receives.
///
/// It represents the current Discord interaction and provides access to both
/// the App and interaction-specific operations. It is temporary: it belongs
/// to the current interaction and is never stored inside a Page or persisted.
pub struct Ctx<'a, E> {
    /// The application services.
    pub app: &'a App<E>,
    /// The raw Discord interaction.
    pub interaction: &'a Interaction,
    /// Serialized `Selection<S>` for select executions, if any.
    pub(crate) selection_json: Option<serde_json::Value>,
}

impl<'a, E> Ctx<'a, E> {
    pub(crate) fn new(app: &'a App<E>, interaction: &'a Interaction) -> Self {
        Self {
            app,
            interaction,
            selection_json: None,
        }
    }

    fn component(&self) -> Result<&serenity::model::application::ComponentInteraction> {
        match self.interaction {
            Interaction::Component(i) => Ok(i),
            _ => Err(Error::msg("operation requires a component interaction")),
        }
    }

    fn token(&self) -> &str {
        match self.interaction {
            Interaction::Component(i) => i.token.as_str(),
            Interaction::Modal(i) => i.token.as_str(),
            Interaction::Command(i) | Interaction::Autocomplete(i) => i.token.as_str(),
            Interaction::Ping(i) => i.token.as_str(),
            _ => "",
        }
    }

    /// The interacting user.
    pub fn user(&self) -> &User {
        match self.interaction {
            Interaction::Component(i) => &i.user,
            Interaction::Modal(i) => &i.user,
            Interaction::Command(i) | Interaction::Autocomplete(i) => &i.user,
            Interaction::Ping(_) => unreachable!("ctx never built for pings"),
            _ => unreachable!("ctx never built for other interactions"),
        }
    }

    /// The guild this interaction occurred in, if any.
    pub fn guild_id(&self) -> Option<GuildId> {
        match self.interaction {
            Interaction::Component(i) => i.guild_id,
            Interaction::Modal(i) => i.guild_id,
            Interaction::Command(i) | Interaction::Autocomplete(i) => i.guild_id,
            Interaction::Ping(_) => None,
            _ => None,
        }
    }

    /// The channel this interaction occurred in.
    pub fn channel_id(&self) -> GenericChannelId {
        match self.interaction {
            Interaction::Component(i) => i.channel_id,
            Interaction::Modal(i) => i.channel_id,
            Interaction::Command(i) | Interaction::Autocomplete(i) => i.channel_id,
            Interaction::Ping(_) => unreachable!("ctx never built for pings"),
            _ => unreachable!(),
        }
    }

    /// The interaction ID.
    pub fn interaction_id(&self) -> InteractionId {
        self.interaction.id()
    }

    /// The message containing the component, for component interactions.
    pub fn message(&self) -> Option<&Message> {
        match self.interaction {
            Interaction::Component(i) => Some(&i.message),
            _ => None,
        }
    }

    /// The selected values of a selection menu execution.
    ///
    /// Selection values are interaction data; they are not part of the
    /// Page's serialized data.
    pub fn selection<S: serde::de::DeserializeOwned>(&self) -> Result<S> {
        let json = self
            .selection_json
            .clone()
            .ok_or_else(|| Error::msg("this execution has no selection values"))?;
        serde_json::from_value(json).map_err(|source| Error::Deserialize {
            def_id: "selection".to_string(),
            source,
        })
    }

    /// Reply to the interaction with a message.
    pub async fn reply(&self, content: impl Into<String>) -> Result<()> {
        let content: String = content.into();
        CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content(content))
            .execute(&self.app.http, self.interaction_id(), self.token())
            .await
            .map_err(Into::into)
    }

    /// Reply to the interaction with a full message builder.
    pub async fn reply_with(&self, msg: CreateInteractionResponseMessage<'_>) -> Result<()> {
        CreateInteractionResponse::Message(msg)
            .execute(&self.app.http, self.interaction_id(), self.token())
            .await
            .map_err(Into::into)
    }

    /// Defer the interaction response (the user sees a loading state).
    pub async fn defer(&self) -> Result<()> {
        CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new())
            .execute(&self.app.http, self.interaction_id(), self.token())
            .await
            .map_err(Into::into)
    }

    /// Acknowledge without a loading state; the original message may be
    /// edited later.
    pub async fn acknowledge(&self) -> Result<()> {
        CreateInteractionResponse::Acknowledge
            .execute(&self.app.http, self.interaction_id(), self.token())
            .await
            .map_err(Into::into)
    }

    /// Edit the message containing the component.
    pub async fn edit_message(&self, rendered: &RenderedMessage) -> Result<Message> {
        let i = self.component()?;
        let builder = EditMessage::new()
            .components(rendered.components.clone())
            .flags(rendered.flags);
        self.app
            .http
            .edit_message(i.channel_id, i.message.id, &builder, Vec::new())
            .await
            .map_err(Into::into)
    }

    /// Edit the original interaction response.
    pub async fn edit_response(&self, builder: EditInteractionResponse<'_>) -> Result<Message> {
        builder
            .execute(&self.app.http, self.token())
            .await
            .map_err(Into::into)
    }

    /// Send a follow-up message.
    pub async fn followup(
        &self,
        builder: CreateInteractionResponseFollowup<'_>,
    ) -> Result<Message> {
        builder
            .execute(&self.app.http, None, self.token())
            .await
            .map_err(Into::into)
    }

    /// Delete the interaction response.
    pub async fn delete_response(&self) -> Result<()> {
        self.app
            .http
            .delete_original_interaction_response(self.token())
            .await
            .map_err(Into::into)
    }

    /// Open a Modal as this interaction's response.
    ///
    /// Opening a Modal is an interaction response, not a message edit or
    /// follow-up.
    pub async fn open_modal(&self, rendered: &RenderedModal) -> Result<()> {
        let modal = CreateModal::new(rendered.custom_id.clone(), rendered.title.clone())
            .components(rendered.components.clone());
        CreateInteractionResponse::Modal(modal)
            .execute(&self.app.http, self.interaction_id(), self.token())
            .await
            .map_err(Into::into)
    }
}
