use std::sync::atomic::Ordering;

use crate::{Context, Error};

/// Stop Spaming
// (TS commands/stopspam.ts.)
#[poise::command(slash_command)]
pub async fn stopspam(ctx: Context<'_>) -> Result<(), Error> {
    let username = ctx.author().name.clone();

    if ("rocketman510 jorby_official").contains(&username) {
        ctx.data().inner.stop_spam.store(true, Ordering::Relaxed);
        ctx.send(
            poise::CreateReply::new()
                .content("Stoped Spaming")
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            poise::CreateReply::new()
                .content(" you have no perms to run this D:")
                .ephemeral(true),
        )
        .await?;
    }
    Ok(())
}
