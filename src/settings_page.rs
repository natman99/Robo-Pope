//! The "Level Banner Settings" page — the Components V2 message shown by the
//! settings button and re-rendered after every change (TS
//! functions/level_settings.ts `generateComponents`).

use poise::serenity_prelude as serenity;
use serde_json::Value;

use crate::banner;
use crate::cv2;
use crate::data::Data;
use crate::level::{get_level, LevelSettings};
use crate::Error;

const PRO_EMOJIS: &str = "<:pro1:1487227954898538496><:pro2:1487227945348239564>";

pub async fn generate_components(
    fw: poise::FrameworkContext<'_, Data, Error>,
    settings: &LevelSettings,
) -> Result<Vec<Value>, Error> {
    let ctx = fw.serenity_context;
    let data = fw.user_data;
    let config = &data.inner.config;

    let guild_id = settings
        .guild_id
        .as_deref()
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or("LevelSettings has no valid guild_id")?;
    let user_id = settings
        .user_id
        .parse::<u64>()
        .map_err(|_| "LevelSettings has no valid user_id")?;

    let member = serenity::GuildId::new(guild_id)
        .member(&ctx.http, user_id)
        .await?;
    let is_pro = member
        .roles
        .iter()
        .any(|r| config.pro_roles.contains(&r.get()));

    let user = serenity::UserId::new(user_id).to_user(&ctx.http).await?;
    let words = crate::level::get_user_words(data, guild_id, user_id);
    let info = get_level(words);

    // Render the preview banner, upload it to the cache channel to get a URL
    // for the media gallery (same trick as the TS version).
    let image_path = data
        .inner
        .renderer
        .render(
            config,
            settings,
            &user.display_name(),
            user.avatar_url().as_deref(),
            info,
        )
        .await?;

    let attachment = serenity::CreateAttachment::path(&image_path).await?;
    let cache_msg = serenity::ChannelId::new(config.cache_channel_id)
        .send_message(
            &ctx.http,
            serenity::CreateMessage::new().files([attachment]),
        )
        .await?;
    banner::delete_if_exists(&image_path);

    let image_url = cache_msg
        .attachments
        .first()
        .map(|a| a.url.clone())
        .ok_or("cache channel upload returned no attachment")?;

    let height = if settings.is_large { 125 } else { (if settings.has_costome_background { 8 } else { 0 }) + 22 };
    let width = if settings.is_large { 512 } else { (if settings.has_costome_background { 8 } else { 0 }) + 256 };

    let color_container = |accent: u32, label: &str, note: &str, custom_id: &str| {
        cv2::container(
            accent,
            vec![
                cv2::text_display(format!("**{label}:**\n-# {note}")),
                cv2::action_row(vec![cv2::button(custom_id, "Set Color", 2, false)]),
            ],
        )
    };

    Ok(vec![
        cv2::container(
            0x242429,
            vec![cv2::text_display(
                "# Level Banner Settings\nThis is the Settings page for configuring the look of your Level Banner. If you don't understand how to set the color then check out our resource [here](<https://htmlcolorcodes.com/>).\nIf you are interested in unlocking **PRO** features consider **Boosting** the server to unlock them.",
            )],
        ),
        color_container(
            settings.primary_color,
            "Primery Color",
            "This is the Color of the Bar.",
            "level_settings_set_primery_color",
        ),
        color_container(
            settings.secondary_color,
            "Secondary Color",
            "This is the Color of the Bar's backgrond",
            "level_settings_set_secondary_color",
        ),
        color_container(
            settings.text_color,
            "Text Color",
            "This is the Color of the Text",
            "level_settings_set_text_color",
        ),
        cv2::container(
            0x242429,
            vec![
                cv2::text_display(format!(
                    "**Backgrond Image {PRO_EMOJIS}:**\n-# This is the image that will be the backgrond\nResolution: {height}x{width}"
                )),
                cv2::action_row(vec![cv2::button(
                    "level_settings_background",
                    "Set Backgrond Image",
                    2,
                    !is_pro,
                )]),
            ],
        ),
        cv2::container(
            0x242429,
            vec![
                cv2::text_display(format!(
                    "**Banner Size {PRO_EMOJIS}:**\n-# You can set the size of the Banner to Small or Large"
                )),
                cv2::action_row(vec![cv2::size_select(
                    &format!("Change Size ({})", if settings.is_large { "Large" } else { "Small" }),
                    !is_pro,
                )]),
            ],
        ),
        cv2::container(
            0x242429,
            vec![
                cv2::text_display("**Preciew:**\n-# This what the Banner will look like."),
                cv2::media_gallery(&image_url, "Image 1"),
            ],
        ),
        cv2::container(
            0xFFAA00,
            vec![cv2::text_display(
                "### :warning: Refresh time can vary, be patient and don't try to use it between Refreshs",
            )],
        ),
    ])
}
