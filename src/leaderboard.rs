//! Live leaderboard (Components V2 container in the leaderboard channels).
//!
//! The TS version derived the up/down/movement arrows by parsing the old
//! message's text displays; serenity 0.12 drops those components on receive, so
//! the previous order is kept in memory (`prev_top`) instead.

use poise::serenity_prelude as serenity;
use serde_json::Value;

use crate::cv2;
use crate::data::Data;
use crate::level::get_level;
use crate::raw_api::FLAGS_COMPONENTS_V2;
use crate::Error;

const BAR: u64 = 1486853453094453439;
const BAR_UP: u64 = 1486850337146146907;
const BAR_DOWN: u64 = 1486850346054582272;

fn emoji(id: u64, name: &str) -> String {
    format!("<:{name}:{id}>")
}

pub async fn update_leaderboard(
    ctx: &serenity::Context,
    data: &Data,
    channel_id: u64,
) -> Result<(), Error> {
    let cid = serenity::ChannelId::new(channel_id);
    let channel = cid
        .to_channel(&ctx.http)
        .await?
        .guild()
        .ok_or_else(|| format!("Not a valid channel at ID: {channel_id}"))?;
    let guild_id = channel.guild_id.get();

    let messages = cid
        .messages(&ctx.http, serenity::GetMessages::new().limit(1))
        .await?;
    let bot_id = ctx.cache.current_user().id;

    if messages.is_empty() {
        let components = generate_leaderboard(data, guild_id)?;
        data.inner
            .raw
            .send_cv2(channel_id, &components, FLAGS_COMPONENTS_V2)
            .await?;
    } else if messages[0].author.id == bot_id {
        let components = generate_leaderboard(data, guild_id)?;
        data.inner
            .raw
            .edit_channel_cv2(channel_id, messages[0].id.get(), &components, FLAGS_COMPONENTS_V2)
            .await?;
    }
    Ok(())
}

fn generate_leaderboard(data: &Data, guild_id: u64) -> Result<Vec<Value>, Error> {
    let sorted = crate::level::top_users(data, guild_id, 10);

    let old: Vec<u64> = data
        .inner
        .prev_top
        .read()
        .unwrap()
        .get(&guild_id)
        .cloned()
        .unwrap_or_default();

    let mut children = vec![cv2::text_display("# Leaderboard")];
    let mut new_order = Vec::new();

    for (i, (user_id, xp)) in sorted.iter().enumerate() {
        let level = get_level(*xp).level;
        let bar = match old.iter().position(|u| *u == *user_id) {
            None => emoji(BAR, "bar"),
            Some(old_i) if old_i == i => emoji(BAR, "bar"),
            Some(old_i) if i > old_i => emoji(BAR_DOWN, "bar_down"),
            Some(_) => emoji(BAR_UP, "bar_up"),
        };
        children.push(cv2::text_display(format!(
            "{bar}  **{}. <@{user_id}> - {level} Lvl. - {xp} xp**",
            i + 1
        )));
        new_order.push(*user_id);
    }

    *data.inner.prev_top.write().unwrap().entry(guild_id).or_default() = new_order;

    Ok(vec![cv2::container(0x242429, children)])
}
