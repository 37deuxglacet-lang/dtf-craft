// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! The start screen's background: an illustration from Pixabay (pixabay.com/api), a new one at
//! each start of DTF Craft.
//!
//! Pixabay's rules are kept: the search is cached for 24 hours (in the preferences, with the
//! image last shown so the next start shows another), the image is downloaded rather than
//! hot-linked, and its source is shown ("Illustration: <author> on Pixabay", a link to its page).
//! Offline, without an API key or with the illustration turned off, the start screen keeps its
//! plain background. The download runs on a worker thread; the start screen never waits for it.
//!
//! The API key comes from the build (`PIXABAY_KEY`, a GitHub Actions secret) or from the
//! preferences (`dtfCraft.homeImage.key`).

use std::sync::{Arc, Mutex};

use egui::{Color32, Rect, pos2};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::PhotocraftApp;

/// The preferences entry: `{"enabled", "query", "key", "cache"}`.
pub const PREF: &str = "dtfCraft.homeImage";
const API: &str = "https://pixabay.com/api/";
/// Searches are kept this long (Pixabay asks for 24 hours).
const CACHE_SECS: u64 = 24 * 3600;
pub const DEFAULT_QUERY: &str = "pop art";
const PER_PAGE: u32 = 60;
/// The key built in by the release workflow.
const BUILT_IN_KEY: Option<&str> = option_env!("PIXABAY_KEY");
/// How long the illustration takes to fade in, in seconds.
const FADE: f64 = 0.6;

/// One illustration of a search: its image (at most 1280 px) and where it comes from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub url: String,
    pub user: String,
    pub page: String,
}

/// A search kept for 24 hours, and the illustration last shown.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cache {
    pub fetched: u64,
    pub query: String,
    pub hits: Vec<Hit>,
    pub last: Option<usize>,
}

impl Cache {
    pub fn fresh(&self, now: u64, query: &str) -> bool {
        !self.hits.is_empty() && self.query == query && now.saturating_sub(self.fetched) < CACHE_SECS
    }
}

/// The settings kept in the preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub query: String,
    pub key: String,
    pub cache: Cache,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { enabled: true, query: DEFAULT_QUERY.into(), key: String::new(), cache: Cache::default() }
    }
}

/// What the worker brings back: the image (RGBA8), the illustration and the search it chose from.
pub struct Loaded {
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
    pub hit: Hit,
    pub cache: Cache,
}

type Slot = Arc<Mutex<Option<Result<Loaded, String>>>>;

/// The start screen's illustration: loading, loaded or none.
#[derive(Default)]
pub struct HomeImage {
    started: bool,
    pending: Option<Slot>,
    pub texture: Option<egui::TextureHandle>,
    pub credit: Option<Hit>,
    shown_at: f64,
    /// Why there is none (offline, no key…), for the tests and the log.
    pub error: Option<String>,
}

pub fn settings(app: &PhotocraftApp) -> Settings {
    app.session.prefs().dialogs.get(PREF).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
}

fn save(app: &mut PhotocraftApp, s: &Settings) {
    let v = serde_json::to_value(s).unwrap_or(Value::Null);
    app.session.prefs.edit(|p| p.dialogs.insert(PREF.into(), v));
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The search's address: Editor's Choice illustrations first (`editors`), else all of them.
pub fn search_url(key: &str, query: &str, editors: bool) -> String {
    let q: String = query
        .trim()
        .bytes()
        .map(|b| match b {
            b' ' => "+".to_string(),
            b if b.is_ascii_alphanumeric() => (b as char).to_string(),
            b => format!("%{b:02X}"),
        })
        .collect();
    format!("{API}?key={key}&q={q}&image_type=illustration&orientation=horizontal&safesearch=true&editors_choice={editors}&per_page={PER_PAGE}")
}

/// The illustrations of a Pixabay answer.
pub fn parse_hits(body: &[u8]) -> Result<Vec<Hit>, String> {
    let v: Value = serde_json::from_slice(body).map_err(|e| format!("Pixabay: {e}"))?;
    let hits = v.get("hits").and_then(Value::as_array).ok_or("Pixabay: no hits")?;
    Ok(hits
        .iter()
        .filter_map(|h| {
            let s = |k: &str| h.get(k).and_then(Value::as_str).map(str::to_string);
            Some(Hit { url: s("largeImageURL").or_else(|| s("webformatURL"))?, user: s("user").unwrap_or_default(), page: s("pageURL").unwrap_or_default() })
        })
        .collect())
}

/// Another illustration than the last one shown, picked from `seed`.
pub fn pick(n: usize, last: Option<usize>, seed: u64) -> Option<usize> {
    match n {
        0 => None,
        1 => Some(0),
        _ => {
            // A step of 1..n from the last one: never the same twice in a row.
            let step = 1 + (seed % (n as u64 - 1)) as usize;
            Some(last.map_or(seed as usize % n, |l| (l + step) % n))
        }
    }
}

fn seed() -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    // splitmix64: the low bits of the clock alone are poorly spread.
    let mut z = t.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The worker: the search (cached for 24 hours), one illustration and its image.
fn load(get: crate::HttpGetFn, key: &str, query: &str, cache: Cache, seed: u64) -> Result<Loaded, String> {
    let now = now_secs();
    let mut cache = if cache.fresh(now, query) {
        cache
    } else {
        let mut hits = parse_hits(&get(&search_url(key, query, true))?)?;
        // Too few Editor's Choice illustrations for this search: all of them.
        if hits.len() < 5 {
            hits = parse_hits(&get(&search_url(key, query, false))?)?;
        }
        Cache { fetched: now, query: query.into(), hits, last: cache.last }
    };
    let i = pick(cache.hits.len(), cache.last, seed).ok_or("Pixabay: no illustration for this search")?;
    let hit = cache.hits[i].clone();
    let bytes = get(&hit.url)?;
    let img = photocraft_codecs::decode(&bytes).map_err(|e| e.to_string())?;
    cache.last = Some(i);
    Ok(Loaded { size: [img.width() as usize, img.height() as usize], rgba: img.to_rgba8(), hit, cache })
}

/// Starts the download once per run, when the illustration is on and a key and the network are
/// there.
pub fn start(app: &mut PhotocraftApp) {
    if app.home_image.started || !app.dtf_craft {
        return;
    }
    app.home_image.started = true;
    let s = settings(app);
    let key = if s.key.trim().is_empty() { BUILT_IN_KEY.unwrap_or("").to_string() } else { s.key.trim().to_string() };
    let Some(get) = app.services.http_get else { return };
    if !s.enabled {
        return;
    }
    if key.is_empty() {
        app.home_image.error = Some("no Pixabay key".into());
        return;
    }
    let query = if s.query.trim().is_empty() { DEFAULT_QUERY.to_string() } else { s.query.trim().to_string() };
    let slot: Slot = Arc::new(Mutex::new(None));
    app.home_image.pending = Some(slot.clone());
    let cache = s.cache.clone();
    let seed = seed();
    std::thread::spawn(move || {
        let r = load(get, &key, &query, cache, seed);
        if let Ok(mut s) = slot.lock() {
            *s = Some(r);
        }
    });
}

/// Shows `rgba` (`size`) as if it had just been downloaded (screenshots).
#[cfg(test)]
pub(crate) fn show_image(app: &mut PhotocraftApp, size: [usize; 2], rgba: Vec<u8>, hit: Hit) {
    app.home_image.started = true;
    app.home_image.pending = Some(Arc::new(Mutex::new(Some(Ok(Loaded { size, rgba, hit, cache: Cache::default() })))));
}

/// Takes the worker's result: the texture, the credit, and the search kept for the next starts.
pub fn poll(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(slot) = app.home_image.pending.clone() else { return };
    let done = slot.lock().ok().and_then(|mut s| s.take());
    match done {
        None => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
        Some(Err(e)) => {
            app.home_image.pending = None;
            app.home_image.error = Some(e);
        }
        Some(Ok(l)) => {
            app.home_image.pending = None;
            let image = egui::ColorImage::from_rgba_unmultiplied(l.size, &l.rgba);
            app.home_image.texture = Some(ctx.load_texture("dtf-home-image", image, egui::TextureOptions::LINEAR));
            app.home_image.credit = Some(l.hit);
            app.home_image.shown_at = ctx.input(|i| i.time);
            let mut s = settings(app);
            s.cache = l.cache;
            save(app, &s);
        }
    }
}

/// Turns the illustration off (it goes at once) or back on (one is loaded at once).
pub fn set_enabled(app: &mut PhotocraftApp, on: bool) {
    let mut s = settings(app);
    s.enabled = on;
    save(app, &s);
    if on {
        app.home_image.started = false;
        start(app);
    } else {
        app.home_image.texture = None;
        app.home_image.credit = None;
        app.home_image.pending = None;
    }
}

/// The illustration over `area`, cropped to fill it, under a veil so the start screen stays
/// legible. False when there is none to draw.
pub fn paint(app: &PhotocraftApp, ui: &egui::Ui, area: Rect) -> bool {
    let Some(tex) = &app.home_image.texture else { return false };
    let t = crate::theme::Tokens::get(ui.ctx());
    let size = tex.size_vec2();
    let k = (area.width() / size.x).max(area.height() / size.y);
    let shown = size * k;
    // The part of the image that fills the area, centred.
    let (u, v) = ((1.0 - area.width() / shown.x) / 2.0, (1.0 - area.height() / shown.y) / 2.0);
    let uv = Rect::from_min_max(pos2(u, v), pos2(1.0 - u, 1.0 - v));
    let fade = ((ui.input(|i| i.time) - app.home_image.shown_at) / FADE).clamp(0.0, 1.0) as f32;
    if fade < 1.0 {
        ui.ctx().request_repaint();
    }
    let painter = ui.painter_at(area);
    painter.image(tex.id(), area, uv, Color32::from_white_alpha((255.0 * fade) as u8));
    let veil = t.canvas;
    painter.rect_filled(area, 0.0, Color32::from_rgba_unmultiplied(veil.r(), veil.g(), veil.b(), (165.0 * fade + 90.0 * (1.0 - fade)) as u8));
    true
}

/// "Illustration: <author> on Pixabay" (its page) and "Hide the image", at the bottom right;
/// "Show an illustration" once hidden (when the build or the preferences have a key).
pub fn credit(app: &mut PhotocraftApp, ui: &mut egui::Ui, area: Rect) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let r = Rect::from_min_max(pos2(area.right() - 460.0, area.bottom() - 24.0), pos2(area.right() - 10.0, area.bottom() - 4.0));
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::right_to_left(egui::Align::Center)));
    row.spacing_mut().item_spacing.x = 10.0;
    let small = |s: &str| egui::RichText::new(s).size(11.0);
    let Some(hit) = app.home_image.credit.clone() else {
        let s = settings(app);
        let has_key = !s.key.trim().is_empty() || BUILT_IN_KEY.is_some_and(|k| !k.is_empty());
        if app.dtf_craft && !s.enabled && has_key && app.services.http_get.is_some() && row.link(small(tl!("Show an illustration")).color(t.text_faint)).clicked() {
            set_enabled(app, true);
        }
        return;
    };
    if row.link(small(tl!("Hide the image")).color(t.text_faint)).clicked() {
        set_enabled(app, false);
    }
    let who = crate::i18n::fmt(tl!("Illustration: {user} on Pixabay"), &[("user", &hit.user)]);
    if row.link(small(&who).color(t.text_dim)).on_hover_text(&hit.page).clicked() && !hit.page.is_empty() {
        crate::links::open(app, ui.ctx(), &hit.page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{"total": 2, "hits": [
        {"largeImageURL": "https://cdn.pixabay.com/a_1280.png", "webformatURL": "https://cdn.pixabay.com/a_640.png", "user": "Alice", "pageURL": "https://pixabay.com/a/"},
        {"webformatURL": "https://cdn.pixabay.com/b_640.png", "user": "Bob", "pageURL": "https://pixabay.com/b/"},
        {"user": "no image"}
    ]}"#;

    #[test]
    fn the_answer_gives_the_illustrations_and_their_sources() {
        let hits = parse_hits(ANSWER.as_bytes()).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], Hit { url: "https://cdn.pixabay.com/a_1280.png".into(), user: "Alice".into(), page: "https://pixabay.com/a/".into() });
        assert_eq!(hits[1].url, "https://cdn.pixabay.com/b_640.png", "the medium size when there is no large one");
        assert!(parse_hits(b"{}").is_err());
        let url = search_url("K", "abstract shapes", true);
        assert!(url.starts_with("https://pixabay.com/api/?key=K&q=abstract+shapes&image_type=illustration&") && url.contains("safesearch=true"), "{url}");
        assert!(search_url("K", "été", false).contains("q=%C3%A9t%C3%A9&") && search_url("K", "x", false).contains("editors_choice=false"));
    }

    #[test]
    fn a_new_illustration_each_time_and_a_search_kept_24_hours() {
        for seed in 0..50 {
            let i = pick(5, Some(2), seed).unwrap();
            assert!(i < 5 && i != 2, "{seed}: {i}");
        }
        assert_eq!(pick(1, Some(0), 7), Some(0));
        assert_eq!(pick(0, None, 7), None);
        let c = Cache { fetched: 1_000, query: "abstract".into(), hits: vec![Hit::default()], last: None };
        assert!(c.fresh(1_000 + CACHE_SECS - 1, "abstract"));
        assert!(!c.fresh(1_000 + CACHE_SECS, "abstract"));
        assert!(!c.fresh(1_100, "flowers"), "another search");
    }

    fn png() -> Vec<u8> {
        let img = photocraft_codecs::Image::from_u8(4, 2, photocraft_codecs::ChannelLayout::Rgba, vec![200; 32]).unwrap();
        photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &photocraft_codecs::EncodeOptions::default()).unwrap()
    }

    fn fake_get(url: &str) -> Result<Vec<u8>, String> {
        if url.starts_with(API) { Ok(ANSWER.as_bytes().to_vec()) } else if url.contains("cdn.pixabay.com") { Ok(png()) } else { Err("offline".into()) }
    }

    fn offline(_: &str) -> Result<Vec<u8>, String> {
        Err("offline".into())
    }

    fn wait(app: &mut PhotocraftApp, ctx: &egui::Context) {
        for _ in 0..200 {
            poll(app, ctx);
            if app.home_image.pending.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn the_start_screen_loads_an_illustration_and_keeps_the_search() {
        let ctx = egui::Context::default();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services { http_get: Some(fake_get), ..Default::default() });
        app.dtf_craft = true;
        app.session.prefs.edit(|p| p.dialogs.insert(PREF.into(), serde_json::json!({"key": "K"})));
        start(&mut app);
        wait(&mut app, &ctx);
        assert!(app.home_image.texture.is_some(), "{:?}", app.home_image.error);
        let first = app.home_image.credit.clone().unwrap();
        let s = settings(&app);
        assert_eq!((s.cache.hits.len(), s.cache.query.as_str()), (2, DEFAULT_QUERY));
        // The next start: the search from the cache, the other illustration.
        let mut next = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services { http_get: Some(fake_get), ..Default::default() });
        next.dtf_craft = true;
        let v = app.session.prefs().dialogs.get(PREF).cloned().unwrap();
        next.session.prefs.edit(|p| p.dialogs.insert(PREF.into(), v));
        start(&mut next);
        wait(&mut next, &ctx);
        assert_ne!(next.home_image.credit.as_ref().unwrap().url, first.url);
        // Hidden: gone at once; shown again: another one loads.
        set_enabled(&mut next, false);
        assert!(next.home_image.texture.is_none() && !settings(&next).enabled);
        set_enabled(&mut next, true);
        wait(&mut next, &ctx);
        assert!(next.home_image.texture.is_some());
        // Offline or without a key: no illustration, no error shown.
        let mut off = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services { http_get: Some(offline), ..Default::default() });
        off.dtf_craft = true;
        off.session.prefs.edit(|p| p.dialogs.insert(PREF.into(), serde_json::json!({"key": "K"})));
        start(&mut off);
        wait(&mut off, &ctx);
        assert!(off.home_image.texture.is_none() && off.home_image.error.is_some());
    }
}
