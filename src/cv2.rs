//! Components V2 JSON builders (the shapes serenity 0.12 cannot build).
//!
//! Component types: Section=9, TextDisplay=11, Container=17, MediaGallery=22,
//! MediaGalleryItem=23, ActionRow=1, Button=2, StringSelect=3.

use serde_json::{json, Value};

pub fn text_display(content: impl Into<String>) -> Value {
    json!({ "type": 11, "content": content.into() })
}

pub fn container(accent_color: u32, components: Vec<Value>) -> Value {
    json!({ "type": 17, "accent_color": accent_color, "components": components })
}

pub fn section(text_displays: Vec<Value>, button: Value) -> Value {
    json!({ "type": 9, "components": text_displays, "accessory": button })
}

pub fn action_row(components: Vec<Value>) -> Value {
    json!({ "type": 1, "components": components })
}

pub fn button(custom_id: &str, label: &str, style: u8, disabled: bool) -> Value {
    json!({ "type": 2, "style": style, "label": label, "custom_id": custom_id, "disabled": disabled })
}

pub fn media_gallery(url: &str, description: &str) -> Value {
    json!({
        "type": 22,
        "components": [{ "type": 23, "media_item_url": url, "description": description }],
    })
}

pub fn size_select(placeholder: &str, disabled: bool) -> Value {
    json!({
        "type": 3,
        "custom_id": "level_settings_size",
        "placeholder": placeholder,
        "options": [
            { "label": "Large", "value": "large" },
            { "label": "Default Small", "value": "small" },
        ],
        "disabled": disabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_type_codes() {
        assert_eq!(text_display("hi")["type"], 11);
        assert_eq!(container(0x242429, vec![text_display("hi")])["type"], 17);
        let sec = section(
            vec![text_display("hi")],
            button("update", "Update the Bot", 3, false),
        );
        assert_eq!(sec["type"], 9);
        assert_eq!(sec["accessory"]["type"], 2);
        assert_eq!(sec["accessory"]["custom_id"], "update");
        let row = action_row(vec![size_select("Change Size (Small)", false)]);
        assert_eq!(row["type"], 1);
        assert_eq!(row["components"][0]["type"], 3);
        let gal = media_gallery("https://x/y.png", "Image 1");
        assert_eq!(gal["type"], 22);
        assert_eq!(gal["components"][0]["type"], 23);
        assert_eq!(gal["components"][0]["media_item_url"], "https://x/y.png");
    }
}
