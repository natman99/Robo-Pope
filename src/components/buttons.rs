use poise::serenity_prelude as serenity;

use crate::data::Data;
use crate::raw_api::FLAGS_COMPONENTS_V2;
use crate::settings_page;
use crate::Error;

/// TS button/level_settings.ts — ephemeral settings page.
pub async fn level_settings(
    fw: poise::FrameworkContext<'_, Data, Error>,
    i: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let ctx = fw.serenity_context;
    let data = fw.user_data;

    i.create_response(
        &ctx.http,
        serenity::CreateInteractionResponse::Defer(
            serenity::CreateInteractionResponseMessage::new().ephemeral(true),
        ),
    )
    .await?;

    let guild_id = i
        .guild_id
        .or(i.message.guild_id)
        .ok_or("no guild")?
        .get();
    let settings = crate::level::get_level_settings(&data.inner.db, i.user.id.get(), guild_id).await?;
    let components = settings_page::generate_components(fw, &settings).await?;

    data.inner
        .raw
        .edit_response_cv2(&i.token, &components, FLAGS_COMPONENTS_V2)
        .await?;
    Ok(())
}

/// TS button/level_share.ts — repost the banner attachment into the channel.
pub async fn level_share(
    fw: poise::FrameworkContext<'_, Data, Error>,
    i: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let ctx = fw.serenity_context;

    let url = i.message.attachments.first().map(|a| a.url.clone());

    i.create_response(&ctx.http, serenity::CreateInteractionResponse::Acknowledge)
        .await?;

    if let Some(url) = url {
        let attachment = serenity::CreateAttachment::url(&ctx.http, &url).await?;
        i.channel_id
            .send_message(&ctx.http, serenity::CreateMessage::new().files([attachment]))
            .await?;
    }
    Ok(())
}

/// TS button/update.ts — dev-only self-update via update.sh.
pub async fn update(
    fw: poise::FrameworkContext<'_, Data, Error>,
    i: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let ctx = fw.serenity_context;
    let data = fw.user_data;

    if data.inner.config.developer_ids.contains(&i.user.id.to_string()) {
        let github_token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
        // Fire-and-forget like the TS `spawn(...).unref()`.
        std::process::Command::new("./update.sh")
            .arg(github_token)
            .spawn()?;
    } else {
        i.create_response(
            &ctx.http,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .content("You have found your self a bug. How did you get this?"),
            ),
        )
        .await?;
        eprintln!("A User that is not a DEVELOPER has clicked the update Button. Something has gone very wrong");
    }
    Ok(())
}

/// TS button/send_update_massage.ts — the v0.2.0 announcement embed.
pub async fn send_update_message(
    fw: poise::FrameworkContext<'_, Data, Error>,
    _i: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let ctx = fw.serenity_context;
    let data = fw.user_data;

    const ANNOUNCEMENT: &str = r#"# Update v0.2.0 - Bundle of Banners Update
Dear,
@everyone
Welcome one and all to all new Robo-Pope version 0.2.0. In this update much has happened and much is still to come. I Robo-Pope have cooked up a delicious consommé of features for you my lovely brethren and *cough cough clare cough* to enjoy. Now lets not kid ourself and get right into the action!
## 1. Leaderboard
I have added a leaderboard so that all of you can compete for getting no maidens. The leaderboard will show the top 10 biggest yappers in the server; and it updates Live! The leaderboard is in the <#1484688796342550569> channel.
## 2. Sharing your Level Banner
I have made it so that you can share you Level Banner. If you run `/level` you will see that under the banner there are 2 new buttons. The first of those buttons is a share button and it will enable you to post your Level Banner in the chat you are talking in.
## 3. One Word Story automatic enforcement.
I made it so that <#1486807946246492260> will now be automatically enforced by me.
## 4. Level System Rebalance
The old level system was to hard for people to level up late into yapping and to easy for people that just started. I rebalanced the level system. So yes your level will now be different. :pensive:
## 5. ALL NEW Level Banners
I have added all new Level Banners and now you can customize your Level Banner! You will be able to change the following:
- Primary color
- Secondary color
- Text color
- Background image <:por1:1487227954898538496><:por2:1487227945348239564>
- Banner size <:por1:1487227954898538496><:por2:1487227945348239564>
Unfortunately the Background image and Banner size options are only for <@&1425570661165961369>, <@&1483145047716266084>,<@&1425988510896095282>, and <@&1484252302704509038>. You can change your banners look by clicking the second button under the Level Banner.

If you want to see my source code then you can go the the [github](https://github.com/rocketman510/Robo-Pope)
Yours truly,
Robo-Pope"#;

    let embed = serenity::CreateEmbed::new()
        .color(0x29292D)
        .title(" ")
        .description(ANNOUNCEMENT);

    for id in &data.inner.config.announcement_channel_ids {
        serenity::ChannelId::new(*id)
            .send_message(&ctx.http, serenity::CreateMessage::new().embed(embed.clone()))
            .await?;
    }
    Ok(())
}

/// TS button/level_settings_background.ts.
///
/// DEVIATION: the TS modal used a components-v2 FileUpload. serenity 0.12
/// cannot parse modal file uploads (the event fails to deserialize), so the
/// image is supplied by URL instead — the TS pipeline fetched + cropped the URL
/// into a data URI anyway when uploads failed.
pub async fn background_modal(
    fw: poise::FrameworkContext<'_, Data, Error>,
    i: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let ctx = fw.serenity_context;

    let modal = serenity::CreateModal::new("level_settings_set_backgrond_modal", "Backgrond Image")
        .components(vec![
            serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Short,
                    "Set Backgrond Style:",
                    "level_settings_set_backgrond_modal_style",
                )
                .placeholder("image or transparent")
                .required(true)
                .min_length(1)
                .max_length(11),
            ),
            serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Short,
                    "Backgrond Image URL",
                    "level_settings_set_backgrond_modal_image",
                )
                .placeholder("https://... (only needed for style 'image')")
                .required(false)
                .max_length(2000),
            ),
        ]);

    i.create_response(&ctx.http, serenity::CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}
