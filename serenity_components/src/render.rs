//! Rendered outputs: messages and modals ready to send.

use serenity::builder::{CreateComponent, CreateMessage, CreateModalComponent};
use serenity::model::channel::MessageFlags;
use serenity::model::id::GenericChannelId;

use crate::error::Result;

/// A rendered Components V2 message.
///
/// Components V2 messages require [`MessageFlags::IS_COMPONENTS_V2`]; the
/// framework sets it. Traditional content and embeds are not used.
#[derive(Debug, Clone)]
pub struct RenderedMessage {
    /// The top-level components.
    pub components: Vec<CreateComponent<'static>>,
    /// Always contains `IS_COMPONENTS_V2`.
    pub flags: MessageFlags,
}

impl RenderedMessage {
    pub(crate) fn new(components: Vec<CreateComponent<'static>>) -> Self {
        Self {
            components,
            flags: MessageFlags::IS_COMPONENTS_V2,
        }
    }

    /// Apply these components to a [`CreateMessage`] builder.
    pub fn apply(self, builder: CreateMessage<'static>) -> CreateMessage<'static> {
        builder.components(self.components).flags(self.flags)
    }

    /// Send this message to a channel.
    pub async fn send(
        &self,
        http: &serenity::http::Http,
        channel_id: GenericChannelId,
    ) -> Result<serenity::model::channel::Message> {
        let builder = self.clone().apply(CreateMessage::new());
        http.send_message(channel_id, Vec::new(), &builder)
            .await
            .map_err(Into::into)
    }
}

/// A rendered Modal, produced by `RegisteredModal::render` and opened with
/// `Ctx::open_modal`.
#[derive(Debug, Clone)]
pub struct RenderedModal {
    /// The deterministic custom ID identifying this rendered instance.
    pub custom_id: String,
    /// The modal title.
    pub title: String,
    /// The modal's components.
    pub components: Vec<CreateModalComponent<'static>>,
}

impl RenderedModal {
    /// Build the serenity `CreateModal` for this render.
    pub fn to_create_modal(&self) -> serenity::builder::CreateModal<'static> {
        serenity::builder::CreateModal::new(self.custom_id.clone(), self.title.clone())
            .components(self.components.clone())
    }
}
