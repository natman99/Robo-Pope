use poise::serenity_prelude as serenity;

use crate::data::Data;
use crate::level;
use crate::raw_api::FLAGS_COMPONENTS_V2;
use crate::settings_page;
use crate::Error;

/// TS selection_menus/level_settings_size.ts — PRO-only banner size.
pub async fn size(
    fw: poise::FrameworkContext<'_, Data, Error>,
    i: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let ctx = fw.serenity_context;
    let data = fw.user_data;

    let serenity::ComponentInteractionDataKind::StringSelect { values } = &i.data.kind else {
        return Ok(());
    };

    i.create_response(&ctx.http, serenity::CreateInteractionResponse::Acknowledge)
        .await?;

    let guild_id = i.guild_id.ok_or("select outside a guild")?.get();
    let mut settings = level::get_level_settings(&data.inner.db, i.user.id.get(), guild_id).await?;

    settings.is_large = values.first().map(|v| v == "large").unwrap_or(false);

    level::set_level_settings(&data.inner.db, &settings).await?;
    let components = settings_page::generate_components(fw, &settings).await?;
    data.inner
        .raw
        .edit_response_cv2(&i.token, &components, FLAGS_COMPONENTS_V2)
        .await?;
    Ok(())
}
