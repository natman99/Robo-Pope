use poise::serenity_prelude as serenity;

use crate::{Context, Error};

/// Excommunicate a user
// (TS commands/excommunicate.ts — ban, with an ecclesiastical flourish.)
#[poise::command(
    slash_command,
    default_member_permissions = "BAN_MEMBERS"
)]
pub async fn excommunicate(
    ctx: Context<'_>,
    #[description = "The user to Excommunicate"] user: serenity::User,
) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.send(poise::CreateReply::new().content("Failed to get guild").ephemeral(true))
            .await?;
        return Ok(());
    };

    let Ok(member) = guild_id.member(ctx.http(), user.id).await else {
        ctx.send(poise::CreateReply::new().content("Failed to get the member").ephemeral(true))
            .await?;
        return Ok(());
    };

    // Refuse to excommunicate administrators (TS `canBan` check).
    // Member::permissions is deprecated but matches the TS semantics (base
    // role permissions, channel overwrites irrelevant for a ban check).
    #[allow(deprecated)]
    let member_perms = member.permissions(ctx.cache()).unwrap_or_default();
    if member_perms.contains(serenity::Permissions::ADMINISTRATOR) {
        ctx.send(
            poise::CreateReply::new()
                .content("Failed to get users permissions")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let my_id = ctx.cache().current_user().id;
    let me = guild_id.member(ctx.http(), my_id).await?;
    #[allow(deprecated)]
    let my_perms = me.permissions(ctx.cache()).unwrap_or_default();
    if !my_perms.contains(serenity::Permissions::BAN_MEMBERS) {
        ctx.send(
            poise::CreateReply::new()
                .content("Dose Not have ban Perms")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let guild = guild_id.to_partial_guild(ctx.http()).await?;
    user
        .create_dm_channel(ctx.http())
        .await?
        .send_message(
            ctx.http(),
            serenity::CreateMessage::new().content(format!("You have been Excommunicated from {}", guild.name)),
        )
        .await?;

    guild_id.ban(ctx.http(), user.id, 0).await?;
    ctx.reply(format!("{} has been Excommunicated", user.display_name()))
        .await?;
    Ok(())
}
