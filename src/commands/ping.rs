use crate::{Context, Error};

/// ping the command.
// (TS commands/ping.ts — username-gated responses.)
#[poise::command(slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let username = ctx.author().name.clone();
    println!("{username}");

    if username == "rocketman510" {
        ctx.reply("Pong").await?;
    } else if username == "jorby_official" {
        ctx.reply("https://tenor.com/view/we-do-not-care-juan-gif-22047769")
            .await?;
    } else {
        ctx.reply("L bozo. You think I have to care what you say? No...")
            .await?;
    }
    Ok(())
}
