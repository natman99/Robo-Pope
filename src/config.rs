use std::path::PathBuf;

use crate::Error;

/// Everything the TS version read from `process.env`, parsed once at startup.
/// Env names are kept identical to the TypeScript bot for deployment parity.
#[derive(Clone, Debug)]
pub struct Config {
    pub client_id: u64,
    pub developer_ids: Vec<String>,
    pub github_token: String,
    pub cache_path: PathBuf,
    pub cache_channel_id: u64,
    pub leaderboard_channel_ids: Vec<u64>,
    pub pro_roles: Vec<u64>,
    pub ows_channel_ids: Vec<u64>,
    pub announcement_channel_ids: Vec<u64>,
    pub error_channel_id: u64,
    pub dev_mode: bool,
    /// Font used for the level banner (closest system match to Discord's Ginto Nord).
    pub font_family: String,
}

fn env_str(name: &str) -> Result<String, Error> {
    std::env::var(name).map_err(|_| format!("No {name} ENV").into())
}

/// Mirrors the TS `JSON.parse(ensure(process.env.X))` arrays of numeric string ids.
fn env_ids(name: &str) -> Result<Vec<u64>, Error> {
    let raw = env_str(name)?;
    let ids: Vec<String> = serde_json::from_str(&raw).map_err(|e| format!("Bad JSON in {name}: {e}"))?;
    ids.iter()
        .map(|s| {
            s.parse::<u64>()
                .map_err(|_| format!("Invalid id '{s}' in {name}").into())
        })
        .collect()
}

pub fn from_env() -> Result<Config, Error> {
    let developer_ids: Vec<String> = {
        let raw = std::env::var("DEVELOPER_IDS").unwrap_or_else(|_| "[]".into());
        serde_json::from_str(&raw).unwrap_or_default()
    };
    let dev_mode = matches!(std::env::var("DEV_MODE").as_deref(), Ok("true") | Ok("1"));

    Ok(Config {
        client_id: env_str("CLIENT_ID")?.parse()?,
        developer_ids,
        github_token: std::env::var("GITHUB_TOKEN").unwrap_or_default(),
        cache_path: PathBuf::from(std::env::var("CACHE_PATH").unwrap_or_else(|_| "./cache/".into())),
        cache_channel_id: env_str("CACHE_CHANNEL_ID")?.parse()?,
        leaderboard_channel_ids: env_ids("LEADERBOARD_CHANNEL_ID")?,
        pro_roles: env_ids("LEVEL_PRO_SETTINGS_ROLES")?,
        ows_channel_ids: env_ids("ONE_WORD_STORY_IDS")?,
        announcement_channel_ids: env_ids("ANNOUNCEMENT_CHANNAL_IDS")?,
        error_channel_id: env_str("ERROR_CHANNEL_ID")?.parse()?,
        dev_mode,
        font_family: std::env::var("FONT_FAMILY").unwrap_or_else(|_| "Noto Sans".into()),
    })
}
