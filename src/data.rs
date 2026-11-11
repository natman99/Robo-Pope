//! Shared bot state (poise user data). Cheap to clone: everything lives in one Arc.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use poise::serenity_prelude as serenity;

use crate::banner::Renderer;
use crate::config::Config;
use crate::raw_api::RawApi;
use crate::Error;

/// guild_id -> (user_id -> words said)
pub type WordMap = HashMap<u64, HashMap<u64, u64>>;
/// guild_id -> (user_id -> next level-up word milestone)
pub type MilestoneMap = HashMap<u64, HashMap<u64, u64>>;

pub struct Inner {
    pub config: Config,
    pub db: mongodb::Database,
    pub raw: RawApi,
    pub renderer: Renderer,
    pub words: RwLock<WordMap>,
    pub milestones: RwLock<MilestoneMap>,
    /// Previous leaderboard order per guild, for the up/down arrows. serenity
    /// drops Components V2 components when receiving messages, so the old
    /// order is tracked in memory instead of re-parsing the old message.
    pub prev_top: RwLock<HashMap<u64, Vec<u64>>>,
    pub stop_spam: AtomicBool,
    /// True while startup history is still being tallied (TS `is_counting_messages`).
    pub counting: AtomicBool,
}

#[derive(Clone)]
pub struct Data {
    pub inner: Arc<Inner>,
}

pub async fn setup(_ctx: &serenity::Context) -> Result<Data, Error> {
    let config = crate::config::from_env()?;
    clear_cache(&config.cache_path);

    let db_password = std::env::var("DB_PASSWORD")?;
    let db_domain = std::env::var("DB_DOMAIN")?;
    let uri = format!("mongodb://admin:{db_password}@{db_domain}");
    let db = mongodb::Client::with_uri_str(&uri).await?.database("Robo-Pope-DB");
    println!("Connected to MongoDB!");

    let token = std::env::var("DISCORD_TOKEN")?;
    let raw = RawApi::new(&token, config.client_id);
    let renderer = Renderer::new(&config)?;

    Ok(Data {
        inner: Arc::new(Inner {
            config,
            db,
            raw,
            renderer,
            words: RwLock::new(HashMap::new()),
            milestones: RwLock::new(HashMap::new()),
            prev_top: RwLock::new(HashMap::new()),
            stop_spam: AtomicBool::new(false),
            counting: AtomicBool::new(true),
        }),
    })
}

/// Port of TS `clear_cache`: wipe the contents of the cache dir at startup.
pub fn clear_cache(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        match result {
            Ok(_) => println!("Deleted: {}", name.to_string_lossy()),
            Err(e) => eprintln!("Failed to delete {}: {e}", name.to_string_lossy()),
        }
    }
}
