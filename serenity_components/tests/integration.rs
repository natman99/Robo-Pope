//! End-to-end framework exercise: registration, rendering, persistence,
//! frame pagination and interaction routing. No network access: routing is
//! driven by Discord-shaped interaction payloads deserialized into Serenity
//! models.

use std::sync::atomic::{AtomicUsize, Ordering};

use serenity::http::Http;
use serenity::model::application::{ButtonStyle, InputTextStyle, Interaction};
use serenity_components::prelude::*;

static CLICKS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
struct Data {
    level: u32,
}

fn app() -> App<()> {
    let token: serenity::secrets::Token = "111.222.333".parse().expect("token");
    App::memory(std::sync::Arc::new(Http::new(token)))
}

fn message_json(custom_id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "123456789012345678",
        "type": 3,
        "application_id": "100000000000000001",
        "channel_id": "200000000000000001",
        "channel": { "id": "200000000000000001", "type": 0, "name": "general" },
        "guild_id": "300000000000000001",
        "token": "tok",
        "version": 1,
        "locale": "en-US",
        "guild_locale": "en-US",
        "permissions": "0",
        "app_permissions": "0",
        "attachment_size_limit": 0,
        "entitlements": [],
        "data": {
            "component_type": 2,
            "custom_id": custom_id,
            "values": []
        },
        "message": {
            "id": "999999999999999999",
            "channel_id": "200000000000000001",
            "author": { "id": "400000000000000001", "username": "u", "discriminator": "0", "global_name": null, "public_flags": 0, "flags": 0 },
            "content": "",
            "timestamp": "2026-01-01T00:00:00.000+00:00",
            "edited_timestamp": null,
            "tts": false,
            "mention_everyone": false,
            "mentions": [],
            "mention_roles": [],
            "mention_channels": [],
            "attachments": [],
            "embeds": [],
            "reactions": [],
            "pinned": false,
            "type": 0,
            "flags": 0
        }
    })
}

#[tokio::test]
async fn register_render_and_route_button() {
    let app = app();
    CLICKS.store(0, Ordering::SeqCst);

    let page = Page::<Data>::new("test.page")
        .add_text("static text")
        .add_text(dynamic(|_ctx, data: Data| {
            Box::pin(async move { Ok(format!("level {}", data.level)) })
        }))
        .add_button_row((
            Some(Button::new(
                "Click",
                "click",
                ButtonStyle::Primary,
                exec(|_ctx, _data| {
                    Box::pin(async move {
                        CLICKS.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                }),
            )),
            Slot::Empty,
            Slot::Empty,
            Slot::Empty,
            Slot::Empty,
        ))
        .register(&app)
        .expect("register");

    // Duplicate registration is rejected.
    let dup = Page::<Data>::new("test.page").register(&app);
    assert!(matches!(dup, Err(Error::DuplicatePageId { .. })));

    let rendered = page.render(&app, Data { level: 7 }).await.expect("render");
    assert!(
        rendered
            .flags
            .contains(serenity::model::channel::MessageFlags::IS_COMPONENTS_V2)
    );
    assert_eq!(rendered.components.len(), 3); // text, text, action row

    // The execution reference was persisted under the deterministic hash.
    let custom_id = serenity_components::custom_id_hash(
        "test.page",
        "click",
        &serde_json::to_vec(&serenity_components::PageData {
            data: Data { level: 7 },
            frame_page: 0,
        })
        .unwrap(),
    );
    let record = app
        .db
        .store
        .lookup(&custom_id)
        .await
        .expect("lookup")
        .expect("record");
    assert_eq!(record.def_id, "test.page");
    assert_eq!(record.exec_id, "click");

    // Route a Discord-shaped component interaction to the callback.
    let interaction: Interaction = serde_json::from_str(&message_json(&custom_id).to_string())
        .expect("deserialize interaction");
    app.handle_interaction(&interaction).await.expect("route");
    assert_eq!(CLICKS.load(Ordering::SeqCst), 1);

    // Unknown custom IDs are reported, not panicked on.
    let unknown: Interaction =
        serde_json::from_str(&message_json("deadbeef").to_string()).expect("deserialize unknown");
    assert!(matches!(
        app.handle_interaction(&unknown).await,
        Err(Error::UnknownCustomId { .. })
    ));
}

#[tokio::test]
async fn frame_pagination_produces_multiple_pages_with_nav() {
    let app = app();

    let page = Page::<Data>::new("test.paged")
        .add_text("header")
        .add_frame(frame(|_ctx, _data: Data| {
            Box::pin(async move {
                let mut items = Vec::new();
                for i in 0..40 {
                    items.push(Element::text(format!("item {i}")));
                }
                Ok(items)
            })
        }))
        .paginate()
        .register(&app)
        .expect("register");

    let pages = page
        .render_all(&app, Data { level: 1 })
        .await
        .expect("render_all");
    assert!(
        pages.len() >= 2,
        "expected pagination, got {} page(s)",
        pages.len()
    );
    for p in &pages {
        assert!(!p.components.is_empty());
    }
}

#[tokio::test]
async fn modal_render_open_and_submit() {
    let app = app();

    let modal = Modal::<Data>::new("test.modal")
        .set_title("Form")
        .add_text_input("name", "Name", InputTextStyle::Short)
        .add_checkbox("agree", "Agree")
        .on_submit(submit(|_ctx, data: Data, submission: ModalSubmission| {
            Box::pin(async move {
                let name = submission.text("name")?;
                let agree = submission.checkbox("agree")?;
                assert_eq!(name, "Ada");
                assert!(agree);
                assert_eq!(data.level, 3);
                Ok(())
            })
        }))
        .register(&app)
        .expect("register modal");

    let rendered = modal
        .render(&app, Data { level: 3 })
        .await
        .expect("render modal");
    assert_eq!(rendered.components.len(), 2);

    // The submission reference is persisted under the modal's hash.
    let data_json = serde_json::to_vec(&Data { level: 3 }).unwrap();
    let custom_id = serenity_components::custom_id_hash("test.modal", "submit", &data_json);
    let record = app
        .db
        .store
        .lookup(&custom_id)
        .await
        .expect("lookup")
        .expect("record");
    assert_eq!(record.def_id, "test.modal");

    // Route a Discord-shaped modal submission.
    let payload = serde_json::json!({
        "id": "123456789012345670",
        "type": 5,
        "application_id": "100000000000000001",
        "channel_id": "200000000000000001",
        "channel": { "id": "200000000000000001", "type": 0, "name": "general" },
        "token": "tok",
        "version": 1,
        "locale": "en-US",
        "permissions": "0",
        "app_permissions": "0",
        "attachment_size_limit": 0,
        "entitlements": [],
        "data": {
            "custom_id": custom_id,
            "components": [
                {
                    "type": 18,
                    "label": "Name",
                    "component": { "type": 4, "custom_id": "name", "value": "Ada" }
                },
                {
                    "type": 18,
                    "label": "Agree",
                    "component": { "type": 23, "custom_id": "agree", "value": true }
                }
            ]
        }
    });
    let interaction: Interaction =
        serde_json::from_str(&payload.to_string()).expect("deserialize modal interaction");
    app.handle_interaction(&interaction)
        .await
        .expect("route modal submit");
}

#[tokio::test]
async fn string_select_routes_selection_values() {
    static SEEN: AtomicUsize = AtomicUsize::new(0);
    let app = app();

    let page = Page::<Data>::new("test.select")
        .add_selection_menu(StringSelect::new(
            "Pick",
            "pick",
            vec![SelectOption::new("A", "a"), SelectOption::new("C", "c")],
            select_exec(|_ctx, _data: Data, sel: Selection<Vec<String>>| {
                Box::pin(async move {
                    let got: Vec<String> = sel.values;
                    assert_eq!(got, vec!["a".to_string(), "c".to_string()]);
                    SEEN.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
        ))
        .register(&app)
        .expect("register select");

    page.render(&app, Data { level: 0 }).await.expect("render");
    let cid = serenity_components::custom_id_hash(
        "test.select",
        "pick",
        &serde_json::to_vec(&serenity_components::PageData {
            data: Data { level: 0 },
            frame_page: 0,
        })
        .unwrap(),
    );

    let mut payload = message_json(&cid);
    payload["data"] = serde_json::json!({
        "component_type": 3,
        "custom_id": cid,
        "values": ["a", "c"]
    });
    let interaction: Interaction =
        serde_json::from_str(&payload.to_string()).expect("deserialize select interaction");
    app.handle_interaction(&interaction)
        .await
        .expect("route select");
    assert_eq!(SEEN.load(Ordering::SeqCst), 1);
}
