use poise::serenity_prelude as serenity;

use crate::banner;
use crate::level::{self, get_level};
use crate::{Context, Error};

/// Get your level
// (TS commands/level.ts — ephemeral banner + Share/Settings buttons.)
#[poise::command(slash_command)]
pub async fn level(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let data = ctx.data();

    if data.inner.counting.load(std::sync::atomic::Ordering::Relaxed) {
        ctx.reply("Come back later, I'm still tallying up the score.")
            .await?;
        return Ok(());
    }

    let Some(guild_id) = ctx.guild_id() else {
        ctx.reply("Could not get Server's data").await?;
        return Ok(());
    };
    let user_id = ctx.author().id.get();

    let words = {
        let words_map = data.inner.words.read().unwrap();
        words_map
            .get(&guild_id.get())
            .map(|guild_map| guild_map.get(&user_id).copied())
    };
    let words = match words {
        Some(Some(words)) => words,
        None => {
            ctx.reply("Could not get Server's data").await?;
            return Ok(());
        }
        Some(None) => {
            ctx.reply("Could not get your data").await?;
            return Ok(());
        }
    };

    let settings = level::get_level_settings(&data.inner.db, user_id, guild_id.get()).await?;

    let author = ctx.author();
    let info = get_level(words);
    let image_path = data
        .inner
        .renderer
        .render(
            &data.inner.config,
            &settings,
            &author.display_name(),
            author.avatar_url().as_deref(),
            info,
        )
        .await?;

    let attachment = serenity::CreateAttachment::path(&image_path).await?;

    let buttons = vec![
        serenity::CreateButton::new("level_share")
            .label("Share")
            .style(serenity::ButtonStyle::Primary),
        serenity::CreateButton::new("level_settings")
            .emoji(serenity::ReactionType::Unicode("⚙️".to_string()))
            .style(serenity::ButtonStyle::Secondary),
    ];

    ctx.send(
        poise::CreateReply::new()
            .attachment(attachment)
            .components(vec![serenity::CreateActionRow::Buttons(buttons)]),
    )
    .await?;

    banner::delete_if_exists(&image_path);
    Ok(())
}
