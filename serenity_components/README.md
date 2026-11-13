# serenity_components

A declarative framework for Discord **Components V2** on top of
[serenity](https://github.com/serenity-rs/serenity)'s `next` branch (0.12.5
has no Components V2 support).

Build Discord interfaces as composable Rust values: a `Page` describes a
message (components, static-or-dynamic values, execution callbacks), an
`App` owns registration, persistence and interaction routing. Serenity's
builders, custom IDs, serialization and routing become implementation
details.

```rust
let app = App::memory(http);

let page = Page::<Data>::new("my.page")
    .add_text(dynamic(|_ctx, data: Data| Box::pin(async move {
        Ok(format!("Level: {}", data.level))
    })))
    .add_button_row((
        Some(Button::new("Click", "click", ButtonStyle::Primary,
            exec(|ctx, _data| Box::pin(async move { ctx.reply("hi").await })))),
        Slot::Empty, Slot::Empty, Slot::Empty, Slot::Empty,
    ))
    .register(&app)?;

let message = page.render(&app, Data { level: 7 }).await?;
message.send(&app.http, channel_id).await?;
```

Key pieces:

- **`Value<V, T, E>`** — every displayable property accepts a static value or
  an async render-time callback, with no separate constructors.
- **`Frame`** — a dynamic region resolved at render time, with automatic
  pagination and framework-provided navigation controls.
- **Modals** — their own blueprint type; submissions route back with typed
  accessors (`text`, `checkbox`, `string_select`, …).
- **Routing** — custom IDs are BLAKE3 hashes of `(page_id, exec_id, data)`;
  execution references persist through a pluggable `Store` trait
  (in-memory default), so interactions resolve to their callback and data
  across bot restarts.

See `SPEC.md` for the full design document and `tests/integration.rs` for
end-to-end examples (registration, rendering, pagination, routing) that run
without network access.

## Status

Library crate, not yet wired into the bot. Note: this crate depends on
serenity's `next` branch via git, while the bot uses serenity 0.12.5 from
crates.io — integrating them means moving the whole bot onto `next`.
