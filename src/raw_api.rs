//! Raw Discord REST calls.
//!
//! serenity 0.12 cannot *send* Components V2 (containers, text displays, media
//! galleries, sections) — it silently drops non-ActionRow components. The TS bot
//! is built entirely on Components V2, so those messages go out through this
//! module as hand-built JSON, with basic 429 retry.

use std::time::Duration;

use serde_json::{json, Value};

use crate::Error;

const API: &str = "https://discord.com/api/v10";

/// Message flags we need.
pub const FLAGS_COMPONENTS_V2: u32 = 32;
pub const FLAGS_EPHEMERAL: u32 = 64;

#[derive(Clone)]
pub struct RawApi {
    client: reqwest::Client,
    token: String,
    app_id: u64,
}

impl RawApi {
    pub fn new(token: &str, app_id: u64) -> Self {
        Self {
            client: reqwest::Client::new(),
            token: token.to_string(),
            app_id,
        }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.client
    }

    async fn call(&self, method: reqwest::Method, path: &str, body: &Value) -> Result<Value, Error> {
        for _attempt in 0..4u32 {
            let resp = self
                .client
                .request(method.clone(), format!("{API}{path}"))
                .bearer_auth(&self.token)
                .json(body)
                .send()
                .await?;

            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let retry: f64 = resp
                    .json::<Value>()
                    .await
                    .ok()
                    .and_then(|v| v.get("retry_after").and_then(|r| r.as_f64()))
                    .unwrap_or(1.0);
                tokio::time::sleep(Duration::from_secs_f64(retry)).await;
                continue;
            }

            let status = resp.status();
            let text = resp.text().await?;
            if !status.is_success() {
                return Err(format!("Discord API {method} {path} -> {status}: {text}").into());
            }
            if text.is_empty() {
                return Ok(Value::Null);
            }
            return Ok(serde_json::from_str(&text)?);
        }
        Err(format!("Discord API {method} {path}: rate limited, giving up").into())
    }

    /// POST /channels/{channel}/messages with Components V2 content.
    pub async fn send_cv2(
        &self,
        channel_id: u64,
        components: &[Value],
        flags: u32,
    ) -> Result<Value, Error> {
        self.call(
            reqwest::Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &json!({ "components": components, "flags": flags }),
        )
        .await
    }

    /// PATCH /channels/{channel}/messages/{message} with Components V2 content.
    pub async fn edit_channel_cv2(
        &self,
        channel_id: u64,
        message_id: u64,
        components: &[Value],
        flags: u32,
    ) -> Result<Value, Error> {
        self.call(
            reqwest::Method::PATCH,
            &format!("/channels/{channel_id}/messages/{message_id}"),
            &json!({ "components": components, "flags": flags }),
        )
        .await
    }

    /// PATCH /webhooks/{app}/{interaction_token}/messages/@original — edit a
    /// deferred interaction response with Components V2 content.
    pub async fn edit_response_cv2(
        &self,
        interaction_token: &str,
        components: &[Value],
        flags: u32,
    ) -> Result<Value, Error> {
        self.call(
            reqwest::Method::PATCH,
            &format!("/webhooks/{}/{}/messages/@original", self.app_id, interaction_token),
            &json!({ "components": components, "flags": flags }),
        )
        .await
    }

    /// POST /webhooks/{app}/{interaction_token} — initial response (type 4)
    /// carrying Components V2 content.
    pub async fn respond_cv2(
        &self,
        interaction_token: &str,
        components: &[Value],
        flags: u32,
    ) -> Result<Value, Error> {
        self.call(
            reqwest::Method::POST,
            &format!("/webhooks/{}/{}", self.app_id, interaction_token),
            &json!({ "type": 4, "data": { "components": components, "flags": flags } }),
        )
        .await
    }

    /// Report an error into the error channel as a red Components V2 container,
    /// mirroring the TS `sendErr`.
    pub async fn report_error(&self, error_channel_id: u64, message: &str) {
        let container = crate::cv2::container(
            0xff0000,
            vec![crate::cv2::text_display(&format!(
                "Bot Had an Error:\n```{message}\n```"
            ))],
        );
        if let Err(e) = self
            .send_cv2(error_channel_id, &[container], FLAGS_COMPONENTS_V2)
            .await
        {
            eprintln!("Failed to report error to error channel: {e}");
        }
    }
}
