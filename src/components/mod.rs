//! Button / select-menu / modal dispatch, mirroring the TS `button/`,
//! `selection_menus/` and `modals/` folders (dispatch by custom_id).

mod buttons;
mod modals;
mod selects;

use poise::serenity_prelude as serenity;

use crate::data::Data;
use crate::Error;

pub async fn handle_component(
    fw: poise::FrameworkContext<'_, Data, Error>,
    i: &serenity::ComponentInteraction,
) {
    let result = match i.data.custom_id.as_str() {
        "level_settings" => buttons::level_settings(fw, i).await,
        "level_share" => buttons::level_share(fw, i).await,
        "update" => buttons::update(fw, i).await,
        "send_update_massage" => buttons::send_update_message(fw, i).await,
        "level_settings_background" => buttons::background_modal(fw, i).await,
        "level_settings_set_primery_color" => modals::color_modal_prompt(fw, i, "primery", "Primery Color").await,
        "level_settings_set_secondary_color" => modals::color_modal_prompt(fw, i, "secondary", "Secondary Color").await,
        "level_settings_set_text_color" => modals::color_modal_prompt(fw, i, "text", "text Color").await,
        "level_settings_size" => selects::size(fw, i).await,
        other => {
            eprintln!("unhandled component custom_id: {other}");
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("component handler {} failed: {e}", i.data.custom_id);
    }
}

pub async fn handle_modal(
    fw: poise::FrameworkContext<'_, Data, Error>,
    i: &serenity::ModalInteraction,
) {
    let result = match i.data.custom_id.as_str() {
        "level_settings_set_primery_color_modal" => modals::color_modal_submit(fw, i, "primery").await,
        "level_settings_set_secondary_color_modal" => modals::color_modal_submit(fw, i, "secondary").await,
        "level_settings_set_text_color_modal" => modals::color_modal_submit(fw, i, "text").await,
        "level_settings_set_backgrond_modal" => modals::background_modal_submit(fw, i).await,
        other => {
            eprintln!("unhandled modal custom_id: {other}");
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("modal handler {} failed: {e}", i.data.custom_id);
    }
}
