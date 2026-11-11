//! Leveling math, XP tracking, level-up reactions, and the MongoDB-backed
//! banner settings. Field names of `LevelSettings` (typos included) match the
//! existing MongoDB documents written by the TypeScript bot.

use mongodb::bson::{doc, to_document};
use mongodb::options::ReturnDocument;
use poise::serenity_prelude as serenity;

use crate::data::Data;
use crate::leaderboard;
use crate::Error;

/// <:Click_to_see_level:1484691165281255457> — reacted on every 10th level.
pub const LEVEL_EMOJI_BIG: u64 = 1484691165281255457;
/// <:Click_to_see_level:1484622933061271775> — reacted on other levels; also
/// the click target that replies with the level announcement.
pub const LEVEL_EMOJI_SMALL: u64 = 1484622933061271775;

#[derive(Clone, Copy, Debug)]
pub struct LevelInfo {
    pub level: u64,
    pub min_xp: u64,
    pub max_xp: u64,
    pub total_max_xp: u64,
}

/// Exact port of the TS `getLevel` (rebalanced curve: 200, +10 per level).
pub fn get_level(xp: u64) -> LevelInfo {
    let mut level = 0u64;
    let mut xp_pool = xp;
    let mut xp_to_next_lvl = 200u64;

    while xp_pool > xp_to_next_lvl {
        level += 1;
        xp_to_next_lvl += 10;
        xp_pool -= xp_to_next_lvl;
    }

    LevelInfo {
        level,
        min_xp: xp_pool,
        max_xp: xp_to_next_lvl,
        total_max_xp: xp + (xp_to_next_lvl - xp_pool),
    }
}

pub fn count_words(s: &str) -> u64 {
    s.split_whitespace().count() as u64
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LevelSettings {
    pub user_id: String,
    pub guild_id: Option<String>,
    pub frost: bool,
    pub primary_color: u32,
    pub primary_color_trans: f64,
    pub secondary_color: u32,
    pub secondary_color_trans: f64,
    pub text_color: u32,
    pub text_color_trans: f64,
    pub shadow_strength: f64,
    pub shadow_color: u32,
    // NOTE: the misspellings below are load-bearing — they are the field names
    // in the existing MongoDB collection.
    pub has_costome_background: bool,
    pub backgrond_url: String,
    pub is_large: bool,
}

impl LevelSettings {
    fn default_for(user_id: &str, guild_id: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            guild_id: Some(guild_id.to_string()),
            frost: false,
            primary_color: 0x5865f2,
            primary_color_trans: 1.0,
            secondary_color: 0xe0e3ff,
            secondary_color_trans: 1.0,
            text_color: 0xe0e3ff,
            text_color_trans: 1.0,
            shadow_color: 0x1a1a1a,
            shadow_strength: 1.0,
            has_costome_background: false,
            backgrond_url: String::new(),
            is_large: false,
        }
    }
}

/// findOneAndUpdate upsert, same as the TS `getLevelBannerSettings`.
pub async fn get_level_settings(
    db: &mongodb::Database,
    user_id: u64,
    guild_id: u64,
) -> Result<LevelSettings, Error> {
    let col = db.collection::<LevelSettings>("level_settings");
    let user_id = user_id.to_string();
    let guild_id = guild_id.to_string();

    let defaults = LevelSettings::default_for(&user_id, &guild_id);

    let result = col
        .find_one_and_update(
            doc! { "user_id": &user_id, "guild_id": &guild_id },
            doc! { "$setOnInsert": to_document(&defaults)? },
        )
        .return_document(ReturnDocument::After)
        .upsert(true)
        .await?;

    result.ok_or_else(|| "Failed to return a valid LevelSettings type from DB.".into())
}

/// updateOne $set upsert, same as the TS `setLevelBannerSettings`.
pub async fn set_level_settings(db: &mongodb::Database, s: &LevelSettings) -> Result<(), Error> {
    let col = db.collection::<LevelSettings>("level_settings");
    col.update_one(
        doc! { "user_id": &s.user_id, "guild_id": &s.guild_id },
        doc! { "$set": to_document(s)? },
    )
    .upsert(true)
    .await?;
    Ok(())
}

/// Port of TS `hexNumToStr`: 0xRRGGBB + alpha 0.0..=1.0 -> "#RRGGBBAA".
pub fn hex_num_to_str(hex: u32, alpha: f64) -> String {
    let rgb = format!("{:06X}", hex & 0xffffff);
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
    format!("#{rgb}{a:02X}")
}

pub fn add_words(data: &Data, guild_id: u64, user_id: u64, words: u64) {
    let mut words_map = data.inner.words.write().unwrap();
    *words_map
        .entry(guild_id)
        .or_default()
        .entry(user_id)
        .or_default() += words;
}

pub fn get_user_words(data: &Data, guild_id: u64, user_id: u64) -> u64 {
    data.inner
        .words
        .read()
        .unwrap()
        .get(&guild_id)
        .and_then(|g| g.get(&user_id))
        .copied()
        .unwrap_or(0)
}

/// Port of TS `handleLevel`: count words, react on level-up, refresh leaderboards.
pub async fn handle_level(
    fw: poise::FrameworkContext<'_, Data, Error>,
    msg: &serenity::Message,
) {
    let ctx = fw.serenity_context;
    let data = fw.user_data;

    if msg.author.bot || data.inner.counting.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Some(guild_id) = msg.guild_id else { return };

    add_words(data, guild_id.get(), msg.author.id.get(), count_words(&msg.content));

    let user_xp = get_user_words(data, guild_id.get(), msg.author.id.get());
    if user_xp == 0 {
        return;
    }

    let next_level = {
        let mut milestones = data.inner.milestones.write().unwrap();
        milestones
            .entry(guild_id.get())
            .or_default()
            .entry(msg.author.id.get())
            .or_insert_with(|| get_level(user_xp).total_max_xp)
            .clone()
    };

    if user_xp >= next_level {
        let info = get_level(user_xp);
        let emoji_id = if info.level % 10 == 0 {
            LEVEL_EMOJI_BIG
        } else {
            LEVEL_EMOJI_SMALL
        };
        let reaction = serenity::ReactionType::Custom {
            animated: false,
            id: serenity::EmojiId::new(emoji_id),
            name: Some("Click_to_see_level".to_string()),
        };
        if let Err(e) = msg.react(&ctx.http, reaction).await {
            eprintln!("failed to react to level-up message: {e}");
        }
        data
            .inner
            .milestones
            .write()
            .unwrap()
            .get_mut(&guild_id.get())
            .unwrap()
            .insert(msg.author.id.get(), info.total_max_xp);
    }

    // Live leaderboard refresh, same as the TS bot.
    for channel_id in &data.inner.config.leaderboard_channel_ids {
        if let Err(e) = leaderboard::update_leaderboard(ctx, data, *channel_id).await {
            eprintln!("leaderboard update failed for {channel_id}: {e}");
        }
    }
}

/// Port of TS `handleReaction`: clicking the small level emoji on your own
/// message replies with your level.
pub async fn handle_reaction(
    fw: poise::FrameworkContext<'_, Data, Error>,
    reaction: &serenity::Reaction,
) {
    let ctx = fw.serenity_context;
    let data = fw.user_data;

    if data.inner.counting.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Some(user_id) = reaction.user_id else { return };
    let serenity::ReactionType::Custom { id, .. } = &reaction.emoji else {
        return;
    };
    if id.get() != LEVEL_EMOJI_SMALL {
        return;
    }
    let Ok(message) = reaction.message(&ctx.http).await else {
        return;
    };
    if message.author.id != user_id {
        return;
    }
    let Some(guild_id) = message.guild_id else { return };

    let words = get_user_words(data, guild_id.get(), user_id.get());
    let level = get_level(words).level;

    if let Err(e) = message.reply(&ctx.http, format!("You made it to Level {level} :partying_face:")).await {
        eprintln!("failed to reply to level reaction: {e}");
    }
}

/// Snapshot of (user, xp) sorted by xp desc — used by the leaderboard.
pub fn top_users(data: &Data, guild_id: u64, limit: usize) -> Vec<(u64, u64)> {
    let mut entries: Vec<(u64, u64)> = data
        .inner
        .words
        .read()
        .unwrap()
        .get(&guild_id)
        .map(|g| g.iter().map(|(u, x)| (*u, *x)).collect())
        .unwrap_or_default();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    entries.truncate(limit);
    entries
}
