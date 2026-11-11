//! Level banner rendering.
//!
//! The TypeScript bot screenshotted an HTML/CSS template with puppeteer.
//! There is no puppeteer in Rust, so the same banner is composed as SVG and
//! rasterized with resvg/tiny-skia (user-approved). Layout constants mirror
//! assets/level_small.css and assets/level_large.css.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use resvg::{tiny_skia, usvg};

use crate::config::Config;
use crate::level::{hex_num_to_str, LevelInfo, LevelSettings};
use crate::Error;

const SMALL_W: u32 = 256;
const SMALL_H: u32 = 22;
const LARGE_W: u32 = 512;
const LARGE_H: u32 = 125;
/// Custom backgrounds add 8px of canvas around the content (TS: +8 w/h).
const BG_PAD: u32 = 8;

#[derive(Clone)]
pub struct Renderer {
    opts: Arc<usvg::Options<'static>>,
    client: reqwest::Client,
}

impl Renderer {
    pub fn new(_config: &Config) -> Result<Self, Error> {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        if db.faces().next().is_none() {
            return Err("No system fonts found for banner rendering".into());
        }
        let opts = usvg::Options {
            fontdb: Arc::new(db),
            ..Default::default()
        };
        Ok(Self {
            opts: Arc::new(opts),
            client: reqwest::Client::new(),
        })
    }

    /// Render the banner for `user` into the cache dir, returning the PNG path.
    pub async fn render(
        &self,
        config: &Config,
        settings: &LevelSettings,
        display_name: &str,
        avatar_url: Option<&str>,
        info: LevelInfo,
    ) -> Result<PathBuf, Error> {
        let avatar = match avatar_url {
            Some(url) => fetch_as_png_data_uri(self.http(), url).await?,
            None => String::new(),
        };
        let background = if settings.has_costome_background && !settings.backgrond_url.is_empty() {
            Some(background_data_uri(self.http(), &settings.backgrond_url).await?)
        } else {
            None
        };

        let svg = build_svg(config, settings, display_name, &avatar, background.as_deref(), info);

        let tree = usvg::Tree::from_str(&svg, &self.opts)
            .map_err(|e| format!("SVG parse failed: {e}"))?;

        let size = tree.size();
        let (w, h) = (size.width() as u32, size.height() as u32);
        let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1))
            .ok_or("failed to allocate banner pixmap")?;
        {
            let mut pm = pixmap.as_mut();
            resvg::render(&tree, usvg::Transform::from_scale(1.0, 1.0), &mut pm);
        }

        std::fs::create_dir_all(&config.cache_path)?;
        let path = config.cache_path.join("level.png");
        pixmap
            .save_png(&path)
            .map_err(|e| format!("failed to save banner PNG: {e}"))?;
        Ok(path)
    }

    fn http(&self) -> &reqwest::Client {
        &self.client
    }
}

/// Fetch an image and re-encode as a base64 PNG data URI (GIF/JPEG/WebP safe).
async fn fetch_as_png_data_uri(client: &reqwest::Client, url: &str) -> Result<String, Error> {
    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    encode_png_data_uri(&bytes)
}

fn encode_png_data_uri(bytes: &[u8]) -> Result<String, Error> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("bad image: {e}"))?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.to_rgba8()
        .write_to(&mut out, image::ImageFormat::Png)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(out.into_inner())
    ))
}

/// Port of the TS sharp pipeline: fetch, cover-resize to 512x125 center, PNG data URI.
pub async fn background_data_uri(client: &reqwest::Client, url: &str) -> Result<String, Error> {
    if url.starts_with("data:") {
        return Ok(url.to_string());
    }
    let fetched = async {
        let bytes = client.get(url).send().await?.error_for_status()?.bytes().await?;
        let img = image::load_from_memory(&bytes)?;
        let (w, h) = (img.width(), img.height());
        let scale = (LARGE_W as f64 / w as f64).max(LARGE_H as f64 / h as f64);
        let (sw, sh) = (
            (w as f64 * scale).ceil() as u32,
            (h as f64 * scale).ceil() as u32,
        );
        let scaled = image::imageops::resize(&img.to_rgba8(), sw, sh, image::imageops::FilterType::Lanczos3);
        let cropped = image::imageops::crop_imm(
            &scaled,
            (sw - LARGE_W) / 2,
            (sh - LARGE_H) / 2,
            LARGE_W,
            LARGE_H,
        );
        let mut out = std::io::Cursor::new(Vec::new());
        cropped.to_image().write_to(&mut out, image::ImageFormat::Png)?;
        Ok::<_, Error>(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(out.into_inner())
        ))
    };
    // Same fallback as the TS version: on any failure, use the raw URL.
    match fetched.await {
        Ok(uri) => Ok(uri),
        Err(e) => {
            eprintln!("background crop failed ({e}); using raw url");
            Ok(url.to_string())
        }
    }
}

/// Rough text width estimate (Noto Sans ~0.58em average per glyph).
fn text_width(text: &str, font_size: f64) -> f64 {
    text.chars().count() as f64 * font_size * 0.58
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        chars.iter().take(max - 1).collect::<String>() + "…"
    }
}

/// Split "#RRGGBBAA" (from hex_num_to_str) into color + opacity for SVG.
fn split_alpha(color: &str) -> (&str, f64) {
    let (rgb, a) = color.split_at(7);
    let alpha = u8::from_str_radix(a, 16).unwrap_or(255) as f64 / 255.0;
    (rgb, alpha)
}

fn build_svg(
    config: &Config,
    s: &LevelSettings,
    display_name: &str,
    avatar: &str,
    background: Option<&str>,
    info: LevelInfo,
) -> String {
    let font = esc(&config.font_family);
    let text_col = hex_num_to_str(s.text_color, s.text_color_trans);
    let (text_rgb, text_a) = split_alpha(&text_col);
    let prim_col = hex_num_to_str(s.primary_color, s.primary_color_trans);
    let (prim_rgb, prim_a) = split_alpha(&prim_col);
    let sec_col = hex_num_to_str(s.secondary_color, s.secondary_color_trans);
    let (sec_rgb, sec_a) = split_alpha(&sec_col);
    let shadow_col = hex_num_to_str(s.shadow_color, s.shadow_strength);
    let (shadow_rgb, shadow_a) = split_alpha(&shadow_col);

    let (base_w, base_h) = if s.is_large { (LARGE_W, LARGE_H) } else { (SMALL_W, SMALL_H) };
    let pad = if s.has_costome_background { BG_PAD } else { 0 };
    let (w, h) = (base_w + pad, base_h + pad);
    let ox = pad as f64 / 2.0;
    let oy = pad as f64 / 2.0;

    let mut defs = String::from(
        "<defs>\
         <filter id=\"sh\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\">\
         <feDropShadow dx=\"0\" dy=\"3\" stdDeviation=\"2\" ",
    );
    defs.push_str(&format!(
        "flood-color=\"{shadow_rgb}\" flood-opacity=\"{shadow_a:.3}\"/></filter>\
         <filter id=\"tsh\" x=\"-50%\" y=\"50%\" width=\"200%\" height=\"200%\">\
         <feDropShadow dx=\"0\" dy=\"3\" stdDeviation=\"1.5\" \
         flood-color=\"{shadow_rgb}\" flood-opacity=\"{shadow_a:.3}\"/></filter>\
         <filter id=\"blur\"><feGaussianBlur stdDeviation=\"5\"/></filter>"
    ));

    let mut body = String::new();
    let bar_clip: String;

    if let Some(bg) = background {
        body.push_str(&format!(
            "<image href=\"{bg}\" x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"xMidYMid slice\"/>"
        ));
    }

    if s.is_large {
        // top row: username (right aligned) | avatar | "Level: N"
        let avatar_d = 62.0f64;
        let name = truncate(display_name, 14);
        let name_x = ox + 215.0;
        let av_x = ox + 225.0;
        let lvl_x = av_x + avatar_d + 10.0;
        let top_cy = oy + 41.0;

        defs.push_str(&format!(
            "<clipPath id=\"av\"><circle cx=\"{}\" cy=\"{}\" r=\"{}\"/></clipPath>",
            av_x + avatar_d / 2.0,
            top_cy,
            avatar_d / 2.0
        ));

        body.push_str(&format!(
            "<text x=\"{name_x}\" y=\"{:.1}\" text-anchor=\"end\" font-family=\"{font}\" font-size=\"25\" font-weight=\"900\" fill=\"{text_rgb}\" fill-opacity=\"{text_a:.3}\" filter=\"url(#tsh)\">{}</text>",
            top_cy + 9.0,
            esc(&name)
        ));
        body.push_str(&format!(
            "<image href=\"{avatar}\" x=\"{av_x:.1}\" y=\"{:.1}\" width=\"{avatar_d}\" height=\"{avatar_d}\" clip-path=\"url(#av)\" filter=\"url(#sh)\"/>",
            top_cy - avatar_d / 2.0
        ));
        body.push_str(&format!(
            "<text x=\"{lvl_x:.1}\" y=\"{:.1}\" font-family=\"{font}\" font-size=\"25\" font-weight=\"900\" fill=\"{text_rgb}\" fill-opacity=\"{text_a:.3}\" filter=\"url(#tsh)\">Level: {}</text>",
            top_cy + 9.0,
            info.level
        ));

        // bar row
        let bar_h = 20.0f64;
        let bar_w = 384.0f64; // 75vw in the CSS
        let min_s = format!("{}", info.min_xp);
        let max_s = format!("{}", info.max_xp);
        let wmin = text_width(&min_s, 20.0) + 12.0;
        let wmax = text_width(&max_s, 20.0) + 12.0;
        let total = wmin + 10.0 + bar_w + 10.0 + wmax;
        let start = ox + (base_w as f64 - total) / 2.0;
        let bar_y = oy + 87.5;
        let text_y = bar_y + bar_h / 2.0 + 7.0;

        body.push_str(&format!(
            "<text x=\"{start:.1}\" y=\"{text_y:.1}\" font-family=\"{font}\" font-size=\"20\" font-weight=\"500\" fill=\"{text_rgb}\" fill-opacity=\"{text_a:.3}\" filter=\"url(#tsh)\">{min_s}<tspan font-size=\"10\">XP</tspan></text>"
        ));
        let bar_x = start + wmin + 10.0;
        bar_clip = format!("<clipPath id=\"barclip\"><rect x=\"{bar_x:.1}\" y=\"{bar_y:.1}\" width=\"{bar_w}\" height=\"{bar_h}\" rx=\"10\"/></clipPath>");
        body.push_str(&format!(
            "<rect x=\"{bar_x:.1}\" y=\"{bar_y:.1}\" width=\"{bar_w}\" height=\"{bar_h}\" rx=\"10\" fill=\"{sec_rgb}\" fill-opacity=\"{sec_a:.3}\" filter=\"url(#sh)\"/>"
        ));
        let fill_w = (bar_w * info.min_xp as f64 / info.max_xp.max(1) as f64).max(0.0);
        body.push_str(&format!(
            "<rect x=\"{bar_x:.1}\" y=\"{bar_y:.1}\" width=\"{fill_w:.1}\" height=\"{bar_h}\" rx=\"10\" fill=\"{prim_rgb}\" fill-opacity=\"{prim_a:.3}\"/>"
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{text_y:.1}\" font-family=\"{font}\" font-size=\"20\" font-weight=\"500\" fill=\"{text_rgb}\" fill-opacity=\"{text_a:.3}\" filter=\"url(#tsh)\">{max_s} <tspan font-size=\"10\">XP</tspan></text>",
            bar_x + bar_w + 10.0
        ));
    } else {
        // small row: avatar | "Lvl. N" | bar | "MAX XP"
        let av_d = 22.0f64;
        let lvl_s = format!("Lvl. {}", info.level);
        let max_s = format!("{}", info.max_xp);
        let w1 = text_width(&lvl_s, 16.0);
        let w3 = text_width(&max_s, 16.0) + 12.0;
        let bar_w = (SMALL_W as f64 - 42.0 - w1 - w3).max(30.0);

        let av_x = ox;
        let av_y = oy;
        defs.push_str(&format!(
            "<clipPath id=\"av\"><circle cx=\"{}\" cy=\"{}\" r=\"{}\"/></clipPath>",
            av_x + av_d / 2.0,
            av_y + av_d / 2.0,
            av_d / 2.0
        ));
        body.push_str(&format!(
            "<image href=\"{avatar}\" x=\"{av_x:.1}\" y=\"{av_y:.1}\" width=\"{av_d}\" height=\"{av_d}\" clip-path=\"url(#av)\" filter=\"url(#sh)\"/>"
        ));
        let lvl_x = av_x + av_d + 5.0;
        body.push_str(&format!(
            "<text x=\"{lvl_x:.1}\" y=\"{:.1}\" font-family=\"{font}\" font-size=\"16\" font-weight=\"500\" fill=\"{text_rgb}\" fill-opacity=\"{text_a:.3}\" filter=\"url(#tsh)\">{lvl_s}</text>",
            av_y + 16.0
        ));
        let bar_x = lvl_x + w1 + 5.0;
        let bar_y = av_y + 6.0;
        bar_clip = format!("<clipPath id=\"barclip\"><rect x=\"{bar_x:.1}\" y=\"{bar_y:.1}\" width=\"{bar_w:.1}\" height=\"10\" rx=\"5\"/></clipPath>");
        body.push_str(&format!(
            "<rect x=\"{bar_x:.1}\" y=\"{bar_y:.1}\" width=\"{bar_w:.1}\" height=\"10\" rx=\"5\" fill=\"{sec_rgb}\" fill-opacity=\"{sec_a:.3}\" filter=\"url(#sh)\"/>"
        ));
        let fill_w = (bar_w * info.min_xp as f64 / info.max_xp.max(1) as f64).max(0.0);
        body.push_str(&format!(
            "<rect x=\"{bar_x:.1}\" y=\"{bar_y:.1}\" width=\"{fill_w:.1}\" height=\"10\" rx=\"5\" fill=\"{prim_rgb}\" fill-opacity=\"{prim_a:.3}\"/>"
        ));
        body.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"{font}\" font-size=\"16\" font-weight=\"500\" fill=\"{text_rgb}\" fill-opacity=\"{text_a:.3}\" filter=\"url(#tsh)\">{max_s} <tspan font-size=\"10\">XP</tspan></text>",
            bar_x + bar_w + 5.0,
            av_y + 16.0
        ));
    }

    // Frost: CSS backdrop-filter blur has no direct SVG equivalent; approximate
    // it by blurring the background image clipped to the bar (no-op without a
    // custom background, same as the browser).
    let frost = if s.frost && background.is_some() {
        format!(
            "<g clip-path=\"url(#barclip)\"><image href=\"{}\" x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"xMidYMid slice\" filter=\"url(#blur)\"/></g>",
            background.unwrap()
        )
    } else {
        String::new()
    };

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">{defs}{bar_clip}</defs>{frost}{body}</svg>"
    )
}

pub fn delete_if_exists(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_settings() -> LevelSettings {
        LevelSettings {
            user_id: "1".into(),
            guild_id: Some("2".into()),
            primary_color: 0x00FF00,
            primary_color_trans: 1.0,
            secondary_color: 0x29292D,
            secondary_color_trans: 1.0,
            text_color: 0xFFFFFF,
            text_color_trans: 1.0,
            shadow_color: 0x000000,
            shadow_strength: 0.5,
            frost: true,
            is_large: false,
            has_costome_background: false,
            backgrond_url: String::new(),
        }
    }

    fn rasterize(svg: &str) -> tiny_skia::Pixmap {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let opts = usvg::Options { fontdb: Arc::new(db), ..Default::default() };
        let tree = usvg::Tree::from_str(svg, &opts).expect("SVG must parse");
        let (w, h) = (tree.size().width() as u32, tree.size().height() as u32);
        let mut pixmap = tiny_skia::Pixmap::new(w, h).unwrap();
        let mut pm = pixmap.as_mut();
        resvg::render(&tree, usvg::Transform::from_scale(1.0, 1.0), &mut pm);
        drop(pm);
        // Something must actually be drawn (non-transparent pixels).
        assert!(
            pixmap.pixels().iter().any(|p| p.alpha() > 0),
            "rendered banner is fully transparent"
        );
        pixmap
    }

    fn config() -> Config {
        Config {
            client_id: 0,
            developer_ids: vec![],
            github_token: String::new(),
            cache_path: PathBuf::from("/tmp/rp-test-cache"),
            cache_channel_id: 0,
            leaderboard_channel_ids: vec![],
            pro_roles: vec![],
            ows_channel_ids: vec![],
            announcement_channel_ids: vec![],
            error_channel_id: 0,
            dev_mode: true,
            font_family: "Noto Sans".into(),
        }
    }

    #[test]
    fn small_banner_renders() {
        let svg = build_svg(
            &config(),
            &sample_settings(),
            "rocketman510",
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
            None,
            LevelInfo { level: 3, min_xp: 245, max_xp: 275, total_max_xp: 275 },
        );
        let pm = rasterize(&svg);
        assert_eq!((pm.width(), pm.height()), (SMALL_W, SMALL_H));
    }

    #[test]
    fn large_banner_with_background_renders() {
        let mut s = sample_settings();
        s.is_large = true;
        s.has_costome_background = true;
        // 1x1 red pixel
        let bg = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        let svg = build_svg(
            &config(),
            &s,
            "jorby_official",
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
            Some(bg),
            LevelInfo { level: 12, min_xp: 800, max_xp: 900, total_max_xp: 900 },
        );
        let pm = rasterize(&svg);
        // Custom background adds the 8px canvas pad (like the TS +8 w/h).
        assert_eq!((pm.width(), pm.height()), (LARGE_W + BG_PAD, LARGE_H + BG_PAD));
    }
}
