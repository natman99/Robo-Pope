# robo-pope

Discord bot: word-count leveling, live leaderboards, customizable level
banners, one-word-story enforcement, moderation.

Rust rewrite (branch `feature/rust-rewrite`) of the original TypeScript bot.
Stack: [serenity](https://crates.io/crates/serenity) + [poise](https://crates.io/crates/poise)
(commands), MongoDB (banner settings), resvg/tiny-skia (level banner rendering
— replaces the old puppeteer screenshot pipeline).

## Build

```bash
cargo build --release
```

## Run

```bash
cp env.example .env   # fill it in
./target/release/robo-pope
```

## Notes on the rewrite

- Env var names are unchanged from the TypeScript version, except
  `PUPPETEEREXECUTABLEPATH` (gone — no browser) and the new optional
  `FONT_FAMILY`.
- MongoDB documents (`level_settings` collection) keep the exact same field
  names, typos included, so existing data carries over.
- Components V2 messages (containers, sections, media galleries) are sent as
  raw JSON: serenity 0.12 cannot build them (see `src/raw_api.rs`, `src/cv2.rs`).
- The background-image modal takes an image **URL** instead of a file upload:
  serenity 0.12 cannot parse modal file-upload responses. The fetch +
  512x125 cover-crop pipeline from the TS version is preserved.
- The bot has no voice features, so songbird is not used.
