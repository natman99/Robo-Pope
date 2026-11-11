use std::sync::atomic::Ordering;
use std::time::Duration;

use poise::serenity_prelude as serenity;

use crate::{Context, Error};

/// spam someone's DM's.
// (TS commands/spam.ts — DM spam loop, stoppable via /stopspam.)
#[poise::command(slash_command)]
pub async fn spam(
    ctx: Context<'_>,
    #[description = "The user to Spam"] user: serenity::User,
    #[description = "The message to DM them"] message: String,
    #[description = "Number of message (-1 for nonstop)"] number: i64,
) -> Result<(), Error> {
    let username = ctx.author().name.clone();
    println!("{username}");

    // Same substring allowlist as the TS `'rocketman510 jorby_official'.includes(...)`.
    if !("rocketman510 jorby_official").contains(&username) {
        ctx.send(
            poise::CreateReply::new()
                .content(" you have no perms to run this D:")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    if number == 0 {
        ctx.send(
            poise::CreateReply::new()
                .content("Invaled Number")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    ctx.send(poise::CreateReply::new().content("Started to Spam").ephemeral(true))
        .await?;

    let data = ctx.data();
    let dm = user.create_dm_channel(ctx.http()).await?;

    let mut i: i64 = 0;
    while !data.inner.stop_spam.load(Ordering::Relaxed) && i != number {
        i += 1;
        tokio::time::sleep(Duration::from_secs(1)).await;
        if let Err(e) = dm
            .send_message(ctx.http(), serenity::CreateMessage::new().content(&message))
            .await
        {
            eprintln!("spam: dm send failed: {e}");
            break;
        }
    }

    if data.inner.stop_spam.load(Ordering::Relaxed) {
        data.inner.stop_spam.store(false, Ordering::Relaxed);
    }
    Ok(())
}
