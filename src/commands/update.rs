use std::process::Command;

use poise::serenity_prelude as serenity;

use crate::cv2;
use crate::raw_api::{FLAGS_COMPONENTS_V2, FLAGS_EPHEMERAL};
use crate::{Context, Error};

/// This command updates the bot to the latest version.
// (TS commands/update.ts — dev-only version check against GitHub releases.)
#[poise::command(slash_command)]
pub async fn update(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    println!("{}", ctx.author().name);

    if !data
        .inner
        .config
        .developer_ids
        .contains(&ctx.author().id.to_string())
    {
        ctx.send(
            poise::CreateReply::new()
                .content("You are not a developer you can not use this command.")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let version = get_version();
    let latest = get_latest_version(data).await?;
    let tag_message = get_latest_tag_message();

    if latest.as_deref() == Some(version.as_str()) {
        let button = serenity::CreateButton::new("send_update_massage")
            .label("Send Message")
            .style(serenity::ButtonStyle::Primary);
        ctx.send(
            poise::CreateReply::new()
                .content(format!("The Bot up-to-date. Version: `{version}`"))
                .ephemeral(true)
                .components(vec![serenity::CreateActionRow::Buttons(vec![button])]),
        )
        .await?;
    } else {
        // Components V2 section with a button accessory (raw JSON, see cv2.rs).
        let section = cv2::section(
            vec![
                cv2::text_display(format!(
                    "The Bot has an update.\nCurrent: `{version}` => `{}`",
                    latest.as_deref().unwrap_or("none")
                )),
                cv2::text_display(format!("```dif\n{tag_message}\n```")),
            ],
            cv2::button("update", "Update the Bot", 3, false),
        );
        let action_row = cv2::action_row(vec![cv2::button("send_update_massage", "Send Message", 1, false)]);

        let token = match ctx {
            poise::Context::Application(app) => app.interaction.token.clone(),
            _ => return Err("update must be invoked as a slash command".into()),
        };
        data.inner
            .raw
            .respond_cv2(
                &token,
                &[section, action_row],
                FLAGS_COMPONENTS_V2 | FLAGS_EPHEMERAL,
            )
            .await?;
    }
    Ok(())
}

/// `git describe --tags --exact-match 2>/dev/null || git rev-parse --short HEAD`
fn get_version() -> String {
    let exact = Command::new("git")
        .args(["describe", "--tags", "--exact-match"])
        .output();
    match exact {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => {
            let short = Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            short
        }
    }
}

async fn get_latest_version(data: &crate::data::Data) -> Result<Option<String>, Error> {
    let token = &data.inner.config.github_token;
    if token.is_empty() {
        return Err("No GITHUB_TOKEN".into());
    }
    let resp = data
        .inner
        .raw
        .http()
        .get("https://api.github.com/repos/rocketman510/Robo-Pope/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub API error: {}", resp.status()).into());
    }

    let body: serde_json::Value = resp.json().await?;
    Ok(body.get("tag_name").and_then(|t| t.as_str()).map(String::from))
}

fn get_latest_tag_message() -> String {
    let tag = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if tag.is_empty() {
        return "No tag message".to_string();
    }
    let message = Command::new("git")
        .args(["tag", "-l", "--format=%(contents)", &tag])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if message.is_empty() {
        "No tag message".to_string()
    } else {
        message
    }
}
