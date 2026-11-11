//! Color modals (primary/secondary/text) and the background modal.
//!
//! serenity 0.12 modals are the legacy action-row format: text inputs only.
//! Values come back in `data.components` as InputText rows.

use poise::serenity_prelude as serenity;

use crate::banner;
use crate::data::Data;
use crate::level;
use crate::raw_api::FLAGS_COMPONENTS_V2;
use crate::settings_page;
use crate::Error;

pub async fn color_modal_prompt(
    fw: poise::FrameworkContext<'_, Data, Error>,
    i: &serenity::ComponentInteraction,
    kind: &str,
    title: &str,
) -> Result<(), Error> {
    let ctx = fw.serenity_context;

    let guild_id = i
        .guild_id
        .or(i.message.guild_id)
        .ok_or("no guild")?
        .get();
    let settings = level::get_level_settings(&fw.user_data.inner.db, i.user.id.get(), guild_id).await?;

    let (color, trans) = match kind {
        "primery" => (settings.primary_color, settings.primary_color_trans),
        "secondary" => (settings.secondary_color, settings.secondary_color_trans),
        _ => (settings.text_color, settings.text_color_trans),
    };
    let current_color: String = level::hex_num_to_str(color & 0xffffff, 0.0)[..7].to_string();
    let current_trans = format!("{trans:.1}");

    let modal = serenity::CreateModal::new(format!("level_settings_set_{kind}_color_modal"), title)
        .components(vec![
            serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Short,
                    "Set The Color",
                    format!("level_settings_set_{kind}_color_modal_color"),
                )
                .placeholder("#RRGGBB")
                .value(current_color)
                .required(true)
                .min_length(7)
                .max_length(7),
            ),
            serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Short,
                    "Set The Transparency",
                    format!("level_settings_set_{kind}_color_modal_trans"),
                )
                .placeholder("1.0 for fully opaque")
                .value(current_trans)
                .required(true)
                .min_length(3)
                .max_length(5),
            ),
        ]);

    i.create_response(&ctx.http, serenity::CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

/// Pull a text input value out of a modal submit by custom id.
fn modal_text(i: &serenity::ModalInteraction, custom_id: &str) -> Option<String> {
    for row in &i.data.components {
        for component in &row.components {
            if let serenity::ActionRowComponent::InputText(input) = component {
                if input.custom_id == custom_id {
                    return input.value.clone();
                }
            }
        }
    }
    None
}

fn valid_hex(color: &str) -> Option<u32> {
    let rest = color.strip_prefix('#')?;
    if rest.len() != 6 || !rest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(rest, 16).ok()
}

/// TS regex /^[0-9]+(\.[0-9]+)?$/
fn valid_trans(trans: &str) -> Option<f64> {
    let core: &str = trans.trim();
    if core.is_empty() {
        return None;
    }
    let mut parts = core.split('.');
    let int_part = parts.next().unwrap_or("");
    let frac_part = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return None;
    }
    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !frac_part.is_empty() && !frac_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    core.parse::<f64>().ok()
}

pub async fn color_modal_submit(
    fw: poise::FrameworkContext<'_, Data, Error>,
    i: &serenity::ModalInteraction,
    kind: &str,
) -> Result<(), Error> {
    let ctx = fw.serenity_context;
    let data = fw.user_data;

    let color_raw = modal_text(i, &format!("level_settings_set_{kind}_color_modal_color"));
    let trans_raw = modal_text(i, &format!("level_settings_set_{kind}_color_modal_trans"));

    let Some(color) = color_raw.as_deref().and_then(valid_hex) else {
        i.create_response(
            &ctx.http,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .ephemeral(true)
                    .content("Improperly formatted color. Try again"),
            ),
        )
        .await?;
        return Ok(());
    };
    let Some(trans) = trans_raw.as_deref().and_then(valid_trans) else {
        i.create_response(
            &ctx.http,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .ephemeral(true)
                    .content("Improperly formatted transparency value. Try again"),
            ),
        )
        .await?;
        return Ok(());
    };
    let trans = trans.min(1.0);

    i.create_response(&ctx.http, serenity::CreateInteractionResponse::Acknowledge)
        .await?;

    let guild_id = i.guild_id.ok_or("modal outside a guild")?.get();
    let mut settings = level::get_level_settings(&data.inner.db, i.user.id.get(), guild_id).await?;

    match kind {
        "primery" => {
            settings.primary_color = color;
            settings.primary_color_trans = trans;
        }
        "secondary" => {
            settings.secondary_color = color;
            settings.secondary_color_trans = trans;
        }
        _ => {
            settings.text_color = color;
            settings.text_color_trans = trans;
        }
    }

    level::set_level_settings(&data.inner.db, &settings).await?;
    let components = settings_page::generate_components(fw, &settings).await?;
    data.inner
        .raw
        .edit_response_cv2(&i.token, &components, FLAGS_COMPONENTS_V2)
        .await?;
    Ok(())
}

pub async fn background_modal_submit(
    fw: poise::FrameworkContext<'_, Data, Error>,
    i: &serenity::ModalInteraction,
) -> Result<(), Error> {
    let ctx = fw.serenity_context;
    let data = fw.user_data;

    let style = modal_text(i, "level_settings_set_backgrond_modal_style")
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let url = modal_text(i, "level_settings_set_backgrond_modal_image")
        .unwrap_or_default()
        .trim()
        .to_string();

    i.create_response(&ctx.http, serenity::CreateInteractionResponse::Acknowledge)
        .await?;

    let guild_id = i.guild_id.ok_or("modal outside a guild")?.get();
    let mut settings = level::get_level_settings(&data.inner.db, i.user.id.get(), guild_id).await?;

    match style.as_str() {
        "transparent" => {
            settings.has_costome_background = false;
            settings.backgrond_url = String::new();
        }
        "image" => {
            if url.is_empty() {
                // Nothing to set; the deferred update already refreshed the page.
                let components = settings_page::generate_components(fw, &settings).await?;
                data.inner
                    .raw
                    .edit_response_cv2(&i.token, &components, FLAGS_COMPONENTS_V2)
                    .await?;
                return Ok(());
            }
            let uri = banner::background_data_uri(data.inner.raw.http(), &url).await?;
            settings.has_costome_background = true;
            settings.backgrond_url = uri;
        }
        _ => {}
    }

    level::set_level_settings(&data.inner.db, &settings).await?;
    let components = settings_page::generate_components(fw, &settings).await?;
    data.inner
        .raw
        .edit_response_cv2(&i.token, &components, FLAGS_COMPONENTS_V2)
        .await?;
    Ok(())
}
