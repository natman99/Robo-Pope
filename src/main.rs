//! Robo-Pope — Discord bot (Rust rewrite of the TypeScript original).
//!
//! serenity + poise. Startup mirrors deploy.ts: clear cache, connect DB,
//! tally message history into word counts, seed XP milestones, refresh
//! leaderboard channels.

mod banner;
mod commands;
mod components;
mod config;
mod cv2;
mod data;
mod leaderboard;
mod level;
mod ows;
mod raw_api;
mod settings_page;

use std::sync::atomic::Ordering;

use poise::serenity_prelude as serenity;
use serenity::FullEvent;

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
pub type Context<'a> = poise::Context<'a, data::Data, Error>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    let token = std::env::var("DISCORD_TOKEN").ok().ok_or("No DISCORD_TOKEN ENV")?;

    let framework = poise::Framework::builder()
        .setup(|ctx, _ready, _framework| Box::pin(data::setup(ctx)))
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::ping::ping(),
                commands::level::level(),
                commands::update::update(),
                commands::spam::spam(),
                commands::stopspam::stopspam(),
                commands::excommunicate::excommunicate(),
            ],
            event_handler: |fw, event| Box::pin(on_event(fw, event)),
            on_error: |error| Box::pin(on_error(error)),
            ..Default::default()
        })
        .build();

    // Same intents as the TS client.
    let intents = serenity::GatewayIntents::GUILDS
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::GUILD_MEMBERS
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_MESSAGE_REACTIONS;

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await?;

    client.start().await?;
    Ok(())
}

async fn on_event(
    fw: poise::FrameworkContext<'_, data::Data, Error>,
    event: &FullEvent,
) -> Result<(), Error> {
    let ctx = fw.serenity_context;
    let data = fw.user_data;

    match event {
        FullEvent::Ready { data_about_bot } => {
            println!("Ready! Logged in as {}", data_about_bot.user.name);

            // Slash command registration (TS deploy.ts `rest.put`).
            if let Err(e) = poise::builtins::register_globally(&ctx.http, &fw.options.commands).await
            {
                eprintln!("Failed to register slash commands: {e}");
            }

            // History tally runs in the background like the TS startup.
            let ctx2 = ctx.clone();
            let data2 = data.clone();
            tokio::spawn(async move {
                if let Err(e) = startup(ctx2, data2.clone()).await {
                    eprintln!("Startup failed: {e}");
                    data2
                        .inner
                        .raw
                        .report_error(data2.inner.config.error_channel_id, &e.to_string())
                        .await;
                }
            });
        }
        FullEvent::Message { new_message } => {
            ows::handle_message(fw, new_message).await;
            level::handle_level(fw, new_message).await;
        }
        FullEvent::ReactionAdd { add_reaction } => {
            level::handle_reaction(fw, add_reaction).await;
        }
        FullEvent::InteractionCreate { interaction } => match interaction {
            // Command/Autocomplete interactions are dispatched by poise itself.
            serenity::Interaction::Component(i) => components::handle_component(fw, i).await,
            serenity::Interaction::Modal(i) => components::handle_modal(fw, i).await,
            _ => {}
        },
        _ => {}
    }
    Ok(())
}

/// deploy.ts equivalent: fetch all message history, seed milestones, refresh
/// leaderboard channels. `counting` gates XP accrual until the tally is done.
async fn startup(ctx: serenity::Context, data: data::Data) -> Result<(), Error> {
    println!("Fetching Messages...");
    data.inner.counting.store(true, Ordering::Relaxed);

    for guild_id in ctx.cache.guilds() {
        let Ok(channels) = guild_id.channels(&ctx.http).await else {
            continue;
        };
        for (channel_id, channel) in channels {
            if !matches!(
                channel.kind,
                serenity::ChannelType::Text
                    | serenity::ChannelType::News
                    | serenity::ChannelType::Voice
                    | serenity::ChannelType::Stage
                    | serenity::ChannelType::Forum
            ) {
                continue;
            }

            let mut before: Option<serenity::MessageId> = None;
            loop {
                let mut builder = serenity::GetMessages::new().limit(100);
                if let Some(id) = before {
                    builder = builder.before(id);
                }
                let messages = match channel_id.messages(&ctx.http, builder).await {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("history fetch failed in {channel_id}: {e}");
                        break;
                    }
                };
                if messages.is_empty() {
                    break;
                }
                for msg in &messages {
                    if let Some(g) = msg.guild_id {
                        level::add_words(&data, g.get(), msg.author.id.get(), level::count_words(&msg.content));
                    }
                }
                if messages.len() < 100 {
                    break;
                }
                before = messages.last().map(|m| m.id);
            }
        }
    }
    println!("DONE");

    // deploy_xp: seed each user's next level-up milestone.
    {
        let words = data.inner.words.read().unwrap();
        let mut milestones = data.inner.milestones.write().unwrap();
        for (guild_id, users) in words.iter() {
            for (user_id, word_count) in users.iter() {
                milestones
                    .entry(*guild_id)
                    .or_default()
                    .insert(*user_id, level::get_level(*word_count).total_max_xp);
            }
        }
    }

    data.inner.counting.store(false, Ordering::Relaxed);

    println!("Clearing leaderbord Channel");
    for channel_id in data.inner.config.leaderboard_channel_ids.clone() {
        if let Err(e) = leaderboard::update_leaderboard(&ctx, &data, channel_id).await {
            eprintln!("leaderboard init failed for {channel_id}: {e}");
        }
    }
    Ok(())
}

async fn on_error(error: poise::FrameworkError<'_, data::Data, Error>) {
    match error {
        poise::FrameworkError::Setup { error, .. } => {
            eprintln!("Startup error: {error}");
        }
        poise::FrameworkError::Command { error, ctx, .. } => {
            eprintln!("Error in command `{}`: {error}", ctx.command().name);
            let data = ctx.data().clone();
            data.inner
                .raw
                .report_error(data.inner.config.error_channel_id, &error.to_string())
                .await;
        }
        poise::FrameworkError::EventHandler { error, .. } => {
            eprintln!("Event handler error: {error}");
        }
        e => {
            eprintln!("Framework error: {e}");
        }
    }
}
