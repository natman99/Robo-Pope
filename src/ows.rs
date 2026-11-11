//! One Word Story enforcement: in the OWS channels, delete messages that are
//! not a single word or where the same user posted twice in a row.

use poise::serenity_prelude as serenity;

use crate::data::Data;
use crate::Error;

pub async fn handle_message(
    fw: poise::FrameworkContext<'_, Data, Error>,
    msg: &serenity::Message,
) {
    let ctx = fw.serenity_context;
    let data = fw.user_data;

    if !data.inner.config.ows_channel_ids.contains(&msg.channel_id.get()) {
        return;
    }

    let before = match msg
        .channel_id
        .messages(&ctx.http, serenity::GetMessages::new().limit(1).before(msg.id))
        .await
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ows: failed to fetch previous message: {e}");
            return;
        }
    };
    if let Some(prev) = before.first() {
        if prev.author.id == msg.author.id {
            let _ = msg.delete(&ctx.http).await;
            return;
        }
    }

    // TS regex /^\S+$/ — non-empty and no whitespace.
    if msg.content.is_empty() || msg.content.chars().any(char::is_whitespace) {
        let _ = msg.delete(&ctx.http).await;
    }
}
