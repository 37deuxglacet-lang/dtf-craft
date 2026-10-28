// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Gang Sheet…: a full-window, light, uncluttered builder to lay visuals out on DTF film.
//!
//! * Header: title, Cancel, Export.
//! * Toolbar: film size, undo/redo, grid, the spacing between visuals, the selection's tools
//!   (turn, mirror, stretch to the film, centre, push to a side, delete, group, ungroup) and Auto
//!   Build.
//! * Left rail and panel: **Home** (drop zone, the visuals as thumbnails with their copies on the
//!   sheet, and the selected visual's size, copies, own spacing, fill and duplicate) and
//!   **Settings** (lines shown, snapping, warnings, sheet background, margins, film).
//! * Canvas: the sheet with rulers in cm; an overlay card with the line switches, the legend and
//!   the hovered or selected visual's position, size, resolution and warnings.
//! * Right panel: the active gang sheets (name, visuals, copies to print, duplicate, delete), New
//!   Sheet, Auto Build and Start Over.
//! * Auto Build view: every visual with its size, proportions lock, resolution and copies; Apply
//!   lays them all out again, opening new sheets when one is full.
//!
//! Visuals keep a margin from the film edges and a spacing between them (each visual may ask for
//! its own); a visual dropped closer than that to another goes back where it was.
//!
//! Selection: a click picks a visual, Shift+click (or Ctrl+click) adds or removes one, a rectangle
//! drawn from an empty part of the film picks every visual it touches, Ctrl+A picks them all. The
//! selection moves, turns (as one block, about its centre), mirrors, centres, pushes, copies,
//! duplicates and deletes together. Ctrl+G groups it, Ctrl+Shift+G ungroups: a group is picked
//! with one click, keeps its inner spacing and stays one block in Auto Build.
//!
//! Number fields are typed: dragging the mouse across them changes nothing. Export writes
//! every sheet as a 300 ppi PNG through `dtf.gangSheet` and opens it as a new document.
//!
//! The window is always light, whatever the app's theme: [`light_scope`] swaps the tokens and the
//! egui style in for its own drawing and puts the app's back afterwards.
//!
//! Control channel: `ui.menu.invoke {"id":"dtf.gangSheet","params":{"ui":{…}}}` with `addFiles`,
//! `addDocument`, `set` (film and display settings), `source` (`index`, `widthCm`, `heightCm`,
//! `lock`, `quantity`, `marginMm`/`null`), `removeSource`, `autoBuild`, `select` (index, list or
//! `null`), `selectAll`, `group`, `ungroup`, `move` (the selection's box corner), `rotate`
//! (quarters), `mirror` (`"h"`/`"v"`), `stretch`, `center`, `align`, `duplicate`, `delete`,
//! `fill`, `copy`, `paste`, `undo`, `redo`, `newSheet`, `sheet` (active), `duplicateSheet`,
//! `alignSelection` (`left`/`hcenter`/`right`/`top`/`vcenter`/`bottom`), `distribute` (`h`/`v`),
//! `deleteSheet`, `sheetCopies`, `renameSheet`, `startOver`, `tab` (`home`/`settings`), `autoView`,
//! `export` (path), `cancel`.

use std::sync::Mutex;

use egui::{Align2, Color32, CornerRadius, FontId, Mesh, Pos2, Rect as ERect, Sense, Shape, Stroke, StrokeKind, TextureHandle, Vec2, pos2, vec2};
use photocraft_algo::gang_sheet::{self as gs, DpiLevel, Item, Sheet, Side, UNITS_PER_CM};
use photocraft_doc::DocId;
use photocraft_engine::gang_sheet_cmds::{self as cmd, Orientation, Placed, Visual};
use photocraft_geom::Rect;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::file_dialog::{FileDialogAnswer, FileDialogRequest};
use crate::theme::{ThemeKind, Tokens};
use crate::widgets;

const HEADER_H: f32 = 52.0;
const TOOLBAR_H: f32 = 40.0;
const RAIL_W: f32 = 70.0;
const LEFT_W: f32 = 300.0;
const RIGHT_W: f32 = 290.0;
const RULER: f32 = 20.0;
/// Width of the overlay card on the sheet view.
const CARD_W: f32 = 250.0;
/// Longest side of a visual's preview texture.
const THUMB: u32 = 640;
/// Arrow-key nudge: 1 mm, 1 cm with Shift (layout units).
const NUDGE: i64 = 10;
const NUDGE_SHIFT: i64 = 100;
/// The film is at most this long (cm).
const MAX_LENGTH_CM: f32 = 100.0;
/// Most sheets and undo steps kept.
const MAX_SHEETS: usize = 50;
const MAX_UNDO: usize = 100;
/// Film widths offered in the toolbar (cm).
const FILM_PRESETS: [f32; 5] = [30.0, 40.0, 56.0, 57.0, 60.0];

// ---------------------------------------------------------------- light look

/// The window's palette: white panels, hairline borders, navy controls, a blue call to action.
pub(crate) fn light_tokens() -> Tokens {
    let mut t = Tokens::for_kind(ThemeKind::StudioLight);
    let navy = Color32::from_rgb(17, 24, 39);
    t.chrome = Color32::WHITE;
    t.dock = Color32::WHITE;
    t.card = Color32::WHITE;
    t.canvas = Color32::from_rgb(243, 244, 246);
    t.card_border = Color32::from_rgb(229, 231, 235);
    t.separator = Color32::from_rgb(229, 231, 235);
    t.field = Color32::WHITE;
    t.field_border = Color32::from_rgb(209, 213, 219);
    t.hover = Color32::from_rgb(243, 244, 246);
    t.pressed = Color32::from_rgb(229, 231, 235);
    t.text = navy;
    t.text_dim = Color32::from_rgb(55, 65, 81);
    t.text_faint = Color32::from_rgb(107, 114, 128);
    t.icon = navy;
    t.accent = navy;
    t.accent_soft = Color32::from_rgba_unmultiplied(17, 24, 39, 24);
    t.accent_border = Color32::from_rgba_unmultiplied(17, 24, 39, 120);
    t.accent_text = navy;
    t.primary_bg = navy;
    t.primary_text = Color32::WHITE;
    t.danger = Color32::from_rgb(220, 38, 38);
    t.warning = Color32::from_rgb(217, 119, 6);
    t.radius_sm = 6.0;
    t.radius = 8.0;
    t.radius_lg = 10.0;
    t
}

/// The blue of Auto Build and of overlapping visuals.
const BLUE: Color32 = Color32::from_rgb(29, 155, 240);

/// Runs `f` with the window's light tokens and style, then gives the app its own back.
fn light_scope<R>(ctx: &egui::Context, f: impl FnOnce() -> R) -> R {
    let saved = Tokens::get(ctx);
    let (light, dark) = (ctx.style_of(egui::Theme::Light), ctx.style_of(egui::Theme::Dark));
    crate::theme::apply_tokens(ctx, light_tokens());
    let r = f();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("photocraft-theme"), saved));
    ctx.set_style_of(egui::Theme::Light, light);
    ctx.set_style_of(egui::Theme::Dark, dark);
    r
}

/// Print-resolution ratings (green, yellow, red, black), the same in every theme.
fn level_color(level: DpiLevel) -> Color32 {
    match level {
        DpiLevel::Optimal => Color32::from_rgb(34, 197, 94),
        DpiLevel::Good => Color32::from_rgb(234, 179, 8),
        DpiLevel::Low => Color32::from_rgb(239, 68, 68),
        DpiLevel::Poor => Color32::from_rgb(17, 24, 39),
    }
}

fn level_label(level: DpiLevel) -> &'static str {
    match level {
        DpiLevel::Optimal => tl!("Optimal"),
        DpiLevel::Good => tl!("Good"),
        DpiLevel::Low => tl!("Poor"),
        DpiLevel::Poor => tl!("Very Poor"),
    }
}

// ---------------------------------------------------------------- state

/// Where a visual comes from: the command reloads it from there when the sheets are exported.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Origin {
    File(String),
    Document(DocId),
}

/// One visual of the list.
#[derive(Clone)]
pub struct Source {
    origin: Origin,
    pub name: String,
    /// Its visible pixels (cropped).
    pub px: (u32, u32),
    /// Print size (layout units), unturned.
    pub w: i64,
    pub h: i64,
    /// Width and height change together.
    pub lock: bool,
    /// Its own spacing (layout units), or the sheet's.
    pub margin: Option<i64>,
    /// Copies Auto Build lays out.
    pub quantity: usize,
    /// A background left on it (opaque corners), and its share of semi-transparent pixels.
    pub background: bool,
    pub semi: f64,
    preview: Option<egui::ColorImage>,
    tex: Option<TextureHandle>,
}

impl Source {
    /// Its print resolution: the lower of the two axes'.
    pub fn dpi(&self) -> f64 {
        gs::effective_dpi(self.px.0, self.w).min(gs::effective_dpi(self.px.1, self.h))
    }

    fn ratio(&self) -> f64 {
        if self.px.1 == 0 { 1.0 } else { f64::from(self.px.0) / f64::from(self.px.1) }
    }
}

/// One gang sheet: its name, its visuals and how many copies of it to print.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetDoc {
    pub name: String,
    pub placed: Vec<Placed>,
    pub copies: usize,
}

impl SheetDoc {
    fn new(n: usize) -> Self {
        SheetDoc { name: format!("{} {n}", tl!("Gang Sheet")), placed: Vec::new(), copies: 1 }
    }
}

/// What ⌘Z steps through.
#[derive(Clone)]
struct Snapshot {
    sources: Vec<Source>,
    sheets: Vec<SheetDoc>,
    active: usize,
}

/// The sheet's background in the window.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Backdrop {
    Checker,
    Color(Color32),
}

/// Film and display settings, remembered from one opening to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Settings {
    width_cm: f32,
    max_length_cm: f32,
    edge_on: bool,
    edge_cm: f32,
    gap_on: bool,
    gap_cm: f32,
    rotate: bool,
    show_res: bool,
    show_overlap: bool,
    snap: bool,
    warn_background: bool,
    warn_semi: bool,
    grid: bool,
    backdrop: Backdrop,
    custom: [u8; 3],
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            width_cm: cmd::DEFAULT_FILM_WIDTH_CM as f32,
            max_length_cm: cmd::DEFAULT_MAX_LENGTH_CM as f32,
            edge_on: true,
            edge_cm: (cmd::DEFAULT_EDGE_MM / 10.0) as f32,
            gap_on: true,
            gap_cm: (cmd::DEFAULT_SPACING_MM / 10.0) as f32,
            rotate: true,
            show_res: true,
            show_overlap: true,
            snap: false,
            warn_background: true,
            warn_semi: true,
            grid: false,
            backdrop: Backdrop::Checker,
            custom: [35, 28, 28],
        }
    }
}

static LAST_SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Home,
    Settings,
}

/// The selection being dragged: where it was grabbed (units from its box's corner), its box and
/// where each of its visuals was before.
#[derive(Clone, Debug)]
struct Drag {
    grab: (i64, i64),
    bbox: Item,
    from: Vec<(usize, i64, i64)>,
}

/// A rectangle drawn from an empty part of the film: its corners (units) and the selection it adds
/// to (Shift held).
#[derive(Clone, Debug)]
struct Marquee {
    from: (i64, i64),
    to: (i64, i64),
    base: Vec<usize>,
}

/// Copied visuals: each one's source, orientation, place in the copied box and group (0: none),
/// and the box's size.
#[derive(Clone, Debug, PartialEq)]
struct Clip {
    members: Vec<(usize, Orientation, i64, i64, u32)>,
    w: i64,
    h: i64,
}

/// The box around `items` (empty when there are none).
fn bounds(items: &[Item]) -> Item {
    let Some(first) = items.first() else { return Item::default() };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x + first.w, first.y + first.h);
    for r in items {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x + r.w);
        y1 = y1.max(r.y + r.h);
    }
    Item::new(x0, y0, x1 - x0, y1 - y0)
}

/// `r`, inside a `bw × bh` box, after the box turns 90° clockwise.
fn turn_cw(r: Item, bh: i64) -> Item {
    Item::new(bh - (r.y + r.h), r.x, r.h, r.w)
}

fn cm_units(v: f32) -> i64 {
    (f64::from(v) * UNITS_PER_CM as f64).round() as i64
}

fn units_cm(v: i64) -> f32 {
    (v as f64 / UNITS_PER_CM as f64) as f32
}

pub struct GangSheetDialog {
    settings: Settings,
    pub sources: Vec<Source>,
    pub sheets: Vec<SheetDoc>,
    pub active: usize,
    /// The selected visuals of the active sheet (indices, sorted).
    pub selected: Vec<usize>,
    hovered: Option<usize>,
    drag: Option<Drag>,
    marquee: Option<Marquee>,
    tab: Tab,
    auto_view: bool,
    /// 1 = the whole sheet fits; more zooms in.
    zoom: f32,
    scroll: Vec2,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The last edit's key: edits of the same field in a row make one undo step.
    last_edit: Option<String>,
    clipboard: Option<Clip>,
    renaming: Option<(usize, String)>,
    confirm_export: bool,
    pub status: String,
}

fn preview_image(v: &Visual) -> egui::ColorImage {
    let (w, h) = (v.width.max(1), v.height.max(1));
    let k = (f64::from(THUMB) / f64::from(w.max(h))).min(1.0);
    let src = Rect::new(0, 0, w as i32, h as i32);
    let (surf, r) = if k < 1.0 {
        let s = photocraft_algo::resample::resize_surface(&v.pixels, k, k, photocraft_algo::resample::Resample::BicubicAutomatic);
        (s, photocraft_algo::resample::scaled_rect(src, k, k))
    } else {
        (v.pixels.clone(), src)
    };
    let (pw, ph) = (r.width().max(1) as usize, r.height().max(1) as usize);
    let mut px = vec![[0u8; 4]; pw * ph];
    surf.read_rgba8_into(Rect::new(r.x0, r.y0, r.x0 + pw as i32, r.y0 + ph as i32), &mut px);
    egui::ColorImage::new([pw, ph], px.iter().map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])).collect())
}

impl GangSheetDialog {
    fn new() -> Self {
        let settings = LAST_SETTINGS.lock().ok().and_then(|f| *f).unwrap_or_default();
        GangSheetDialog {
            settings,
            sources: Vec::new(),
            sheets: vec![SheetDoc::new(1)],
            active: 0,
            selected: Vec::new(),
            hovered: None,
            drag: None,
            marquee: None,
            tab: Tab::Home,
            auto_view: false,
            zoom: 1.0,
            scroll: Vec2::ZERO,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            clipboard: None,
            renaming: None,
            confirm_export: false,
            status: String::new(),
        }
    }

    // ---------------------------------------------------------------- geometry

    fn edge(&self) -> i64 {
        if self.settings.edge_on { cm_units(self.settings.edge_cm) } else { 0 }
    }

    fn gap(&self) -> i64 {
        if self.settings.gap_on { cm_units(self.settings.gap_cm) } else { 0 }
    }

    pub fn sheet(&self) -> Sheet {
        Sheet::new(cm_units(self.settings.width_cm), cm_units(self.settings.max_length_cm), self.edge(), self.gap())
    }

    /// A source's spacing: its own, or the sheet's.
    fn gap_of(&self, source: usize) -> i64 {
        self.sources.get(source).and_then(|s| s.margin).unwrap_or_else(|| self.gap())
    }

    /// The largest spacing any visual asks for (Auto Build packs at it).
    fn widest_gap(&self) -> i64 {
        self.sources.iter().filter_map(|s| s.margin).fold(self.gap(), i64::max)
    }

    fn placed(&self) -> &[Placed] {
        self.sheets.get(self.active).map_or(&[], |s| s.placed.as_slice())
    }

    fn placed_mut(&mut self) -> Option<&mut Vec<Placed>> {
        self.sheets.get_mut(self.active).map(|s| &mut s.placed)
    }

    fn size_of(&self, source: usize) -> (i64, i64) {
        self.sources.get(source).map_or((0, 0), |s| (s.w, s.h))
    }

    pub fn item(&self, i: usize) -> Item {
        self.placed().get(i).map_or_else(Item::default, |p| {
            let (w, h) = self.size_of(p.source);
            cmd::footprint(p, w, h)
        })
    }

    pub fn items(&self) -> Vec<Item> {
        (0..self.placed().len()).map(|i| self.item(i)).collect()
    }

    fn gaps(&self) -> Vec<i64> {
        self.placed().iter().map(|p| self.gap_of(p.source)).collect()
    }

    pub fn conflicts(&self) -> Vec<bool> {
        self.sheet().conflicts(&self.items(), &self.gaps())
    }

    /// Visuals too close or off the film, on every sheet.
    pub fn all_conflicts(&self) -> usize {
        let mut d = self.shadow();
        (0..self.sheets.len())
            .map(|i| {
                d.active = i;
                d.conflicts().iter().filter(|c| **c).count()
            })
            .sum()
    }

    /// A copy of the layout (no textures), to measure other sheets with.
    fn shadow(&self) -> GangSheetDialog {
        let mut d = GangSheetDialog::new();
        d.settings = self.settings;
        d.sources = self.sources.iter().map(|s| Source { preview: None, tex: None, ..s.clone() }).collect();
        d.sheets = self.sheets.clone();
        d
    }

    fn copies_on_sheet(&self, source: usize) -> usize {
        self.placed().iter().filter(|p| p.source == source).count()
    }

    // ---------------------------------------------------------------- history

    /// Records the state before an edit. `key`: edits with the same key in a row (typing in one
    /// field, nudging) make one step.
    fn checkpoint(&mut self, key: Option<String>) {
        if key.is_some() && key == self.last_edit {
            return;
        }
        self.last_edit = key;
        let snap = Snapshot { sources: self.sources.clone(), sheets: self.sheets.clone(), active: self.active };
        self.undo.push(snap);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(&mut self, s: Snapshot) {
        self.sources = s.sources;
        self.sheets = s.sheets;
        self.active = s.active.min(self.sheets.len().saturating_sub(1));
        self.selected.clear();
        self.drag = None;
        self.marquee = None;
        self.last_edit = None;
    }

    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else { return false };
        self.redo.push(Snapshot { sources: self.sources.clone(), sheets: self.sheets.clone(), active: self.active });
        self.restore(s);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else { return false };
        self.undo.push(Snapshot { sources: self.sources.clone(), sheets: self.sheets.clone(), active: self.active });
        self.restore(s);
        true
    }

    // ---------------------------------------------------------------- placing

    /// Places `count` copies of `source` on the active sheet where there is room; returns how
    /// many found a place.
    fn add_copies(&mut self, source: usize, count: usize) -> usize {
        if count == 0 || source >= self.sources.len() {
            return 0;
        }
        let sheet = self.sheet().with_gap(self.widest_gap());
        let size = self.size_of(source);
        let spots = sheet.pack(&vec![size; count], &self.items(), self.settings.rotate);
        let mut n = 0;
        for sp in spots.into_iter().flatten() {
            let orientation = Orientation { quarter: u8::from(sp.rotated), ..Orientation::default() };
            if let Some(list) = self.placed_mut() {
                list.push(Placed { source, x: sp.x, y: sp.y, orientation, group: 0 });
                n += 1;
            }
        }
        n
    }

    /// Sets the copies of `source` on the active sheet: adds where there is room, or takes the
    /// last placed off.
    fn set_copies_on_sheet(&mut self, source: usize, want: usize) {
        let have = self.copies_on_sheet(source);
        if want > have {
            let placed = self.add_copies(source, want - have);
            if placed < want - have {
                self.status = crate::i18n::fmt(tl!("{n} not placed"), &[("n", &(want - have - placed).to_string())]);
            }
        } else {
            for _ in want..have {
                if let Some(list) = self.placed_mut()
                    && let Some(i) = list.iter().rposition(|p| p.source == source)
                {
                    list.remove(i);
                }
            }
            self.tidy_groups();
            self.selected.clear();
        }
    }

    /// Fills the room left on the active sheet with copies of `source`.
    pub fn fill(&mut self, source: usize) -> usize {
        let (w, h) = self.size_of(source);
        let (iw, il) = self.sheet().inner();
        let g = self.widest_gap();
        if w <= 0 || h <= 0 || iw <= 0 || il <= 0 {
            return 0;
        }
        let most = ((iw + g) / (w.min(h) + g)).max(1) * ((il + g) / (w.min(h) + g)).max(1);
        self.add_copies(source, (most as usize).min(2_000))
    }

    /// Auto Build: every visual's copies laid out again from the first sheet on, a new sheet
    /// opened whenever one is full. Each group stays as it is, one block (its visuals count among
    /// their source's copies). Returns the copies that fit nowhere.
    pub fn auto_build(&mut self) -> usize {
        let names: Vec<(String, usize)> = self.sheets.iter().map(|s| (s.name.clone(), s.copies)).collect();
        // The groups of every sheet.
        let mut blocks: Vec<Clip> = Vec::new();
        for k in 0..self.sheets.len() {
            let mut d = self.shadow();
            d.active = k;
            let mut ids: Vec<u32> = d.placed().iter().map(|p| p.group).filter(|g| *g != 0).collect();
            ids.sort_unstable();
            ids.dedup();
            for g in ids {
                let list: Vec<usize> = (0..d.placed().len()).filter(|i| d.placed().get(*i).is_some_and(|p| p.group == g)).collect();
                blocks.extend(d.copy_block(&list));
            }
        }
        let mut want: Vec<usize> = self.sources.iter().map(|s| s.quantity).collect();
        for m in blocks.iter().flat_map(|b| &b.members) {
            if let Some(n) = want.get_mut(m.0) {
                *n = n.saturating_sub(1);
            }
        }
        self.sheets.clear();
        let mut queue: Vec<usize> = Vec::new();
        for (i, n) in want.iter().enumerate() {
            queue.extend(std::iter::repeat_n(i, *n));
        }
        let sheet = self.sheet().with_gap(self.widest_gap());
        loop {
            let n = self.sheets.len();
            let mut doc = SheetDoc::new(n + 1);
            if let Some((name, copies)) = names.get(n) {
                doc.name = name.clone();
                doc.copies = *copies;
            }
            // Groups first, as they stand, then the single visuals around them.
            let block_spots = sheet.pack(&blocks.iter().map(|b| (b.w, b.h)).collect::<Vec<_>>(), &[], false);
            let mut left_blocks = Vec::new();
            let mut occupied = Vec::new();
            for (b, spot) in blocks.iter().zip(block_spots) {
                match spot {
                    Some(sp) => {
                        let g = doc.placed.iter().map(|p| p.group).max().unwrap_or(0) + 1;
                        for (source, orientation, rx, ry, _) in &b.members {
                            doc.placed.push(Placed { source: *source, x: sp.x + rx, y: sp.y + ry, orientation: *orientation, group: g });
                        }
                        occupied.push(Item::new(sp.x, sp.y, b.w, b.h));
                    }
                    None => left_blocks.push(b.clone()),
                }
            }
            let sizes: Vec<(i64, i64)> = queue.iter().map(|s| self.size_of(*s)).collect();
            let spots = sheet.pack(&sizes, &occupied, self.settings.rotate);
            let mut left = Vec::new();
            for (s, spot) in queue.iter().zip(spots) {
                match spot {
                    Some(sp) => doc.placed.push(Placed {
                        source: *s,
                        x: sp.x,
                        y: sp.y,
                        orientation: Orientation { quarter: u8::from(sp.rotated), ..Orientation::default() },
                        group: 0,
                    }),
                    None => left.push(*s),
                }
            }
            let progress = !doc.placed.is_empty();
            if progress || self.sheets.is_empty() {
                self.sheets.push(doc);
            }
            queue = left;
            blocks = left_blocks;
            if (queue.is_empty() && blocks.is_empty()) || !progress || self.sheets.len() >= MAX_SHEETS {
                break;
            }
        }
        self.active = 0;
        self.selected.clear();
        queue.len() + blocks.iter().map(|b| b.members.len()).sum::<usize>()
    }

    /// Every visual kept on the film (moved, not resized) after a size or film change.
    fn clamp_all(&mut self) {
        let sheet = self.sheet();
        for k in 0..self.sheets.len() {
            let sizes: Vec<(i64, i64)> = self.sheets.get(k).map_or_else(Vec::new, |s| s.placed.iter().map(|p| self.size_of(p.source)).collect());
            if let Some(doc) = self.sheets.get_mut(k) {
                for (p, (w, h)) in doc.placed.iter_mut().zip(sizes) {
                    let r = sheet.clamp(cmd::footprint(p, w, h));
                    (p.x, p.y) = (r.x, r.y);
                }
            }
        }
    }

    /// Sets a source's size (layout units, unturned); with the lock on, `w` or `h` (whichever is
    /// `Some`) drives the other. Never larger than the film.
    fn set_size(&mut self, source: usize, w: Option<i64>, h: Option<i64>) {
        let (iw, il) = self.sheet().inner();
        let longest = iw.max(il).max(10);
        let Some(s) = self.sources.get_mut(source) else { return };
        let ratio = s.ratio();
        match (w, h, s.lock) {
            (Some(w), _, true) => {
                s.w = w.clamp(10, longest);
                s.h = ((s.w as f64 / ratio).round() as i64).max(1);
            }
            (None, Some(h), true) => {
                s.h = h.clamp(10, longest);
                s.w = ((s.h as f64 * ratio).round() as i64).max(1);
            }
            (w, h, false) => {
                s.w = w.unwrap_or(s.w).clamp(10, longest);
                s.h = h.unwrap_or(s.h).clamp(10, longest);
            }
            _ => {}
        }
        self.clamp_all();
    }

    pub(crate) fn add_visual(&mut self, origin: Origin, v: &Visual) {
        self.checkpoint(None);
        let width = v.natural_width(&self.sheet());
        let (w, h) = gs::size_for_width(v.width, v.height, width);
        self.sources.push(Source {
            origin,
            name: v.name.clone(),
            px: (v.width, v.height),
            w,
            h,
            lock: true,
            margin: None,
            quantity: 1,
            background: v.background,
            semi: v.semi_transparent,
            preview: Some(preview_image(v)),
            tex: None,
        });
        let s = self.sources.len() - 1;
        if self.add_copies(s, 1) == 0 {
            self.status = crate::i18n::fmt(tl!("{n} not placed"), &[("n", "1")]);
        } else {
            self.selected = vec![self.placed().len() - 1];
        }
    }

    fn remove_source(&mut self, source: usize) {
        if source >= self.sources.len() {
            return;
        }
        self.checkpoint(None);
        for doc in &mut self.sheets {
            doc.placed.retain(|p| p.source != source);
            for p in &mut doc.placed {
                if p.source > source {
                    p.source -= 1;
                }
            }
        }
        self.sources.remove(source);
        self.tidy_groups();
        self.selected.clear();
        self.drag = None;
        // The clipboard loses the visual too; the others keep their sources.
        if let Some(clip) = self.clipboard.as_mut() {
            clip.members.retain(|m| m.0 != source);
            for m in &mut clip.members {
                if m.0 > source {
                    m.0 -= 1;
                }
            }
            if clip.members.is_empty() {
                self.clipboard = None;
            }
        }
    }

    // ---------------------------------------------------------------- selection and groups

    /// The selected visuals that are on the active sheet, sorted.
    pub fn sel(&self) -> Vec<usize> {
        let n = self.placed().len();
        let mut v: Vec<usize> = self.selected.iter().copied().filter(|i| *i < n).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The one visual selected on its own (not several, not a group).
    pub fn single(&self) -> Option<usize> {
        match self.sel().as_slice() {
            [i] => Some(*i),
            _ => None,
        }
    }

    /// Visual `i` and the visuals grouped with it.
    fn members(&self, i: usize) -> Vec<usize> {
        let list = self.placed();
        match list.get(i).map(|p| p.group) {
            None => Vec::new(),
            Some(0) => vec![i],
            Some(g) => (0..list.len()).filter(|j| list.get(*j).is_some_and(|p| p.group == g)).collect(),
        }
    }

    /// `list` with every group it touches made whole, sorted.
    fn whole(&self, list: &[usize]) -> Vec<usize> {
        let mut v: Vec<usize> = list.iter().flat_map(|i| self.members(*i)).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Selects visual `i` and its group (`None`: nothing).
    pub fn select_one(&mut self, i: Option<usize>) {
        self.selected = i.map_or_else(Vec::new, |i| self.members(i));
    }

    /// Adds visual `i` and its group to the selection, or takes them out when already in it.
    fn toggle(&mut self, i: usize) {
        let group = self.members(i);
        let mut sel = self.sel();
        if sel.contains(&i) {
            sel.retain(|j| !group.contains(j));
        } else {
            sel.extend(group);
        }
        self.selected = self.whole(&sel);
    }

    pub fn select_all(&mut self) {
        self.selected = (0..self.placed().len()).collect();
    }

    /// Whether the selection is exactly one group.
    fn is_one_group(&self, list: &[usize]) -> bool {
        let g = list.first().and_then(|i| self.placed().get(*i)).map_or(0, |p| p.group);
        g != 0 && self.members(list.first().copied().unwrap_or(0)) == list
    }

    /// Whether any selected visual is in a group.
    fn any_grouped(&self, list: &[usize]) -> bool {
        list.iter().any(|i| self.placed().get(*i).is_some_and(|p| p.group != 0))
    }

    /// A group number no visual of the active sheet uses.
    fn new_group_id(&self) -> u32 {
        self.placed().iter().map(|p| p.group).max().unwrap_or(0) + 1
    }

    /// Groups the selection (several visuals) into one; false when there is nothing to group.
    pub fn group(&mut self) -> bool {
        let list = self.sel();
        if list.len() < 2 || self.is_one_group(&list) {
            return false;
        }
        let g = self.new_group_id();
        if let Some(placed) = self.placed_mut() {
            for i in &list {
                if let Some(p) = placed.get_mut(*i) {
                    p.group = g;
                }
            }
        }
        true
    }

    /// Ungroups the selected visuals (they stay selected); false when none was grouped.
    pub fn ungroup(&mut self) -> bool {
        let list = self.sel();
        if !self.any_grouped(&list) {
            return false;
        }
        if let Some(placed) = self.placed_mut() {
            for i in &list {
                if let Some(p) = placed.get_mut(*i) {
                    p.group = 0;
                }
            }
        }
        true
    }

    /// A group left with a single visual (the others deleted) is no group any more.
    fn tidy_groups(&mut self) {
        for doc in &mut self.sheets {
            let ids: Vec<u32> = doc.placed.iter().map(|p| p.group).collect();
            for p in &mut doc.placed {
                if p.group != 0 && ids.iter().filter(|g| **g == p.group).count() < 2 {
                    p.group = 0;
                }
            }
        }
    }

    // ---------------------------------------------------------------- moving blocks

    /// The box around the visuals `list`.
    fn bbox(&self, list: &[usize]) -> Item {
        bounds(&list.iter().map(|i| self.item(*i)).collect::<Vec<_>>())
    }

    /// The visuals not in `list`, with their spacings.
    fn others(&self, list: &[usize]) -> (Vec<Item>, Vec<i64>) {
        let gaps = self.gaps();
        self.items().into_iter().zip(gaps).enumerate().filter(|(i, _)| !list.contains(i)).map(|(_, x)| x).unzip()
    }

    /// The largest spacing the visuals of `list` ask for.
    fn block_gap(&self, list: &[usize]) -> i64 {
        list.iter().filter_map(|i| self.placed().get(*i)).map(|p| self.gap_of(p.source)).fold(self.gap(), i64::max)
    }

    /// Whether the visuals of `list`, standing at `rects`, come closer to one not in `list` than
    /// their spacing allows.
    fn block_collides(&self, list: &[usize], rects: &[Item]) -> bool {
        let sheet = self.sheet();
        let (items, gaps) = self.others(list);
        list.iter().zip(rects).any(|(i, r)| {
            let gap = self.placed().get(*i).map_or(self.gap(), |p| self.gap_of(p.source));
            sheet.collides(&items, &gaps, usize::MAX, r, gap)
        })
    }

    /// Moves the visuals of `list` by (`dx`, `dy`), whatever is there.
    fn shift(&mut self, list: &[usize], dx: i64, dy: i64) {
        if let Some(placed) = self.placed_mut() {
            for i in list {
                if let Some(p) = placed.get_mut(*i) {
                    p.x += dx;
                    p.y += dy;
                }
            }
        }
    }

    /// Moves the visuals of `list` together so their box's corner lands at (`x`, `y`), kept on the
    /// film; refused (false) when one would come closer to another visual than their spacing.
    pub fn move_block(&mut self, list: &[usize], x: i64, y: i64) -> bool {
        if list.is_empty() {
            return false;
        }
        let b = self.bbox(list);
        let nb = self.sheet().clamp(Item { x, y, ..b });
        let (dx, dy) = (nb.x - b.x, nb.y - b.y);
        let rects: Vec<Item> = list
            .iter()
            .map(|i| {
                let r = self.item(*i);
                Item { x: r.x + dx, y: r.y + dy, ..r }
            })
            .collect();
        if self.block_collides(list, &rects) {
            return false;
        }
        self.shift(list, dx, dy);
        true
    }

    /// Turns the visuals of `list` by `quarters` × 90° (negative: anticlockwise) as one block about
    /// its centre, in place when there is room, else wherever there is.
    pub fn rotate_block(&mut self, list: &[usize], quarters: i32) -> bool {
        if list.is_empty() {
            return false;
        }
        let q = quarters.rem_euclid(4);
        let b = self.bbox(list);
        let (mut bw, mut bh) = (b.w, b.h);
        let mut rel: Vec<Item> = list
            .iter()
            .map(|i| {
                let r = self.item(*i);
                Item { x: r.x - b.x, y: r.y - b.y, ..r }
            })
            .collect();
        for _ in 0..q {
            rel = rel.into_iter().map(|r| turn_cw(r, bh)).collect();
            (bw, bh) = (bh, bw);
        }
        let sheet = self.sheet();
        if !sheet.fits(bw, bh) {
            return false;
        }
        let mut nb = sheet.clamp(Item::new(b.x + (b.w - bw) / 2, b.y + (b.h - bh) / 2, bw, bh));
        let at = |nb: Item| -> Vec<Item> { rel.iter().map(|r| Item { x: nb.x + r.x, y: nb.y + r.y, ..*r }).collect() };
        if self.block_collides(list, &at(nb)) {
            let (others, _) = self.others(list);
            match sheet.with_gap(self.widest_gap()).pack(&[(bw, bh)], &others, false).first().copied().flatten() {
                Some(sp) => nb = Item { x: sp.x, y: sp.y, ..nb },
                None => return false,
            }
        }
        let rects = at(nb);
        if let Some(placed) = self.placed_mut() {
            for (i, r) in list.iter().zip(rects) {
                if let Some(p) = placed.get_mut(*i) {
                    *p = Placed { x: r.x, y: r.y, orientation: p.orientation.rotated(quarters), ..*p };
                }
            }
        }
        true
    }

    /// Mirrors the visuals of `list` as one block, left-right (`horizontal`) or top-bottom.
    pub fn mirror_block(&mut self, list: &[usize], horizontal: bool) {
        let b = self.bbox(list);
        let rects: Vec<Item> = list.iter().map(|i| self.item(*i)).collect();
        if let Some(placed) = self.placed_mut() {
            for (i, r) in list.iter().zip(rects) {
                if let Some(p) = placed.get_mut(*i) {
                    if horizontal {
                        p.x = b.x + b.w - (r.x - b.x) - r.w;
                    } else {
                        p.y = b.y + b.h - (r.y - b.y) - r.h;
                    }
                    p.orientation = p.orientation.mirrored(horizontal);
                }
            }
        }
    }

    /// Stretches visual `i`'s source to the film's inner width (`horizontal`) or length, as the
    /// visual stands on the sheet (proportions kept when locked).
    pub fn stretch(&mut self, i: usize, horizontal: bool) {
        let Some(p) = self.placed().get(i).copied() else { return };
        let (iw, il) = self.sheet().inner();
        let target = if horizontal { iw } else { il };
        // The source's own axis that lies along the sheet's.
        let along_width = horizontal != p.orientation.turned();
        if along_width {
            self.set_size(p.source, Some(target), None)
        } else {
            self.set_size(p.source, None, Some(target))
        }
        // Too long for the film once stretched with its proportions: shrink back to fit.
        let (w, h) = self.size_of(p.source);
        let (fw, fh) = if p.orientation.turned() { (h, w) } else { (w, h) };
        if fw > iw || fh > il {
            let k = (iw as f64 / fw as f64).min(il as f64 / fh as f64);
            self.set_size(p.source, Some((w as f64 * k).floor() as i64), Some((h as f64 * k).floor() as i64));
        }
        if let Some(q) = self.placed_mut().and_then(|l| l.get_mut(i)) {
            if horizontal {
                q.x = 0;
            } else {
                q.y = 0;
            }
        }
        self.clamp_all();
    }

    /// Centres the visuals of `list` as one block across the film (`h`), along it (`v`) or both.
    pub fn center_block(&mut self, list: &[usize], h: bool, v: bool) -> bool {
        let r = self.bbox(list);
        let sheet = self.sheet();
        let used = sheet.used_length(&self.items());
        let x = if h { (sheet.width - r.w) / 2 } else { r.x };
        let y = if v { (used.max(r.y + r.h + sheet.edge) - r.h) / 2 } else { r.y };
        self.move_block(list, x, y)
    }

    /// Pushes the visuals of `list` as one block towards `side`.
    pub fn align_block(&mut self, list: &[usize], side: Side) {
        if list.is_empty() {
            return;
        }
        let b = self.bbox(list);
        let (mut items, mut gaps) = self.others(list);
        items.push(b);
        gaps.push(self.block_gap(list));
        let r = self.sheet().slide(&items, &gaps, items.len() - 1, side);
        self.shift(list, r.x - b.x, r.y - b.y);
    }

    /// The selection as the blocks that move apart: each group whole, each other visual alone.
    fn units(&self, list: &[usize]) -> Vec<Vec<usize>> {
        let mut out: Vec<Vec<usize>> = Vec::new();
        for i in list {
            if out.iter().any(|u| u.contains(i)) {
                continue;
            }
            let members: Vec<usize> = self.members(*i).into_iter().filter(|j| list.contains(j)).collect();
            out.push(if members.is_empty() { vec![*i] } else { members });
        }
        out
    }

    /// Moves each block of `units` by its own (dx, dy), unless one of their visuals would then
    /// come closer than their spacing to another visual, selected or not: then nothing moves.
    fn move_units(&mut self, units: &[Vec<usize>], moves: &[(i64, i64)]) -> bool {
        let mut list = Vec::new();
        let mut rects = Vec::new();
        let mut unit_of = Vec::new();
        for (k, (u, (dx, dy))) in units.iter().zip(moves).enumerate() {
            for i in u {
                let r = self.item(*i);
                list.push(*i);
                rects.push(Item { x: r.x + dx, y: r.y + dy, ..r });
                unit_of.push(k);
            }
        }
        if self.block_collides(&list, &rects) {
            return false;
        }
        let gap = |i: usize| self.placed().get(i).map_or(self.gap(), |p| self.gap_of(p.source));
        for a in 0..rects.len() {
            for b in a + 1..rects.len() {
                if unit_of[a] != unit_of[b] && rects[a].too_close(&rects[b], gap(list[a]).max(gap(list[b]))) {
                    return false;
                }
            }
        }
        for (u, (dx, dy)) in units.iter().zip(moves) {
            self.shift(u, *dx, *dy);
        }
        true
    }

    /// Aligns the blocks of the selection on the selection's box: `edge` 0 / 1 / 2 = left edges,
    /// centres, right edges (`horizontal`), or top edges, centres, bottom edges. Refused (false)
    /// when two visuals would come closer than their spacing: nothing moves then.
    pub fn align_units(&mut self, list: &[usize], horizontal: bool, edge: u8) -> bool {
        let units = self.units(list);
        if units.len() < 2 {
            return false;
        }
        let b = self.bbox(list);
        let moves: Vec<(i64, i64)> = units
            .iter()
            .map(|u| {
                let r = self.bbox(u);
                let (start, len, box_start, box_len) = if horizontal { (r.x, r.w, b.x, b.w) } else { (r.y, r.h, b.y, b.h) };
                let to = match edge {
                    0 => box_start,
                    1 => box_start + (box_len - len) / 2,
                    _ => box_start + box_len - len,
                };
                if horizontal { (to - start, 0) } else { (0, to - start) }
            })
            .collect();
        self.move_units(&units, &moves)
    }

    /// Spaces the blocks of the selection evenly across (`horizontal`) or along the film, the
    /// first and last staying where they are. Refused (false) when the room left between them is
    /// less than their spacing: nothing moves then.
    pub fn distribute(&mut self, list: &[usize], horizontal: bool) -> bool {
        let mut units = self.units(list);
        if units.len() < 3 {
            return false;
        }
        let pos = |r: Item| if horizontal { (r.x, r.w) } else { (r.y, r.h) };
        units.sort_by_key(|u| {
            let (p, l) = pos(self.bbox(u));
            2 * p + l
        });
        let spans: Vec<(i64, i64)> = units.iter().map(|u| pos(self.bbox(u))).collect();
        let (Some(first), Some(last)) = (spans.first(), spans.last()) else { return false };
        let total = last.0 + last.1 - first.0;
        let room = total - spans.iter().map(|s| s.1).sum::<i64>();
        let n = (units.len() - 1) as i64;
        let mut at = first.0;
        let mut moves = Vec::new();
        for (k, (p, l)) in spans.iter().enumerate() {
            // The rounding goes to the gaps in turn, so the last block stays exactly in place.
            let to = if k == spans.len() - 1 { *p } else { at };
            moves.push(if horizontal { (to - p, 0) } else { (0, to - p) });
            at = to + l + (room * (k as i64 + 1)) / n - (room * k as i64) / n;
        }
        self.move_units(&units, &moves)
    }

    /// The visuals of `list`, ready to paste.
    fn copy_block(&self, list: &[usize]) -> Option<Clip> {
        if list.is_empty() {
            return None;
        }
        let b = self.bbox(list);
        let members = list
            .iter()
            .filter_map(|i| {
                let p = self.placed().get(*i)?;
                let r = self.item(*i);
                Some((p.source, p.orientation, r.x - b.x, r.y - b.y, p.group))
            })
            .collect();
        Some(Clip { members, w: b.w, h: b.h })
    }

    /// Puts `clip` where there is room on the active sheet (its groups as new groups) and selects
    /// it.
    fn paste(&mut self, clip: &Clip) -> bool {
        let spot = self.sheet().with_gap(self.widest_gap()).pack(&[(clip.w, clip.h)], &self.items(), false).first().copied().flatten();
        let Some(sp) = spot else {
            self.status = crate::i18n::fmt(tl!("{n} not placed"), &[("n", &clip.members.len().to_string())]);
            return false;
        };
        let mut next = self.new_group_id();
        let mut renamed: Vec<(u32, u32)> = Vec::new();
        let first = self.placed().len();
        for (source, orientation, rx, ry, g) in &clip.members {
            let group = match *g {
                0 => 0,
                g => match renamed.iter().find(|(old, _)| *old == g) {
                    Some((_, new)) => *new,
                    None => {
                        renamed.push((g, next));
                        next += 1;
                        next - 1
                    }
                },
            };
            if let Some(list) = self.placed_mut() {
                list.push(Placed { source: *source, x: sp.x + rx, y: sp.y + ry, orientation: *orientation, group });
            }
        }
        self.selected = (first..self.placed().len()).collect();
        true
    }

    /// One more copy of the visuals of `list`, where there is room.
    pub fn duplicate_block(&mut self, list: &[usize]) -> bool {
        match self.copy_block(list) {
            Some(clip) => self.paste(&clip),
            None => false,
        }
    }

    pub fn delete_block(&mut self, list: &[usize]) {
        let mut list = list.to_vec();
        list.sort_unstable();
        list.dedup();
        if let Some(placed) = self.placed_mut() {
            for i in list.into_iter().rev() {
                if i < placed.len() {
                    placed.remove(i);
                }
            }
        }
        self.tidy_groups();
        self.selected.clear();
        self.drag = None;
    }

    fn new_sheet(&mut self) {
        if self.sheets.len() >= MAX_SHEETS {
            return;
        }
        let n = self.sheets.len() + 1;
        self.sheets.push(SheetDoc::new(n));
        self.active = self.sheets.len() - 1;
        self.selected.clear();
    }

    fn duplicate_sheet(&mut self, k: usize) {
        if self.sheets.len() >= MAX_SHEETS {
            return;
        }
        if let Some(doc) = self.sheets.get(k).cloned() {
            self.sheets.insert(k + 1, SheetDoc { name: format!("{} ({})", doc.name, tl!("copy")), ..doc });
            self.active = k + 1;
            self.selected.clear();
        }
    }

    fn delete_sheet(&mut self, k: usize) {
        if k < self.sheets.len() {
            self.sheets.remove(k);
        }
        if self.sheets.is_empty() {
            self.sheets.push(SheetDoc::new(1));
        }
        self.active = self.active.min(self.sheets.len() - 1);
        self.selected.clear();
    }

    fn start_over(&mut self) {
        self.sheets = vec![SheetDoc::new(1)];
        self.active = 0;
        self.selected.clear();
    }

    fn set_settings(&mut self, s: Settings) {
        if s == self.settings {
            return;
        }
        let film_changed = (s.width_cm, s.max_length_cm, s.edge_on, s.edge_cm)
            != (self.settings.width_cm, self.settings.max_length_cm, self.settings.edge_on, self.settings.edge_cm);
        self.settings = s;
        if let Ok(mut last) = LAST_SETTINGS.lock() {
            *last = Some(s);
        }
        if film_changed {
            // Visuals stay within the new film: no larger than it, moved onto it.
            let (iw, il) = self.sheet().inner();
            for k in 0..self.sources.len() {
                let (w, h) = self.size_of(k);
                if w > iw.max(il) || h > iw.max(il) {
                    let k2 = (iw.max(il) as f64 / w.max(h) as f64).min(1.0);
                    self.set_size(k, Some((w as f64 * k2) as i64), Some((h as f64 * k2) as i64));
                }
            }
            self.clamp_all();
        }
    }

    // ---------------------------------------------------------------- export

    /// `dtf.gangSheet`'s params for sheet `k` (only the visuals it uses).
    pub fn params(&self, app: &PhotocraftApp, k: usize) -> Result<Value, String> {
        let doc = self.sheets.get(k).ok_or("no such sheet")?;
        let mut used: Vec<usize> = doc.placed.iter().map(|p| p.source).collect();
        used.sort_unstable();
        used.dedup();
        let mut sources = Vec::new();
        for s in used.iter().filter_map(|i| self.sources.get(*i)) {
            let mut v = match &s.origin {
                Origin::File(path) => json!({"path": path}),
                Origin::Document(id) => {
                    let i = app
                        .session
                        .documents()
                        .iter()
                        .position(|d| d.doc.id == *id)
                        .ok_or_else(|| format!("{}: {}", s.name, tl!("the document was closed")))?;
                    json!({"document": i})
                }
            };
            v["widthCm"] = json!(f64::from(units_cm(s.w)));
            v["heightCm"] = json!(f64::from(units_cm(s.h)));
            if let Some(m) = s.margin {
                v["marginMm"] = json!(m as f64 / 10.0);
            }
            sources.push(v);
        }
        let placements: Vec<Value> = doc
            .placed
            .iter()
            .map(|p| {
                json!({
                    "source": used.iter().position(|u| *u == p.source).unwrap_or(0),
                    "x": f64::from(units_cm(p.x)), "y": f64::from(units_cm(p.y)),
                    "rotation": u32::from(p.orientation.quarter % 4) * 90,
                    "flipH": p.orientation.flip_h, "flipV": p.orientation.flip_v,
                })
            })
            .collect();
        Ok(json!({
            "sources": sources,
            "placements": placements,
            "filmWidth": self.settings.width_cm,
            "maxLength": self.settings.max_length_cm,
            "edgeMargin": self.edge() as f64 / 10.0,
            "spacing": self.gap() as f64 / 10.0,
            "resolution": cmd::DEFAULT_RESOLUTION,
            "name": doc.name,
        }))
    }

    pub fn describe(&self) -> Value {
        let conflicts = self.conflicts();
        let items = self.items();
        let s = &self.settings;
        json!({
            "filmWidth": s.width_cm, "maxLength": s.max_length_cm,
            "edgeMargin": if s.edge_on { s.edge_cm } else { 0.0 }, "spacing": if s.gap_on { s.gap_cm } else { 0.0 },
            "rotate": s.rotate, "showResolution": s.show_res, "showOverlap": s.show_overlap, "snap": s.snap,
            "warnBackground": s.warn_background, "warnTransparency": s.warn_semi, "grid": s.grid,
            "sources": self.sources.iter().enumerate().map(|(i, src)| json!({
                "name": src.name, "pixels": [src.px.0, src.px.1], "widthCm": units_cm(src.w), "heightCm": units_cm(src.h),
                "lock": src.lock, "marginMm": src.margin.map(|m| m as f64 / 10.0), "dpi": src.dpi().round(),
                "level": DpiLevel::of(src.dpi()), "quantity": src.quantity, "onSheet": self.copies_on_sheet(i),
                "background": src.background, "semiTransparent": src.semi,
            })).collect::<Vec<_>>(),
            "sheets": self.sheets.iter().map(|d| json!({"name": d.name, "images": d.placed.len(), "copies": d.copies})).collect::<Vec<_>>(),
            "active": self.active,
            "placements": self.placed().iter().zip(&conflicts).map(|(p, c)| json!({
                "source": p.source, "x": units_cm(p.x), "y": units_cm(p.y), "rotation": u32::from(p.orientation.quarter) * 90,
                "flipH": p.orientation.flip_h, "flipV": p.orientation.flip_v, "conflict": c, "group": p.group,
            })).collect::<Vec<_>>(),
            "selected": self.sel(),
            "usedLengthCm": units_cm(self.sheet().used_length(&items)),
            "conflicts": conflicts.iter().filter(|c| **c).count(),
            "undo": self.undo.len(), "redo": self.redo.len(),
        })
    }
}

// ---------------------------------------------------------------- opening, files, export

pub fn open(app: &mut PhotocraftApp) {
    if app.gang_sheet.is_none() {
        app.gang_sheet = Some(GangSheetDialog::new());
    }
}

/// Adds PNG files from disk; failures go to the status line.
pub fn add_files(app: &mut PhotocraftApp, paths: &[String]) {
    let Some(d) = app.gang_sheet.as_mut() else { return };
    let mut errors = Vec::new();
    for path in paths {
        match cmd::load_file(path) {
            Ok(v) => d.add_visual(Origin::File(path.clone()), &v),
            Err(e) => errors.push(e.to_string()),
        }
    }
    if !errors.is_empty() {
        d.status = errors.join(" · ");
    }
}

/// Adds the active document, flattened.
pub fn add_document(app: &mut PhotocraftApp) -> Result<(), String> {
    let st = app.session.active().ok_or(tl!("Open a document first"))?;
    let v = cmd::load_document(&st.doc, &st.doc.name).map_err(|e| e.to_string())?;
    let id = st.doc.id;
    if let Some(d) = app.gang_sheet.as_mut() {
        d.add_visual(Origin::Document(id), &v);
    }
    Ok(())
}

/// Files dropped on the window while the dialog is open go onto the sheet.
pub fn drop_files(app: &mut PhotocraftApp, files: &[egui::DroppedFileHandle]) {
    let paths: Vec<String> = files.iter().map(|f| f.path()).filter(|p| p.is_absolute()).map(|p| p.to_string_lossy().into_owned()).collect();
    add_files(app, &paths);
}

fn pick_files(app: &mut PhotocraftApp) {
    let initial_dir = crate::file_dialog::last_used_dir(&app.ui.recent_files);
    let r = app.ask_file(FileDialogRequest::Open { multiple: true, initial_dir, extensions: Some(vec!["png".into()]) }, |app, answer| match answer {
        FileDialogAnswer::Paths(paths) => {
            add_files(app, &paths);
            Ok(Value::Null)
        }
        _ => Err("PNG files are added from a folder on this computer".to_string()),
    });
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
        && let Some(d) = app.gang_sheet.as_mut()
    {
        d.status = e;
    }
}

/// The file of sheet `k` of `n`: the chosen path for a single sheet, else its stem plus the
/// sheet's number and, past one, its copies (`Gang sheet 2 x3.png`).
fn sheet_path(path: &str, k: usize, n: usize, copies: usize) -> String {
    if n <= 1 && copies <= 1 {
        return path.to_string();
    }
    let p = std::path::Path::new(path);
    let stem = p.file_stem().map_or_else(|| "Gang sheet".to_string(), |s| s.to_string_lossy().into_owned());
    let mut name = if n > 1 { format!("{stem} {}", k + 1) } else { stem };
    if copies > 1 {
        name.push_str(&format!(" x{copies}"));
    }
    p.with_file_name(format!("{name}.png")).to_string_lossy().into_owned()
}

/// Renders every sheet with visuals, writes it next to `path` and opens it; the dialog closes on
/// success.
fn export_to(app: &mut PhotocraftApp, path: &str) -> Result<Value, String> {
    let d = app.gang_sheet.as_ref().ok_or("Gang Sheet isn't open")?;
    let todo: Vec<usize> = (0..d.sheets.len()).filter(|k| d.sheets.get(*k).is_some_and(|s| !s.placed.is_empty())).collect();
    let mut jobs = Vec::new();
    for (n, k) in todo.iter().enumerate() {
        let mut params = d.params(app, *k)?;
        let copies = d.sheets.get(*k).map_or(1, |s| s.copies);
        params["output"] = json!(sheet_path(path, n, todo.len(), copies));
        jobs.push(params);
    }
    let mut done = Vec::new();
    for params in jobs {
        match app.run(cmd::GANG_SHEET, params) {
            Ok(v) => done.push(v),
            Err(e) => {
                if let Some(d) = app.gang_sheet.as_mut() {
                    d.status = e.clone();
                }
                return Err(e);
            }
        }
    }
    app.gang_sheet = None;
    Ok(json!({"sheets": done}))
}

fn export(app: &mut PhotocraftApp) {
    let Some(d) = app.gang_sheet.as_ref() else { return };
    if d.sheets.iter().all(|s| s.placed.is_empty()) {
        return;
    }
    if let Err(e) = (0..d.sheets.len()).try_for_each(|k| d.params(app, k).map(|_| ())) {
        if let Some(d) = app.gang_sheet.as_mut() {
            d.status = e;
        }
        return;
    }
    let r = app.pick_save("Gang sheet.png", |app, path| export_to(app, &path));
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
        && let Some(d) = app.gang_sheet.as_mut()
    {
        d.status = e;
    }
}

// ---------------------------------------------------------------- control channel

/// Menu / control routing. `Some` when this module handles the invocation.
pub fn menu(app: &mut PhotocraftApp, _ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id != cmd::GANG_SHEET {
        return None;
    }
    if params.as_object().is_none_or(|o| o.is_empty()) {
        open(app);
        return Some(Ok(app.gang_sheet.as_ref().map(GangSheetDialog::describe).unwrap_or(Value::Null)));
    }
    let ui = params.get("ui")?;
    open(app);
    Some(control(app, ui))
}

fn control(app: &mut PhotocraftApp, ui: &Value) -> Result<Value, String> {
    let cm = |v: &Value| v.as_f64().filter(|f| f.is_finite()).map(|f| (f * UNITS_PER_CM as f64).round() as i64);
    let index = |v: Option<&Value>| v.and_then(Value::as_u64).map(|v| v as usize);
    if let Some(paths) = ui.get("addFiles").and_then(Value::as_array) {
        let paths: Vec<String> = paths.iter().filter_map(Value::as_str).map(str::to_string).collect();
        add_files(app, &paths);
    }
    if ui.get("addDocument").and_then(Value::as_bool) == Some(true) {
        add_document(app)?;
    }
    if ui.get("cancel").and_then(Value::as_bool) == Some(true) {
        app.gang_sheet = None;
        return Ok(json!({"cancelled": true}));
    }
    if let Some(path) = ui.get("export").and_then(Value::as_str) {
        return export_to(app, path);
    }
    let d = app.gang_sheet.as_mut().ok_or("Gang Sheet isn't open")?;
    if let Some(set) = ui.get("set") {
        let mut s = d.settings;
        let f = |k: &str, v: f32| set.get(k).and_then(Value::as_f64).filter(|f| f.is_finite()).map_or(v, |f| f as f32);
        let b = |k: &str, v: bool| set.get(k).and_then(Value::as_bool).unwrap_or(v);
        s.width_cm = f("filmWidth", s.width_cm).clamp(10.0, 300.0);
        s.max_length_cm = f("maxLength", s.max_length_cm).clamp(10.0, MAX_LENGTH_CM);
        if let Some(e) = set.get("edgeMargin").and_then(Value::as_f64) {
            s.edge_on = e > 0.0;
            s.edge_cm = if e > 0.0 { (e as f32).clamp(0.0, 10.0) } else { s.edge_cm };
        }
        if let Some(g) = set.get("spacing").and_then(Value::as_f64) {
            s.gap_on = g > 0.0;
            s.gap_cm = if g > 0.0 { (g as f32).clamp(0.0, 10.0) } else { s.gap_cm };
        }
        s.rotate = b("rotate", s.rotate);
        s.show_res = b("showResolution", s.show_res);
        s.show_overlap = b("showOverlap", s.show_overlap);
        s.snap = b("snap", s.snap);
        s.warn_background = b("warnBackground", s.warn_background);
        s.warn_semi = b("warnTransparency", s.warn_semi);
        s.grid = b("grid", s.grid);
        d.set_settings(s);
    }
    if let Some(s) = ui.get("source") {
        let i = index(s.get("index")).filter(|i| *i < d.sources.len()).ok_or("`source` needs a valid index")?;
        d.checkpoint(None);
        if let Some(lock) = s.get("lock").and_then(Value::as_bool)
            && let Some(src) = d.sources.get_mut(i)
        {
            src.lock = lock;
        }
        let (w, h) = (s.get("widthCm").and_then(cm), s.get("heightCm").and_then(cm));
        if w.is_some() || h.is_some() {
            d.set_size(i, w, h);
        }
        if let Some(q) = s.get("quantity").and_then(Value::as_u64)
            && let Some(src) = d.sources.get_mut(i)
        {
            src.quantity = (q as usize).min(999);
        }
        if let Some(q) = s.get("onSheet").and_then(Value::as_u64) {
            d.set_copies_on_sheet(i, (q as usize).min(999));
        }
        if let Some(m) = s.get("marginMm")
            && let Some(src) = d.sources.get_mut(i)
        {
            src.margin = m.as_f64().filter(|v| v.is_finite()).map(|v| (v.clamp(0.0, 100.0) * 10.0).round() as i64);
        }
    }
    if let Some(i) = index(ui.get("removeSource")) {
        d.remove_source(i);
    }
    if ui.get("autoBuild").and_then(Value::as_bool) == Some(true) {
        d.checkpoint(None);
        let left = d.auto_build();
        if left > 0 {
            d.status = crate::i18n::fmt(tl!("{n} not placed"), &[("n", &left.to_string())]);
        }
    }
    if ui.get("newSheet").and_then(Value::as_bool) == Some(true) {
        d.checkpoint(None);
        d.new_sheet();
    }
    if let Some(k) = index(ui.get("sheet")).filter(|k| *k < d.sheets.len()) {
        d.active = k;
        d.selected.clear();
    }
    if let Some(v) = ui.get("select") {
        let n = d.placed().len();
        let picked: Vec<usize> = match v {
            Value::Array(list) => list.iter().filter_map(|v| index(Some(v))).filter(|i| *i < n).collect(),
            v => index(Some(v)).filter(|i| *i < n).into_iter().collect(),
        };
        d.selected = d.whole(&picked);
    }
    if ui.get("selectAll").and_then(Value::as_bool) == Some(true) {
        d.select_all();
    }
    if ui.get("group").and_then(Value::as_bool) == Some(true) {
        d.checkpoint(None);
        if !d.group() {
            d.undo.pop();
            return Err("select at least two visuals to group".into());
        }
    }
    if ui.get("ungroup").and_then(Value::as_bool) == Some(true) {
        d.checkpoint(None);
        if !d.ungroup() {
            d.undo.pop();
        }
    }
    // The visuals a tool works on: `index`'s (with its group), else the selection.
    let target = |d: &GangSheetDialog, v: Option<&Value>| -> Result<Vec<usize>, String> {
        let list = match index(v) {
            Some(i) if i < d.placed().len() => d.members(i),
            Some(_) => return Err("no such visual".into()),
            None => d.sel(),
        };
        if list.is_empty() { Err("select a visual first".into()) } else { Ok(list) }
    };
    if let Some(m) = ui.get("move") {
        let list = target(d, m.get("index"))?;
        let (Some(x), Some(y)) = (m.get("x").and_then(cm), m.get("y").and_then(cm)) else { return Err("`move` needs x and y (cm)".into()) };
        d.checkpoint(None);
        if !d.move_block(&list, x, y) {
            return Err(tl!("Too close to another visual").to_string());
        }
    }
    if let Some(q) = ui.get("rotate").and_then(Value::as_i64) {
        let list = target(d, None)?;
        d.checkpoint(None);
        if !d.rotate_block(&list, q as i32) {
            return Err(tl!("No room to turn this visual").to_string());
        }
    }
    if let Some(m) = ui.get("mirror").and_then(Value::as_str) {
        let list = target(d, None)?;
        d.checkpoint(None);
        d.mirror_block(&list, m == "h");
    }
    if let Some(m) = ui.get("stretch").and_then(Value::as_str) {
        let i = d.single().ok_or("select one visual alone to stretch it")?;
        d.checkpoint(None);
        d.stretch(i, m == "h");
    }
    if let Some(m) = ui.get("center").and_then(Value::as_str) {
        let list = target(d, None)?;
        d.checkpoint(None);
        if !d.center_block(&list, m != "v", m != "h") {
            return Err(tl!("Too close to another visual").to_string());
        }
    }
    if let Some(m) = ui.get("align").and_then(Value::as_str) {
        let list = target(d, None)?;
        let side = match m {
            "left" => Side::Left,
            "right" => Side::Right,
            "top" => Side::Top,
            _ => Side::Bottom,
        };
        d.checkpoint(None);
        d.align_block(&list, side);
    }
    // `alignSelection`: left, hcenter, right, top, vcenter, bottom; `distribute`: h, v.
    if let Some(m) = ui.get("alignSelection").and_then(Value::as_str) {
        let list = target(d, None)?;
        let (horizontal, edge) = match m {
            "left" => (true, 0),
            "hcenter" => (true, 1),
            "right" => (true, 2),
            "top" => (false, 0),
            "vcenter" => (false, 1),
            _ => (false, 2),
        };
        d.checkpoint(None);
        if !d.align_units(&list, horizontal, edge) {
            d.undo.pop();
            return Err(tl!("Too close to another visual").to_string());
        }
    }
    if let Some(m) = ui.get("distribute").and_then(Value::as_str) {
        let list = target(d, None)?;
        d.checkpoint(None);
        if !d.distribute(&list, m != "v") {
            d.undo.pop();
            return Err(tl!("Too close to another visual").to_string());
        }
    }
    if ui.get("duplicate").and_then(Value::as_bool) == Some(true)
        && let Ok(list) = target(d, None)
    {
        d.checkpoint(None);
        d.duplicate_block(&list);
    }
    if ui.get("copy").and_then(Value::as_bool) == Some(true)
        && let Ok(list) = target(d, None)
    {
        d.clipboard = d.copy_block(&list);
    }
    if ui.get("paste").and_then(Value::as_bool) == Some(true)
        && let Some(clip) = d.clipboard.clone()
    {
        d.checkpoint(None);
        d.paste(&clip);
    }
    if let Some(s) = index(ui.get("fill")) {
        d.checkpoint(None);
        d.fill(s);
    }
    if ui.get("delete").and_then(Value::as_bool) == Some(true)
        && let Ok(list) = target(d, None)
    {
        d.checkpoint(None);
        d.delete_block(&list);
    }
    if let Some(k) = index(ui.get("duplicateSheet")) {
        d.checkpoint(None);
        d.duplicate_sheet(k);
    }
    if let Some(k) = index(ui.get("deleteSheet")) {
        d.checkpoint(None);
        d.delete_sheet(k);
    }
    if let Some(n) = ui.get("sheetCopies").and_then(Value::as_u64) {
        let k = d.active;
        if let Some(doc) = d.sheets.get_mut(k) {
            doc.copies = (n as usize).clamp(1, 999);
        }
    }
    if let Some(name) = ui.get("renameSheet").and_then(Value::as_str) {
        let k = d.active;
        if let Some(doc) = d.sheets.get_mut(k) {
            doc.name = name.trim().to_string();
        }
    }
    if let Some(tab) = ui.get("tab").and_then(Value::as_str) {
        d.tab = if tab == "settings" { Tab::Settings } else { Tab::Home };
    }
    if let Some(on) = ui.get("autoView").and_then(Value::as_bool) {
        d.auto_view = on;
    }
    if ui.get("startOver").and_then(Value::as_bool) == Some(true) {
        d.checkpoint(None);
        d.start_over();
    }
    if ui.get("undo").and_then(Value::as_bool) == Some(true) {
        d.undo();
    }
    if ui.get("redo").and_then(Value::as_bool) == Some(true) {
        d.redo();
    }
    Ok(d.describe())
}

// ---------------------------------------------------------------- drawing helpers

/// The image of a visual drawn into `r` as it is placed: turned `quarter` × 90° clockwise, then
/// mirrored (as the command renders it).
fn image_shape(tex: egui::TextureId, r: ERect, o: Orientation) -> Shape {
    // UV of the source image at the screen corners top-left, top-right, bottom-right, bottom-left.
    let mut uv = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
    // Each clockwise quarter turn: the screen's top-left now shows what was at bottom-left.
    for _ in 0..o.quarter % 4 {
        uv = [uv[3], uv[0], uv[1], uv[2]];
    }
    if o.flip_h {
        uv = [uv[1], uv[0], uv[3], uv[2]];
    }
    if o.flip_v {
        uv = [uv[3], uv[2], uv[1], uv[0]];
    }
    let mut m = Mesh::with_texture(tex);
    for (p, t) in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].into_iter().zip(uv) {
        m.vertices.push(egui::epaint::Vertex { pos: p, uv: t, color: Color32::WHITE });
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    Shape::mesh(m)
}

fn cm_text(units: i64) -> String {
    widgets::fmt_num_prec(units as f64 / UNITS_PER_CM as f64, 2)
}

/// "300 ppi" in the UI language.
fn ppi_text(dpi: f64) -> String {
    crate::i18n::fmt(tl!("{dpi} ppi"), &[("dpi", &format!("{dpi:.0}"))])
}

/// A square icon button of the toolbar: `angle` turns the icon (radians), `mirror` flips it.
fn tool_button(ui: &mut egui::Ui, icon: &str, tip: &str, enabled: bool, angle: f32, mirror: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, t.hover);
    }
    let tint = if !enabled {
        t.text_faint.gamma_multiply(0.5)
    } else if icon == "trash" {
        t.danger
    } else {
        t.text
    };
    if let Some(shape) = drawn_icon(icon, ERect::from_center_size(rect.center(), vec2(16.0, 16.0)), tint) {
        ui.painter().extend(shape);
        let resp = resp.on_hover_text(tip);
        return if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };
    }
    let mut img = crate::icons::image(icon, 16.0, tint);
    if mirror {
        img = img.uv(ERect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0)));
    }
    if angle != 0.0 {
        img = img.rotate(angle, vec2(0.5, 0.5));
    }
    img.paint_at(ui, ERect::from_center_size(rect.center(), vec2(16.0, 16.0)));
    let resp = resp.on_hover_text(tip);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// The toolbar icons the icon set lacks, drawn: mirroring (two triangles either side of an axis)
/// and stretching (an arrow between two bars), across (`-h`) or along (`-v`) the film.
fn drawn_icon(name: &str, r: ERect, c: Color32) -> Option<Vec<Shape>> {
    let stroke = Stroke::new(1.5, c);
    let turn = |p: Pos2| -> Pos2 { pos2(r.center().x - (p.y - r.center().y), r.center().y + (p.x - r.center().x)) };
    if let Some(shapes) = align_icon(name, r, c) {
        return Some(shapes);
    }
    let (base, vertical) = match name {
        "flip-h" => ("flip", false),
        "flip-v" => ("flip", true),
        "stretch-h" => ("stretch", false),
        "stretch-v" => ("stretch", true),
        _ => return None,
    };
    let at = |x: f32, y: f32| {
        let p = pos2(r.left() + x * r.width(), r.top() + y * r.height());
        if vertical { turn(p) } else { p }
    };
    let mut out = Vec::new();
    if base == "flip" {
        out.extend(Shape::dashed_line(&[at(0.5, 0.0), at(0.5, 1.0)], Stroke::new(1.0, c), 2.0, 2.0));
        out.push(Shape::convex_polygon(vec![at(0.4, 0.2), at(0.4, 0.8), at(0.05, 0.8)], c, Stroke::NONE));
        out.push(Shape::closed_line(vec![at(0.6, 0.2), at(0.6, 0.8), at(0.95, 0.8)], stroke));
    } else {
        out.push(Shape::line_segment([at(0.05, 0.15), at(0.05, 0.85)], stroke));
        out.push(Shape::line_segment([at(0.95, 0.15), at(0.95, 0.85)], stroke));
        out.push(Shape::line_segment([at(0.18, 0.5), at(0.82, 0.5)], stroke));
        out.push(Shape::line(vec![at(0.35, 0.33), at(0.18, 0.5), at(0.35, 0.67)], stroke));
        out.push(Shape::line(vec![at(0.65, 0.33), at(0.82, 0.5), at(0.65, 0.67)], stroke));
    }
    Some(out)
}

/// Align and distribute icons: a guide line and two bars set against it (`al-*`), or three bars
/// evenly spaced (`dist-*`).
fn align_icon(name: &str, r: ERect, c: Color32) -> Option<Vec<Shape>> {
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    let bar = |x0: f32, y0: f32, x1: f32, y1: f32| Shape::rect_filled(ERect::from_min_max(at(x0, y0), at(x1, y1)), 1.0, c);
    let line = |x0: f32, y0: f32, x1: f32, y1: f32| Shape::line_segment([at(x0, y0), at(x1, y1)], Stroke::new(1.5, c));
    Some(match name {
        "al-left" => vec![line(0.08, 0.0, 0.08, 1.0), bar(0.2, 0.18, 0.95, 0.42), bar(0.2, 0.58, 0.65, 0.82)],
        "al-hcenter" => vec![line(0.5, 0.0, 0.5, 1.0), bar(0.1, 0.18, 0.9, 0.42), bar(0.25, 0.58, 0.75, 0.82)],
        "al-right" => vec![line(0.92, 0.0, 0.92, 1.0), bar(0.05, 0.18, 0.8, 0.42), bar(0.35, 0.58, 0.8, 0.82)],
        "al-top" => vec![line(0.0, 0.08, 1.0, 0.08), bar(0.18, 0.2, 0.42, 0.95), bar(0.58, 0.2, 0.82, 0.65)],
        "al-vcenter" => vec![line(0.0, 0.5, 1.0, 0.5), bar(0.18, 0.1, 0.42, 0.9), bar(0.58, 0.25, 0.82, 0.75)],
        "al-bottom" => vec![line(0.0, 0.92, 1.0, 0.92), bar(0.18, 0.05, 0.42, 0.8), bar(0.58, 0.35, 0.82, 0.8)],
        "dist-h" => vec![bar(0.02, 0.2, 0.22, 0.8), bar(0.4, 0.2, 0.6, 0.8), bar(0.78, 0.2, 0.98, 0.8)],
        "dist-v" => vec![bar(0.2, 0.02, 0.8, 0.22), bar(0.2, 0.4, 0.8, 0.6), bar(0.2, 0.78, 0.8, 0.98)],
        _ => return None,
    })
}

/// A switch on the right of a labelled row (Settings).
fn switch_row(ui: &mut egui::Ui, label: &str, on: &mut bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut changed = false;
    let w = ui.available_width();
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(w - 44.0, 26.0), egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true), |ui| {
            ui.set_min_width(w - 44.0);
            ui.add(egui::Label::new(egui::RichText::new(label).color(t.text)).wrap());
        });
        changed = switch(ui, on);
    });
    changed
}

/// A label in a box `w` wide, on the left (a column of field names).
fn fixed_label(ui: &mut egui::Ui, text: egui::RichText, w: f32, h: f32) {
    ui.allocate_ui_with_layout(vec2(w, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(vec2(w, h));
        ui.add(egui::Label::new(text).truncate());
    });
}

fn switch(ui: &mut egui::Ui, on: &mut bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 18.0), Sense::click());
    let how_on = ui.ctx().animate_bool(resp.id, *on);
    let bg = if *on { t.primary_bg } else { t.field_border };
    ui.painter().rect_filled(rect, 9.0, bg);
    let x = egui::lerp((rect.left() + 9.0)..=(rect.right() - 9.0), how_on);
    ui.painter().circle_filled(pos2(x, rect.center().y), 7.0, Color32::WHITE);
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        *on = !*on;
        return true;
    }
    false
}

/// A labelled number row: label on the left, field (typed) and unit on the right.
fn number_row(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, unit: &str, decimals: u32) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut changed = false;
    ui.horizontal(|ui| {
        fixed_label(ui, egui::RichText::new(label).color(t.text), 128.0, 24.0);
        changed = widgets::typed_value_field(ui, value, range, unit, 120.0, decimals).changed();
    });
    changed
}

fn card_frame(t: &Tokens) -> egui::Frame {
    egui::Frame::NONE.fill(Color32::WHITE).stroke(Stroke::new(1.0, t.card_border)).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(10))
}

// ---------------------------------------------------------------- the window

/// What the frame's widgets asked for, done once the dialog is no longer borrowed.
#[derive(Default)]
struct Actions {
    add_files: bool,
    add_document: bool,
    export: bool,
    cancel: bool,
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if app.gang_sheet.is_none() {
        return;
    }
    let has_doc = app.session.active().is_some();
    let mut act = Actions::default();
    light_scope(ctx, || {
        draw(app, ctx, has_doc, &mut act);
        confirm_dialog(app, ctx, &mut act);
    });
    if act.add_files {
        pick_files(app);
    }
    if act.add_document
        && let Err(e) = add_document(app)
        && let Some(d) = app.gang_sheet.as_mut()
    {
        d.status = e;
    }
    if act.export {
        // Visuals too close or off the film: ask first.
        let ask = app.gang_sheet.as_ref().is_some_and(|d| !d.confirm_export && d.all_conflicts() > 0);
        if ask {
            if let Some(d) = app.gang_sheet.as_mut() {
                d.confirm_export = true;
            }
        } else {
            export(app);
        }
    }
    if act.cancel {
        app.gang_sheet = None;
    }
}

fn draw(app: &mut PhotocraftApp, ctx: &egui::Context, has_doc: bool, act: &mut Actions) {
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("gang-sheet-dialog")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let Some(d) = app.gang_sheet.as_mut() else { return };
        let t = Tokens::get(ctx);
        // Previews become textures here, where the context is.
        for (i, s) in d.sources.iter_mut().enumerate() {
            if let Some(img) = s.preview.take() {
                s.tex = Some(ctx.load_texture(format!("gang-sheet-{i}-{}", s.name), img, egui::TextureOptions::LINEAR));
            }
        }
        let (full, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(full, 0.0, Color32::WHITE);
        let header = ERect::from_min_size(full.min, vec2(full.width(), HEADER_H));
        let toolbar = ERect::from_min_size(pos2(full.left() + RAIL_W + LEFT_W, header.bottom()), vec2(full.width() - RAIL_W - LEFT_W, TOOLBAR_H));
        let rail = ERect::from_min_max(pos2(full.left(), header.bottom()), pos2(full.left() + RAIL_W, full.bottom()));
        let left = ERect::from_min_max(pos2(rail.right(), header.bottom()), pos2(rail.right() + LEFT_W, full.bottom()));
        let body = ERect::from_min_max(pos2(left.right(), toolbar.bottom()), full.max);
        let right = ERect::from_min_max(pos2(body.right() - RIGHT_W, body.top()), body.max);
        let canvas = ERect::from_min_max(body.min, pos2(right.left(), body.bottom()));
        header_bar(ui, d, header, &t, act);
        rail_bar(ui, d, rail, &t);
        match d.tab {
            Tab::Home => home_panel(ui, d, left, &t, has_doc, act),
            Tab::Settings => settings_panel(ui, d, left, &t),
        }
        toolbar_bar(ui, d, toolbar, &t);
        if d.auto_view {
            auto_view(ui, d, body, &t, act, has_doc);
        } else {
            sheet_view(ui, d, canvas, &t);
            sheets_panel(ui, d, right, &t);
        }
        for line in [header.left_bottom(), toolbar.left_bottom()] {
            let to = if line == header.left_bottom() { header.right_bottom() } else { toolbar.right_bottom() };
            p.line_segment([line, to], Stroke::new(1.0, t.separator));
        }
        p.line_segment([rail.right_top(), rail.right_bottom()], Stroke::new(1.0, t.separator));
        p.line_segment([left.right_top(), left.right_bottom()], Stroke::new(1.0, t.separator));
    });
    if !ctx.text_edit_focused()
        && let Some(d) = app.gang_sheet.as_mut()
        && !d.confirm_export
    {
        keys(d, ctx, act);
    }
}

fn header_bar(ui: &mut egui::Ui, d: &mut GangSheetDialog, rect: ERect, t: &Tokens, act: &mut Actions) {
    let mut h = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(18.0, 10.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    crate::icons::image("layers", 20.0, t.text).paint_at(&h, ERect::from_min_size(h.cursor().min + vec2(0.0, 6.0), vec2(20.0, 20.0)));
    h.add_space(28.0);
    h.label(egui::RichText::new(tl!("Gang Sheet")).font(crate::theme::semibold(16.0)).color(t.text));
    h.add_space(16.0);
    if !d.status.is_empty() {
        h.label(egui::RichText::new(&d.status).color(t.warning));
    }
    h.with_layout(egui::Layout::right_to_left(egui::Align::Center), |h| {
        let any = d.sheets.iter().any(|s| !s.placed.is_empty());
        let label = if d.sheets.len() > 1 { tl!("Export Sheets (PNG)…") } else { tl!("Export PNG…") };
        if h.add_enabled_ui(any, |h| widgets::primary_button(h, label, 150.0)).inner.clicked() {
            act.export = true;
        }
        if widgets::secondary_button(h, tl!("Cancel"), 90.0).clicked() {
            act.cancel = true;
        }
    });
}

fn rail_bar(ui: &mut egui::Ui, d: &mut GangSheetDialog, rect: ERect, t: &Tokens) {
    let mut y = rect.top() + 8.0;
    for (tab, icon, label) in [(Tab::Home, "house", tl!("Home")), (Tab::Settings, "settings", tl!("Settings"))] {
        let r = ERect::from_min_size(pos2(rect.left() + 5.0, y), vec2(RAIL_W - 10.0, 52.0));
        let resp = ui.interact(r, ui.id().with(("gang-rail", label)), Sense::click());
        let on = d.tab == tab;
        let (bg, fg) = if on {
            (t.primary_bg, t.primary_text)
        } else if resp.hovered() {
            (t.hover, t.text)
        } else {
            (Color32::TRANSPARENT, t.text)
        };
        ui.painter().rect_filled(r, 8.0, bg);
        crate::icons::image(icon, 18.0, fg).paint_at(ui, ERect::from_center_size(r.center() - vec2(0.0, 8.0), vec2(18.0, 18.0)));
        ui.painter().text(r.center() + vec2(0.0, 13.0), Align2::CENTER_CENTER, label, FontId::proportional(10.0), fg);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            d.tab = tab;
        }
        y += 58.0;
    }
}

fn home_panel(ui: &mut egui::Ui, d: &mut GangSheetDialog, rect: ERect, t: &Tokens, has_doc: bool, act: &mut Actions) {
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 14.0))));
    panel.spacing_mut().item_spacing.y = 8.0;
    let w = panel.available_width();
    // Drop zone.
    let zone = egui::Frame::NONE.stroke(Stroke::new(1.0, t.field_border)).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(12));
    zone.show(&mut panel, |ui| {
        ui.set_width(w - 24.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(tl!("Drag and drop a file here or click to")).color(t.text));
            if widgets::primary_button(ui, tl!("Import Images"), 150.0).clicked() {
                act.add_files = true;
            }
            ui.label(egui::RichText::new(tl!("Supported format: PNG (cropped to its visible pixels)")).color(t.text_faint).size(11.0));
            if ui.add_enabled_ui(has_doc, |ui| widgets::secondary_button(ui, tl!("Add Open Document"), 180.0)).inner.clicked() {
                act.add_document = true;
            }
        });
    });
    // Thumbnails: click to add a copy, × to remove the visual, the badge counts its copies.
    let mut add = None;
    let mut remove = None;
    if !d.sources.is_empty() {
        let cell = 80.0;
        let per_row = ((w + 8.0) / (cell + 8.0)).floor().max(1.0) as usize;
        for row in d.sources.chunks(per_row).enumerate() {
            panel.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for (k, s) in row.1.iter().enumerate() {
                    let i = row.0 * per_row + k;
                    let (r, resp) = ui.allocate_exact_size(vec2(cell, cell + 16.0), Sense::click());
                    let img = ERect::from_min_size(r.min, vec2(cell, cell));
                    widgets::checker(ui.painter(), img, 6.0);
                    if let Some(tex) = &s.tex {
                        let k = (cell / s.px.0.max(1) as f32).min(cell / s.px.1.max(1) as f32);
                        let ir = ERect::from_center_size(img.center(), vec2(s.px.0 as f32 * k, s.px.1 as f32 * k));
                        ui.painter().image(tex.id(), ir, ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    ui.painter().rect_stroke(img, 4.0, Stroke::new(1.0, if resp.hovered() { t.text } else { t.card_border }), StrokeKind::Inside);
                    let name_r = ERect::from_min_size(pos2(r.left(), img.bottom()), vec2(cell, 16.0));
                    ui.painter().rect_filled(name_r, 0.0, t.primary_bg);
                    let name = if s.name.chars().count() > 12 { format!("{}…", s.name.chars().take(11).collect::<String>()) } else { s.name.clone() };
                    ui.painter().text(name_r.center(), Align2::CENTER_CENTER, name, FontId::proportional(9.5), t.primary_text);
                    let n = d.sheets.get(d.active).map_or(0, |sh| sh.placed.iter().filter(|p| p.source == i).count());
                    let badge = ERect::from_min_size(pos2(img.right() - 16.0, img.bottom() - 16.0), vec2(16.0, 16.0));
                    ui.painter().rect_filled(badge, 0.0, t.primary_bg);
                    ui.painter().text(badge.center(), Align2::CENTER_CENTER, n.to_string(), FontId::proportional(9.5), t.primary_text);
                    let x = ERect::from_min_size(pos2(img.right() - 15.0, img.top() + 1.0), vec2(14.0, 14.0));
                    let xr = ui.interact(x, ui.id().with(("gang-x", i)), Sense::click());
                    ui.painter().rect_filled(x, 3.0, t.danger);
                    crate::icons::image("x", 10.0, Color32::WHITE).paint_at(ui, ERect::from_center_size(x.center(), vec2(10.0, 10.0)));
                    if xr.on_hover_text(tl!("Remove")).clicked() {
                        remove = Some(i);
                    } else if resp.on_hover_text(tl!("Add a copy to the sheet")).clicked() {
                        add = Some(i);
                    }
                }
            });
        }
    }
    if let Some(i) = remove {
        d.remove_source(i);
    }
    if let Some(i) = add {
        d.checkpoint(None);
        if d.add_copies(i, 1) == 0 {
            d.status = crate::i18n::fmt(tl!("{n} not placed"), &[("n", "1")]);
        } else {
            d.selected = vec![d.placed().len() - 1];
        }
    }
    widgets::hairline(&mut panel);
    let list = d.sel();
    if list.len() > 1 {
        multi_panel(&mut panel, d, &list, t);
        return;
    }
    let Some(i) = list.first().copied() else {
        panel.add_space(30.0);
        panel.vertical_centered(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(90.0, 90.0), Sense::hover());
            ui.painter().rect_stroke(r, 6.0, Stroke::new(1.0, t.card_border), StrokeKind::Inside);
            crate::icons::image("mouse-pointer-2", 34.0, t.text_faint).paint_at(ui, ERect::from_center_size(r.center() + vec2(18.0, 18.0), vec2(34.0, 34.0)));
            ui.add_space(12.0);
            ui.label(egui::RichText::new(tl!("Select an object to edit.")).color(t.text));
        });
        return;
    };
    selection_panel(&mut panel, d, i, t);
}

fn selection_panel(ui: &mut egui::Ui, d: &mut GangSheetDialog, i: usize, t: &Tokens) {
    let Some(source) = d.placed().get(i).map(|p| p.source) else { return };
    let Some(src) = d.sources.get(source).cloned() else { return };
    ui.label(egui::RichText::new(&src.name).font(crate::theme::medium(13.0)).color(t.text));
    let (mut w, mut h) = (units_cm(src.w), units_cm(src.h));
    if number_row(ui, tl!("Width:"), &mut w, 0.1..=300.0, "cm", 2) {
        d.checkpoint(Some(format!("w{source}")));
        d.set_size(source, Some(cm_units(w)), None);
    }
    if number_row(ui, tl!("Height:"), &mut h, 0.1..=300.0, "cm", 2) {
        d.checkpoint(Some(format!("h{source}")));
        d.set_size(source, None, Some(cm_units(h)));
    }
    ui.horizontal(|ui| {
        fixed_label(ui, egui::RichText::new(tl!("Proportions:")).color(t.text), 128.0, 24.0);
        let ratio = if src.h > 0 { src.w as f64 / src.h as f64 } else { 1.0 };
        let (r, _) = ui.allocate_exact_size(vec2(120.0, 24.0), Sense::hover());
        ui.painter().rect_stroke(r, 6.0, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
        ui.painter().text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, widgets::fmt_num_prec(ratio, 2), FontId::proportional(12.0), t.text);
        let lock_r = ERect::from_center_size(r.right_center() - vec2(14.0, 0.0), vec2(20.0, 20.0));
        let lr = ui.interact(lock_r, ui.id().with(("gang-lock", source)), Sense::click());
        crate::icons::image(if src.lock { "lock" } else { "lock-open" }, 14.0, t.text).paint_at(ui, ERect::from_center_size(lock_r.center(), vec2(14.0, 14.0)));
        if lr.on_hover_text(tl!("Lock Proportions")).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            d.checkpoint(None);
            if let Some(s) = d.sources.get_mut(source) {
                s.lock = !s.lock;
            }
        }
    });
    // Resolution and warnings.
    let dpi = src.dpi();
    let level = DpiLevel::of(dpi);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
        ui.painter().rect_filled(r, 2.0, level_color(level));
        ui.label(egui::RichText::new(level_label(level)).color(t.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(egui::RichText::new(ppi_text(dpi)).color(t.text_faint)));
    });
    warnings_lines(ui, d, &src, t);
    card_frame(t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let mut q = d.copies_on_sheet(source) as f32;
        ui.horizontal(|ui| {
            fixed_label(ui, egui::RichText::new(tl!("Number of Images:")).color(t.text), 128.0, 24.0);
            if widgets::typed_value_field(ui, &mut q, 0.0..=999.0, "", 70.0, 0).changed() {
                d.checkpoint(Some(format!("q{source}")));
                d.set_copies_on_sheet(source, q.round().max(0.0) as usize);
            }
        });
        let mut own = src.margin.is_some();
        let mut m = units_cm(src.margin.unwrap_or_else(|| d.gap()));
        ui.horizontal(|ui| {
            fixed_label(ui, egui::RichText::new(tl!("Margin (cm)")).color(t.text), 128.0, 24.0);
            let changed = ui.add_enabled_ui(own, |ui| widgets::typed_value_field(ui, &mut m, 0.0..=10.0, "", 70.0, 2)).inner.changed();
            if changed {
                d.checkpoint(Some(format!("m{source}")));
                if let Some(s) = d.sources.get_mut(source) {
                    s.margin = Some(cm_units(m));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if switch(ui, &mut own) {
                    d.checkpoint(None);
                    if let Some(s) = d.sources.get_mut(source) {
                        s.margin = own.then(|| cm_units(m));
                    }
                }
            });
        });
        ui.vertical(|ui| {
            let bw = ui.available_width();
            if widgets::primary_button(ui, tl!("Auto Fill"), bw).on_hover_text(tl!("Fill the room left on the sheet with this visual")).clicked() {
                d.checkpoint(None);
                if d.fill(source) == 0 {
                    d.status = tl!("No room left on the sheet").to_string();
                }
            }
            if widgets::primary_button(ui, tl!("Duplicate Image"), bw).clicked() {
                d.checkpoint(None);
                d.duplicate_block(&[i]);
            }
        });
    });
}

/// Several visuals selected: their count and box, Group / Ungroup, Duplicate, Delete.
fn multi_panel(ui: &mut egui::Ui, d: &mut GangSheetDialog, list: &[usize], t: &Tokens) {
    let one_group = d.is_one_group(list);
    let title = if one_group { tl!("Group") } else { tl!("Selection") };
    ui.label(egui::RichText::new(title).font(crate::theme::medium(13.0)).color(t.text));
    ui.label(egui::RichText::new(crate::i18n::fmt(tl!("{n} visuals selected"), &[("n", &list.len().to_string())])).color(t.text_dim));
    let b = d.bbox(list);
    egui::Grid::new("gang-multi").num_columns(2).spacing(vec2(24.0, 2.0)).show(ui, |ui| {
        for (k, v) in [(tl!("Width"), format!("{} cm", cm_text(b.w))), (tl!("Height"), format!("{} cm", cm_text(b.h)))] {
            ui.label(egui::RichText::new(k).color(t.text));
            ui.label(egui::RichText::new(v).color(t.text));
            ui.end_row();
        }
    });
    // Align on the selection's box, distribute evenly (a group counts as one block).
    let blocks = d.units(list).len();
    let mut op: Option<(bool, Option<u8>)> = None;
    card_frame(t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(tl!("Align")).color(t.text));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (icon, tip, horizontal, edge) in [
                ("al-left", tl!("Left Edges"), true, 0),
                ("al-hcenter", tl!("Horizontal Centers"), true, 1),
                ("al-right", tl!("Right Edges"), true, 2),
                ("al-top", tl!("Top Edges"), false, 0),
                ("al-vcenter", tl!("Vertical Centers"), false, 1),
                ("al-bottom", tl!("Bottom Edges"), false, 2),
            ] {
                if tool_button(ui, icon, tip, blocks >= 2, 0.0, false).clicked() {
                    op = Some((horizontal, Some(edge)));
                }
                if edge == 2 && horizontal {
                    widgets::vline(ui, 22.0);
                }
            }
        });
        ui.label(egui::RichText::new(tl!("Distribute")).color(t.text));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (icon, tip, horizontal) in [("dist-h", tl!("Horizontally"), true), ("dist-v", tl!("Vertically"), false)] {
                if tool_button(ui, icon, tip, blocks >= 3, 0.0, false).clicked() {
                    op = Some((horizontal, None));
                }
            }
        });
    });
    if let Some((horizontal, edge)) = op {
        d.checkpoint(None);
        let ok = match edge {
            Some(e) => d.align_units(list, horizontal, e),
            None => d.distribute(list, horizontal),
        };
        if !ok {
            d.undo.pop();
            d.status = tl!("Too close to another visual").to_string();
        }
    }
    card_frame(t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.vertical(|ui| {
            let bw = ui.available_width();
            if ui
                .add_enabled_ui(!one_group, |ui| widgets::primary_button(ui, tl!("Group the Selection"), bw))
                .inner
                .on_hover_text(tl!("Group (Ctrl+G)"))
                .clicked()
            {
                d.checkpoint(None);
                d.group();
            }
            if ui
                .add_enabled_ui(d.any_grouped(list), |ui| widgets::secondary_button(ui, tl!("Ungroup"), bw))
                .inner
                .on_hover_text(tl!("Ungroup (Ctrl+Shift+G)"))
                .clicked()
            {
                d.checkpoint(None);
                d.ungroup();
            }
            if widgets::secondary_button(ui, tl!("Duplicate"), bw).on_hover_text(tl!("Duplicate (Ctrl+D)")).clicked() {
                d.checkpoint(None);
                d.duplicate_block(list);
            }
            if widgets::secondary_button(ui, tl!("Delete"), bw).on_hover_text(tl!("Delete (Del)")).clicked() {
                d.checkpoint(None);
                d.delete_block(list);
            }
        });
    });
}

fn warnings_lines(ui: &mut egui::Ui, d: &GangSheetDialog, src: &Source, t: &Tokens) {
    let mut lines = Vec::new();
    if d.settings.warn_background && src.background {
        lines.push(tl!("Background not removed (opaque corners)").to_string());
    }
    if d.settings.warn_semi && src.semi > cmd::SEMI_TRANSPARENT_WARNING {
        lines.push(crate::i18n::fmt(tl!("Semi-transparent pixels: {p} %"), &[("p", &widgets::fmt_num_prec(src.semi * 100.0, 1))]));
    }
    for l in lines {
        ui.horizontal(|ui| {
            crate::icons::image("triangle-alert", 13.0, t.warning).paint_at(ui, ERect::from_min_size(ui.cursor().min + vec2(0.0, 3.0), vec2(13.0, 13.0)));
            ui.add_space(18.0);
            ui.label(egui::RichText::new(l).color(t.warning).size(11.5));
        });
    }
}

fn settings_panel(ui: &mut egui::Ui, d: &mut GangSheetDialog, rect: ERect, t: &Tokens) {
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 14.0))));
    panel.spacing_mut().item_spacing.y = 6.0;
    let mut s = d.settings;
    egui::ScrollArea::vertical().id_salt("gang-settings").auto_shrink([false, false]).show(&mut panel, |ui| {
        switch_row(ui, tl!("Show Resolution Lines"), &mut s.show_res);
        switch_row(ui, tl!("Overlap Lines"), &mut s.show_overlap);
        switch_row(ui, tl!("Snap While Moving"), &mut s.snap);
        switch_row(ui, tl!("Background Warning:"), &mut s.warn_background);
        switch_row(ui, tl!("Transparency Warning:"), &mut s.warn_semi);
        switch_row(ui, tl!("Grid (1 cm)"), &mut s.grid);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tl!("Sheet Background")).color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let custom = Color32::from_rgb(s.custom[0], s.custom[1], s.custom[2]);
                let r = widgets::color_edit_button_srgb(ui, &mut s.custom).on_hover_text(tl!("Custom Color"));
                if r.changed() || r.clicked() {
                    s.backdrop = Backdrop::Color(Color32::from_rgb(s.custom[0], s.custom[1], s.custom[2]));
                }
                let _ = custom;
                for (bd, tip) in [
                    (Backdrop::Color(Color32::WHITE), tl!("White")),
                    (Backdrop::Color(Color32::BLACK), tl!("Black")),
                    (Backdrop::Color(Color32::from_gray(128)), tl!("Gray")),
                    (Backdrop::Checker, tl!("Transparent")),
                ] {
                    let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                    match bd {
                        Backdrop::Checker => widgets::checker(ui.painter(), r, 5.5),
                        Backdrop::Color(c) => {
                            ui.painter().rect_filled(r, 3.0, c);
                        }
                    }
                    let sel = s.backdrop == bd;
                    ui.painter().rect_stroke(r, 3.0, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { t.text } else { t.field_border }), StrokeKind::Outside);
                    if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        s.backdrop = bd;
                    }
                }
            });
        });
        widgets::hairline(ui);
        switch_row(ui, tl!("Sheet Margin"), &mut s.edge_on);
        if s.edge_on {
            number_row(ui, tl!("Along the edges"), &mut s.edge_cm, 0.0..=10.0, "cm", 2);
        }
        switch_row(ui, tl!("Image Margin"), &mut s.gap_on);
        if s.gap_on {
            number_row(ui, tl!("Between images"), &mut s.gap_cm, 0.0..=10.0, "cm", 2);
        }
        widgets::hairline(ui);
        number_row(ui, tl!("Film Width"), &mut s.width_cm, 10.0..=300.0, "cm", 1);
        number_row(ui, tl!("Max Length"), &mut s.max_length_cm, 10.0..=MAX_LENGTH_CM, "cm", 1);
        switch_row(ui, tl!("Auto Build May Rotate"), &mut s.rotate);
    });
    d.set_settings(s);
}

fn toolbar_bar(ui: &mut egui::Ui, d: &mut GangSheetDialog, rect: ERect, t: &Tokens) {
    let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(10.0, 6.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    bar.spacing_mut().item_spacing.x = 2.0;
    // Film size.
    let mut s = d.settings;
    let mut width = s.width_cm;
    let labels: Vec<(f32, String)> = {
        let mut v: Vec<f32> = FILM_PRESETS.to_vec();
        if !v.iter().any(|p| (p - width).abs() < 0.05) {
            v.push(width);
        }
        v.iter().map(|w| (*w, format!("{} cm x {} cm", widgets::fmt_num(f64::from(*w)), widgets::fmt_num(f64::from(s.max_length_cm))))).collect()
    };
    let opts: Vec<(f32, &str)> = labels.iter().map(|(w, l)| (*w, l.as_str())).collect();
    if widgets::dropdown(&mut bar, "gang-film", &mut width, &opts, 140.0) {
        s.width_cm = width;
    }
    bar.add_space(8.0);
    widgets::vline(&mut bar, 22.0);
    if tool_button(&mut bar, "undo-2", tl!("Undo (Ctrl+Z)"), !d.undo.is_empty(), 0.0, false).clicked() {
        d.undo();
    }
    if tool_button(&mut bar, "redo-2", tl!("Redo (Ctrl+Y)"), !d.redo.is_empty(), 0.0, false).clicked() {
        d.redo();
    }
    widgets::vline(&mut bar, 22.0);
    if tool_button(&mut bar, "grid-2x2", tl!("Grid (1 cm)"), true, 0.0, false).clicked() {
        s.grid = !s.grid;
    }
    // A narrow window leaves the spacing to Settings and Auto Build, so every tool keeps its room.
    let narrow = rect.width() < 1160.0;
    if !narrow {
        widgets::vline(&mut bar, 22.0);
        widgets::checkbox(&mut bar, &mut s.gap_on, tl!("Image Margin"));
        bar.add_space(4.0);
        bar.add_enabled_ui(s.gap_on, |ui| widgets::typed_value_field(ui, &mut s.gap_cm, 0.0..=10.0, "cm", 64.0, 2));
    }
    d.set_settings(s);
    widgets::vline(&mut bar, 22.0);
    let list = d.sel();
    let on = !list.is_empty();
    let single = d.single();
    let q = std::f32::consts::FRAC_PI_2;
    type Op = (&'static str, &'static str, f32, bool, u8);
    let ops: [Op; 16] = [
        ("rotate-cw", tl!("Rotate 90° Counterclockwise"), 0.0, true, 0),
        ("rotate-cw", tl!("Rotate 90° Clockwise"), 0.0, false, 1),
        ("flip-h", tl!("Flip Horizontally"), 0.0, false, 2),
        ("flip-v", tl!("Flip Vertically"), 0.0, false, 3),
        ("stretch-h", tl!("Stretch to the Film Width"), 0.0, false, 4),
        ("stretch-v", tl!("Stretch to the Sheet Length"), 0.0, false, 5),
        ("align-center", tl!("Center Horizontally"), 0.0, false, 6),
        ("align-center", tl!("Center Vertically"), q, false, 7),
        ("scan", tl!("Center on the Sheet"), 0.0, false, 8),
        ("align-left", tl!("Push Left"), 0.0, false, 9),
        ("align-right", tl!("Push Right"), 0.0, false, 10),
        ("align-left", tl!("Push Up"), q, false, 11),
        ("align-right", tl!("Push Down"), q, false, 12),
        ("link", tl!("Group (Ctrl+G)"), 0.0, false, 14),
        ("unlink", tl!("Ungroup (Ctrl+Shift+G)"), 0.0, false, 15),
        ("trash", tl!("Delete (Del)"), 0.0, false, 13),
    ];
    let mut op = None;
    for (icon, tip, angle, mirror, id) in ops {
        // Stretching resizes the visual's source (all its copies): one visual alone only.
        let enabled = match id {
            4 | 5 => single.is_some(),
            14 => list.len() > 1 && !d.is_one_group(&list),
            15 => d.any_grouped(&list),
            _ => on,
        };
        if tool_button(&mut bar, icon, tip, enabled, angle, mirror).clicked() {
            op = Some(id);
        }
        if matches!(id, 3 | 5 | 8 | 12 | 15) {
            widgets::vline(&mut bar, 22.0);
        }
    }
    if let Some(op) = op
        && on
    {
        d.checkpoint(None);
        let ok = match op {
            0 => d.rotate_block(&list, -1),
            1 => d.rotate_block(&list, 1),
            2 => {
                d.mirror_block(&list, true);
                true
            }
            3 => {
                d.mirror_block(&list, false);
                true
            }
            4 | 5 => {
                if let Some(i) = single {
                    d.stretch(i, op == 4);
                }
                true
            }
            6 => d.center_block(&list, true, false),
            7 => d.center_block(&list, false, true),
            8 => d.center_block(&list, true, true),
            9 => {
                d.align_block(&list, Side::Left);
                true
            }
            10 => {
                d.align_block(&list, Side::Right);
                true
            }
            11 => {
                d.align_block(&list, Side::Top);
                true
            }
            12 => {
                d.align_block(&list, Side::Bottom);
                true
            }
            14 => d.group(),
            15 => d.ungroup(),
            _ => {
                d.delete_block(&list);
                true
            }
        };
        if !ok {
            if op >= 14 {
                d.undo.pop();
            } else {
                d.status = if op <= 1 { tl!("No room to turn this visual") } else { tl!("Too close to another visual") }.to_string();
            }
        }
    }
    bar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let label = tl!("Auto Build");
        let (r, resp) = ui.allocate_exact_size(vec2(widgets::button_width(ui, &[label]).max(if narrow { 0.0 } else { 150.0 }), 28.0), Sense::click());
        let fill = if d.auto_view { t.primary_bg } else { BLUE };
        ui.painter().rect_filled(r, 6.0, if resp.hovered() { fill.gamma_multiply(0.9) } else { fill });
        ui.painter().text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.5), Color32::WHITE);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            d.auto_view = !d.auto_view;
        }
    });
}

/// Auto Build: every visual's size and copies, then Apply.
fn auto_view(ui: &mut egui::Ui, d: &mut GangSheetDialog, rect: ERect, t: &Tokens, act: &mut Actions, has_doc: bool) {
    ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(20.0, 14.0))));
    panel.spacing_mut().item_spacing.y = 10.0;
    panel.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Auto Build — every image at once")).font(crate::theme::semibold(15.0)).color(t.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if tool_button(ui, "x", tl!("Close"), true, 0.0, false).clicked() {
                d.auto_view = false;
            }
        });
    });
    let mut s = d.settings;
    panel.horizontal(|ui| {
        widgets::checkbox(ui, &mut s.edge_on, tl!("With the sheet margin"));
        ui.add_enabled_ui(s.edge_on, |ui| widgets::typed_value_field(ui, &mut s.edge_cm, 0.0..=10.0, "cm", 70.0, 2));
        ui.add_space(16.0);
        widgets::checkbox(ui, &mut s.gap_on, tl!("With the image margin"));
        ui.add_enabled_ui(s.gap_on, |ui| widgets::typed_value_field(ui, &mut s.gap_cm, 0.0..=10.0, "cm", 70.0, 2));
        ui.add_space(16.0);
        widgets::checkbox(ui, &mut s.rotate, tl!("Allow Rotation"));
    });
    d.set_settings(s);
    if d.sources.is_empty() {
        panel.add_space(30.0);
        panel.vertical_centered(|ui| {
            ui.label(egui::RichText::new(tl!("No image yet: import PNG files or add the open document.")).color(t.text_faint));
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 340.0).max(0.0) / 2.0);
                if widgets::primary_button(ui, tl!("Import Images"), 160.0).clicked() {
                    act.add_files = true;
                }
                if ui.add_enabled_ui(has_doc, |ui| widgets::primary_button(ui, tl!("Add Open Document"), 170.0)).inner.clicked() {
                    act.add_document = true;
                }
            });
        });
        return;
    }
    let mut remove = None;
    let mut dup = None;
    egui::ScrollArea::vertical().id_salt("gang-auto").auto_shrink([false, false]).max_height(panel.available_height() - 50.0).show(&mut panel, |ui| {
        let (card_w, card_h) = (310.0, 178.0);
        let per_row = ((ui.available_width() + 12.0) / (card_w + 12.0)).floor().max(1.0) as usize;
        let n = d.sources.len();
        for row in 0..n.div_ceil(per_row) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for i in row * per_row..((row + 1) * per_row).min(n) {
                    let Some(src) = d.sources.get(i).cloned() else { continue };
                    let (card, _) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::hover());
                    ui.painter().rect(card, 8.0, Color32::WHITE, Stroke::new(1.0, t.card_border), StrokeKind::Inside);
                    let inner = card.shrink(10.0);
                    // Picture.
                    let img = ERect::from_min_size(inner.min, vec2(100.0, 116.0));
                    ui.painter().rect_filled(img, 4.0, t.canvas);
                    widgets::checker(ui.painter(), img.shrink(6.0), 5.0);
                    if let Some(tex) = &src.tex {
                        let k = ((img.width() - 12.0) / src.px.0.max(1) as f32).min((img.height() - 12.0) / src.px.1.max(1) as f32);
                        ui.painter().image(
                            tex.id(),
                            ERect::from_center_size(img.center(), vec2(src.px.0 as f32 * k, src.px.1 as f32 * k)),
                            ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    // Fields.
                    let fields = ERect::from_min_max(pos2(img.right() + 10.0, inner.top()), pos2(inner.right(), img.bottom()));
                    let mut f = ui.new_child(egui::UiBuilder::new().max_rect(fields).layout(egui::Layout::top_down(egui::Align::Min)));
                    f.spacing_mut().item_spacing.y = 4.0;
                    let (mut w, mut h) = (units_cm(src.w), units_cm(src.h));
                    f.horizontal(|ui| {
                        fixed_label(ui, egui::RichText::new(tl!("Width:")).color(t.text).size(11.5), 60.0, 22.0);
                        if widgets::typed_value_field(ui, &mut w, 0.1..=300.0, "cm", 110.0, 2).changed() {
                            d.checkpoint(Some(format!("w{i}")));
                            d.set_size(i, Some(cm_units(w)), None);
                        }
                    });
                    f.horizontal(|ui| {
                        fixed_label(ui, egui::RichText::new(tl!("Height:")).color(t.text).size(11.5), 60.0, 22.0);
                        if widgets::typed_value_field(ui, &mut h, 0.1..=300.0, "cm", 110.0, 2).changed() {
                            d.checkpoint(Some(format!("h{i}")));
                            d.set_size(i, None, Some(cm_units(h)));
                        }
                    });
                    let mut lock = src.lock;
                    if widgets::checkbox(&mut f, &mut lock, tl!("Lock Proportions")).changed()
                        && let Some(s) = d.sources.get_mut(i)
                    {
                        s.lock = lock;
                    }
                    let dpi = src.dpi();
                    let level = DpiLevel::of(dpi);
                    f.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(11.0, 11.0), Sense::hover());
                        ui.painter().rect_filled(r, 2.0, level_color(level));
                        ui.label(egui::RichText::new(level_label(level)).color(t.text).size(11.5));
                        ui.label(egui::RichText::new(format!("({})", ppi_text(dpi))).color(t.text_faint).size(11.5));
                    });
                    warnings_lines(&mut f, d, &src, t);
                    // Copies, duplicate, delete.
                    let foot = ERect::from_min_max(pos2(inner.left(), inner.bottom() - 30.0), inner.max);
                    ui.painter().line_segment([pos2(card.left(), foot.top() - 6.0), pos2(card.right(), foot.top() - 6.0)], Stroke::new(1.0, t.separator));
                    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(foot).layout(egui::Layout::left_to_right(egui::Align::Center)));
                    b.spacing_mut().item_spacing.x = 4.0;
                    let mut q = src.quantity as f32;
                    if tool_button(&mut b, "minus", tl!("One copy less"), src.quantity > 0, 0.0, false).clicked() {
                        q -= 1.0;
                    }
                    let typed = widgets::typed_value_field(&mut b, &mut q, 0.0..=999.0, "", 48.0, 0).changed();
                    if tool_button(&mut b, "plus", tl!("One copy more"), src.quantity < 999, 0.0, false).clicked() {
                        q += 1.0;
                    }
                    let q = q.round().clamp(0.0, 999.0) as usize;
                    if (typed || q != src.quantity)
                        && let Some(s) = d.sources.get_mut(i)
                    {
                        s.quantity = q;
                    }
                    let room = b.available_width() - 80.0 - 90.0 - 8.0;
                    b.add_space(room.max(0.0));
                    if widgets::primary_button(&mut b, tl!("Duplicate"), 80.0).on_hover_text(tl!("Same image, another size")).clicked() {
                        dup = Some(i);
                    }
                    let (r, resp) = b.allocate_exact_size(vec2(90.0, 26.0), Sense::click());
                    b.painter().rect_filled(r, 6.0, if resp.hovered() { t.danger.gamma_multiply(0.9) } else { t.danger });
                    b.painter().text(r.center(), Align2::CENTER_CENTER, tl!("Delete"), FontId::proportional(12.0), Color32::WHITE);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        remove = Some(i);
                    }
                }
            });
            ui.add_space(12.0);
        }
    });
    if let Some(i) = remove {
        d.remove_source(i);
    }
    if let Some(i) = dup
        && let Some(src) = d.sources.get(i).cloned()
    {
        d.checkpoint(None);
        d.sources.insert(i + 1, Source { quantity: 1, ..src });
        for doc in &mut d.sheets {
            for p in &mut doc.placed {
                if p.source > i {
                    p.source += 1;
                }
            }
        }
    }
    panel.vertical_centered(|ui| {
        let total: usize = d.sources.iter().map(|s| s.quantity).sum();
        if ui.add_enabled_ui(total > 0, |ui| widgets::primary_button(ui, tl!("Apply"), 140.0)).inner.clicked() {
            d.checkpoint(None);
            let left = d.auto_build();
            d.status = if left > 0 { crate::i18n::fmt(tl!("{n} not placed"), &[("n", &left.to_string())]) } else { String::new() };
            d.auto_view = false;
        }
    });
}

/// The active gang sheets.
fn sheets_panel(ui: &mut egui::Ui, d: &mut GangSheetDialog, rect: ERect, t: &Tokens) {
    ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
    ui.painter().line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(1.0, t.separator));
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 12.0))));
    panel.spacing_mut().item_spacing.y = 8.0;
    panel.label(egui::RichText::new(format!("({})   {}", d.sheets.len(), tl!("Active Gang Sheets"))).font(crate::theme::semibold(13.0)).color(t.text));
    let film = format!("{} cm x {} cm", widgets::fmt_num(f64::from(d.settings.width_cm)), widgets::fmt_num(f64::from(d.settings.max_length_cm)));
    let mut pick = None;
    let mut dup = None;
    let mut del = None;
    let mut rename_done = None;
    let many = d.sheets.len() > 1;
    egui::ScrollArea::vertical().id_salt("gang-sheets").max_height(panel.available_height() * 0.6).auto_shrink([false, true]).show(&mut panel, |ui| {
        for k in 0..d.sheets.len() {
            let active = k == d.active;
            let Some(doc) = d.sheets.get(k).cloned() else { continue };
            let frame = egui::Frame::NONE
                .fill(Color32::WHITE)
                .stroke(Stroke::new(if active { 1.5 } else { 1.0 }, if active { t.text } else { t.card_border }))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(10));
            let resp = frame
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.horizontal(|ui| {
                        crate::icons::image("house", 12.0, t.text).paint_at(ui, ERect::from_min_size(ui.cursor().min + vec2(0.0, 2.0), vec2(12.0, 12.0)));
                        ui.add_space(16.0);
                        ui.label(egui::RichText::new(&film).size(11.0).color(t.text));
                        if many {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if tool_button(ui, "x", tl!("Delete the Sheet"), true, 0.0, false).clicked() {
                                    del = Some(k);
                                }
                            });
                        }
                    });
                    ui.horizontal(|ui| match d.renaming.as_mut() {
                        Some((rk, text)) if *rk == k => {
                            let r = widgets::text_field(ui, egui::Id::new(("gang-rename", k)), text, "", ui.available_width() - 4.0, false);
                            r.request_focus();
                            if r.lost_focus() {
                                rename_done = Some((k, text.clone()));
                            }
                        }
                        _ => {
                            ui.label(egui::RichText::new(&doc.name).color(t.text));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if tool_button(ui, "pencil", tl!("Rename"), true, 0.0, false).clicked() {
                                    d.renaming = Some((k, doc.name.clone()));
                                }
                            });
                        }
                    });
                    ui.label(egui::RichText::new(crate::i18n::fmt(tl!("{n} images"), &[("n", &doc.placed.len().to_string())])).size(11.0).color(t.text_faint));
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tl!("Qty:")).color(t.text));
                        let mut q = doc.copies as f32;
                        if widgets::typed_value_field(ui, &mut q, 1.0..=999.0, "", 56.0, 0).on_hover_text(tl!("Copies of this sheet to print")).changed()
                            && let Some(s) = d.sheets.get_mut(k)
                        {
                            s.copies = q.round().clamp(1.0, 999.0) as usize;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(egui::Label::new(egui::RichText::new(tl!("Duplicate")).color(t.text)).sense(Sense::click()))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                dup = Some(k);
                            }
                        });
                    });
                })
                .response;
            if !active && ui.interact(resp.rect, ui.id().with(("gang-sheet-card", k)), Sense::click()).clicked() {
                pick = Some(k);
            }
        }
    });
    if let Some((k, name)) = rename_done {
        if let Some(s) = d.sheets.get_mut(k)
            && !name.trim().is_empty()
        {
            s.name = name.trim().to_string();
        }
        d.renaming = None;
    }
    if let Some(k) = pick {
        d.active = k;
        d.selected.clear();
    }
    if let Some(k) = dup {
        d.checkpoint(None);
        d.duplicate_sheet(k);
    }
    if let Some(k) = del {
        d.checkpoint(None);
        d.delete_sheet(k);
    }
    panel.add_space(8.0);
    let link = |ui: &mut egui::Ui, icon: &str, label: &str| -> bool {
        let r = ui
            .horizontal(|ui| {
                crate::icons::image(icon, 15.0, t.text).paint_at(ui, ERect::from_min_size(ui.cursor().min + vec2(0.0, 2.0), vec2(15.0, 15.0)));
                ui.add_space(22.0);
                ui.add(egui::Label::new(egui::RichText::new(label).color(t.text)).sense(Sense::click()))
            })
            .inner;
        r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    };
    if link(&mut panel, "file-plus", tl!("Add a New Sheet")) {
        d.checkpoint(None);
        d.new_sheet();
    }
    if link(&mut panel, "layers", tl!("Auto Build")) {
        d.auto_view = true;
    }
    if link(&mut panel, "undo-2", tl!("Start Over")) {
        d.checkpoint(None);
        d.start_over();
    }
    let items = d.items();
    let used = d.sheet().used_length(&items);
    panel.add_space(8.0);
    panel.label(egui::RichText::new(crate::i18n::fmt(tl!("Length used: {length} cm"), &[("length", &cm_text(used))])).color(t.text_dim));
}

fn keys(d: &mut GangSheetDialog, ctx: &egui::Context, act: &mut Actions) {
    use egui::{Key, Modifiers};
    let take = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
    // egui ignores an extra Shift when matching, so the Shift chords come first.
    if take(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || take(Modifiers::COMMAND, Key::Y) {
        d.redo();
        return;
    }
    if take(Modifiers::COMMAND, Key::Z) {
        d.undo();
        return;
    }
    if take(Modifiers::NONE, Key::Escape) {
        if d.auto_view {
            d.auto_view = false;
        } else if !d.sel().is_empty() {
            d.selected.clear();
        } else {
            act.cancel = true;
        }
        return;
    }
    if take(Modifiers::COMMAND, Key::A) {
        d.select_all();
        return;
    }
    if take(Modifiers::COMMAND, Key::V)
        && let Some(clip) = d.clipboard.clone()
    {
        d.checkpoint(None);
        d.paste(&clip);
        return;
    }
    let list = d.sel();
    if list.is_empty() {
        return;
    }
    if take(Modifiers::COMMAND | Modifiers::SHIFT, Key::G) {
        d.checkpoint(None);
        if !d.ungroup() {
            d.undo.pop();
        }
        return;
    }
    if take(Modifiers::COMMAND, Key::G) {
        d.checkpoint(None);
        if !d.group() {
            d.undo.pop();
        }
        return;
    }
    if take(Modifiers::COMMAND, Key::C) {
        d.clipboard = d.copy_block(&list);
        return;
    }
    if take(Modifiers::COMMAND, Key::D) {
        d.checkpoint(None);
        d.duplicate_block(&list);
        return;
    }
    let cut = take(Modifiers::COMMAND, Key::X);
    if cut || take(Modifiers::NONE, Key::Delete) || take(Modifiers::NONE, Key::Backspace) {
        if cut {
            d.clipboard = d.copy_block(&list);
        }
        d.checkpoint(None);
        d.delete_block(&list);
        return;
    }
    let (shift, l, r, u, dn) = ctx.input(|i| {
        (i.modifiers.shift, i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight), i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown))
    });
    let step = if shift { NUDGE_SHIFT } else { NUDGE };
    let (dx, dy) = (i64::from(r) - i64::from(l), i64::from(dn) - i64::from(u));
    if dx != 0 || dy != 0 {
        d.checkpoint(Some(format!("nudge{list:?}")));
        let b = d.bbox(&list);
        if !d.move_block(&list, b.x + dx * step, b.y + dy * step) {
            d.status = tl!("Too close to another visual").to_string();
        }
    }
}

fn confirm_dialog(app: &mut PhotocraftApp, ctx: &egui::Context, act: &mut Actions) {
    let Some(d) = app.gang_sheet.as_mut() else { return };
    if !d.confirm_export {
        return;
    }
    let n = d.all_conflicts();
    let mut answer = None;
    egui::Modal::new(egui::Id::new("gang-sheet-confirm")).show(ctx, |ui| {
        ui.set_max_width(400.0);
        ui.label(crate::i18n::fmt(tl!("{n} visuals are closer than the margin to another visual or to an edge. Export anyway?"), &[("n", &n.to_string())]));
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(role) = widgets::dialog_buttons(
                ui,
                &[
                    widgets::DialogButton::new(widgets::ButtonRole::Default, tl!("Export"), 90.0),
                    widgets::DialogButton::new(widgets::ButtonRole::Cancel, tl!("Cancel"), 90.0),
                ],
            ) {
                answer = Some(role == widgets::ButtonRole::Default);
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        answer = Some(false);
    }
    if let Some(yes) = answer {
        d.confirm_export = false;
        act.export = false;
        if yes {
            export(app);
        }
    }
}

/// The sheet: rulers, film, visuals with their lines, overlay card, dragging and zoom.
fn sheet_view(ui: &mut egui::Ui, d: &mut GangSheetDialog, view: ERect, t: &Tokens) {
    let sheet = d.sheet();
    let painter = ui.painter_at(view);
    painter.rect_filled(view, 0.0, Color32::WHITE);
    let area = ERect::from_min_max(view.min + vec2(RULER, RULER), view.max);
    let (sw, sl) = (sheet.width.max(1) as f32, sheet.max_length.max(1) as f32);
    // The overlay card sits at the top left: the sheet fits in the room beside it when there is.
    let beside = if area.width() > CARD_W + 320.0 { CARD_W + 20.0 } else { 0.0 };
    let fit = ((area.width() - beside - 40.0) / sw).min((area.height() - 40.0) / sl).max(1e-4);
    let resp = ui.interact(area, ui.id().with("gang-sheet-view"), Sense::click_and_drag());
    if resp.hovered() {
        let (zoom_delta, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        if (zoom_delta - 1.0).abs() > 1e-4 {
            let old = fit * d.zoom;
            d.zoom = (d.zoom * zoom_delta).clamp(1.0, 12.0);
            if let Some(p) = resp.hover_pos() {
                let k = fit * d.zoom / old;
                let origin = area.min + d.scroll;
                d.scroll = (p - (p - origin) * k) - area.min;
            }
        } else {
            d.scroll += scroll;
        }
    }
    let s = fit * d.zoom;
    let size = vec2(sw * s, sl * s);
    let clamp_axis = |off: f32, content: f32, avail: f32, lead: f32| {
        if content + lead + 40.0 <= avail { lead + (avail - lead - content) / 2.0 } else { off.clamp(avail - content - 20.0, 20.0 + lead) }
    };
    d.scroll = vec2(clamp_axis(d.scroll.x, size.x, area.width(), beside), clamp_axis(d.scroll.y, size.y, area.height(), 0.0));
    let origin = area.min + d.scroll;
    let to_screen = |x: i64, y: i64| pos2(origin.x + x as f32 * s, origin.y + y as f32 * s);
    let to_units = |p: Pos2| (((p.x - origin.x) / s).round() as i64, ((p.y - origin.y) / s).round() as i64);
    let film_r = ERect::from_min_size(origin, size);
    let clip = painter.with_clip_rect(area);
    match d.settings.backdrop {
        Backdrop::Checker => widgets::checker(&clip, film_r, 8.0),
        Backdrop::Color(c) => {
            clip.rect_filled(film_r, 0.0, c);
        }
    }
    clip.rect_stroke(film_r, 0.0, Stroke::new(1.0, t.field_border), StrokeKind::Outside);
    let px_per_cm = UNITS_PER_CM as f32 * s;
    if d.settings.grid && px_per_cm >= 4.0 {
        let line = |strong: bool| {
            Stroke::new(1.0, if strong { Color32::from_rgba_unmultiplied(29, 155, 240, 90) } else { Color32::from_rgba_unmultiplied(29, 155, 240, 40) })
        };
        for c in 1..(sheet.width / UNITS_PER_CM) {
            let x = origin.x + c as f32 * px_per_cm;
            clip.line_segment([pos2(x, film_r.top()), pos2(x, film_r.bottom())], line(c % 5 == 0));
        }
        for c in 1..(sheet.max_length / UNITS_PER_CM) {
            let y = origin.y + c as f32 * px_per_cm;
            clip.line_segment([pos2(film_r.left(), y), pos2(film_r.right(), y)], line(c % 5 == 0));
        }
    }
    let m = sheet.edge as f32 * s;
    if m >= 2.0 {
        let dash = Shape::dashed_line(
            &{
                let r = film_r.shrink(m);
                [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()]
            },
            Stroke::new(1.0, t.text_faint.gamma_multiply(0.6)),
            4.0,
            4.0,
        );
        clip.extend(dash);
    }
    let items = d.items();
    let used = sheet.used_length(&items);
    if used > 0 {
        let y = origin.y + used as f32 * s;
        clip.extend(Shape::dashed_line(&[pos2(film_r.left(), y), pos2(film_r.right(), y)], Stroke::new(1.5, BLUE), 6.0, 4.0));
        clip.text(pos2(film_r.right() - 4.0, y - 3.0), Align2::RIGHT_BOTTOM, format!("{} cm", cm_text(used)), FontId::proportional(11.0), BLUE);
    }
    // Pointer: pick (Shift or Ctrl adds), drag the selection, draw a rectangle from empty film.
    let hit = |p: Pos2| -> Option<usize> {
        let (ux, uy) = to_units(p);
        items.iter().rposition(|r| ux >= r.x && ux < r.x + r.w && uy >= r.y && uy < r.y + r.h)
    };
    d.hovered = resp.hover_pos().and_then(hit);
    let adding = ui.input(|i| i.modifiers.shift || i.modifiers.command);
    if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let at = ui.input(|i| i.pointer.press_origin()).unwrap_or(p);
        match hit(at) {
            Some(i) => {
                if !d.sel().contains(&i) {
                    if adding { d.toggle(i) } else { d.select_one(Some(i)) }
                }
                let list = d.sel();
                let b = d.bbox(&list);
                let (ux, uy) = to_units(at);
                d.checkpoint(None);
                let from = list
                    .iter()
                    .map(|i| {
                        let r = d.item(*i);
                        (*i, r.x, r.y)
                    })
                    .collect();
                d.drag = Some(Drag { grab: (ux - b.x, uy - b.y), bbox: b, from });
            }
            None => {
                let base = if adding { d.sel() } else { Vec::new() };
                d.selected = base.clone();
                d.marquee = Some(Marquee { from: to_units(at), to: to_units(p), base });
            }
        }
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        match (hit(p), adding) {
            (Some(i), true) => d.toggle(i),
            (Some(i), false) => d.select_one(Some(i)),
            (None, true) => {}
            (None, false) => d.selected.clear(),
        }
    }
    // A right-click keeps the selection it lands in, so the menu acts on all of it.
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        match hit(p) {
            Some(i) if d.sel().contains(&i) => {}
            other => d.select_one(other),
        }
    }
    if let Some(dr) = d.drag.clone()
        && let Some(p) = resp.interact_pointer_pos()
        && resp.dragged()
    {
        let (ux, uy) = to_units(p);
        let mmr = |v: i64| (v as f64 / 10.0).round() as i64 * 10;
        let list: Vec<usize> = dr.from.iter().map(|f| f.0).collect();
        let mut b = sheet.clamp(Item { x: mmr(ux - dr.grab.0), y: mmr(uy - dr.grab.1), ..dr.bbox });
        if d.settings.snap {
            // Snapped as the box it moves as, against the visuals left where they are.
            let (others, gaps) = d.others(&list);
            b = sheet.clamp(sheet.snap(&others, &gaps, usize::MAX, b, d.block_gap(&list), (6.0 / s).round() as i64));
        }
        let (dx, dy) = (b.x - dr.bbox.x, b.y - dr.bbox.y);
        if let Some(placed) = d.placed_mut() {
            for (i, x, y) in &dr.from {
                if let Some(pl) = placed.get_mut(*i) {
                    (pl.x, pl.y) = (x + dx, y + dy);
                }
            }
        }
    }
    if resp.drag_stopped()
        && let Some(dr) = d.drag.take()
    {
        let list: Vec<usize> = dr.from.iter().map(|f| f.0).collect();
        let rects: Vec<Item> = list.iter().map(|i| d.item(*i)).collect();
        let moved = dr.from.iter().zip(&rects).any(|(f, r)| (r.x, r.y) != (f.1, f.2));
        if d.block_collides(&list, &rects) {
            // Dropped too close to another visual: back where it was.
            if let Some(placed) = d.placed_mut() {
                for (i, x, y) in &dr.from {
                    if let Some(pl) = placed.get_mut(*i) {
                        (pl.x, pl.y) = (*x, *y);
                    }
                }
            }
            d.status = tl!("Too close to another visual").to_string();
        } else if !moved {
            // A click without a move: nothing to undo.
            d.undo.pop();
        }
    }
    // The rectangle picks every visual it touches.
    if let Some(m) = d.marquee.as_mut()
        && let Some(p) = resp.interact_pointer_pos()
        && resp.dragged()
    {
        m.to = to_units(p);
    }
    if let Some(m) = d.marquee.clone() {
        let (x0, x1) = (m.from.0.min(m.to.0), m.from.0.max(m.to.0));
        let (y0, y1) = (m.from.1.min(m.to.1), m.from.1.max(m.to.1));
        let touched: Vec<usize> =
            items.iter().enumerate().filter(|(_, r)| r.x <= x1 && x0 <= r.x + r.w && r.y <= y1 && y0 <= r.y + r.h).map(|(i, _)| i).collect();
        let mut all = m.base.clone();
        all.extend(touched);
        d.selected = d.whole(&all);
        if resp.drag_stopped() || !resp.is_pointer_button_down_on() {
            d.marquee = None;
        }
    }
    // Right-click menu.
    resp.context_menu(|ui| {
        let list = d.sel();
        let on = !list.is_empty();
        let item = |ui: &mut egui::Ui, label: &str, on: bool| ui.add_enabled(on, egui::Button::new(label)).clicked();
        if item(ui, tl!("Copy"), on) {
            d.clipboard = d.copy_block(&list);
            ui.close();
        }
        if item(ui, tl!("Paste"), d.clipboard.is_some())
            && let Some(clip) = d.clipboard.clone()
        {
            d.checkpoint(None);
            d.paste(&clip);
            ui.close();
        }
        if item(ui, tl!("Delete"), on) {
            d.checkpoint(None);
            d.delete_block(&list);
            ui.close();
        }
        if item(ui, tl!("Duplicate"), on) {
            d.checkpoint(None);
            d.duplicate_block(&list);
            ui.close();
        }
        ui.separator();
        if item(ui, tl!("Group the Selection"), list.len() > 1 && !d.is_one_group(&list)) {
            d.checkpoint(None);
            d.group();
            ui.close();
        }
        if item(ui, tl!("Ungroup"), d.any_grouped(&list)) {
            d.checkpoint(None);
            d.ungroup();
            ui.close();
        }
        if item(ui, tl!("Select All"), !d.placed().is_empty()) {
            d.select_all();
            ui.close();
        }
        ui.separator();
        if item(ui, tl!("Auto Fill"), d.single().is_some())
            && let Some(src) = d.single().and_then(|i| d.placed().get(i)).map(|p| p.source)
        {
            d.checkpoint(None);
            d.fill(src);
            ui.close();
        }
        if item(ui, tl!("Rotate 90° Clockwise"), on) {
            d.checkpoint(None);
            if !d.rotate_block(&list, 1) {
                d.status = tl!("No room to turn this visual").to_string();
            }
            ui.close();
        }
    });
    // Visuals (positions re-read: the drag may have moved some).
    let items = d.items();
    let overlaps = sheet.overlaps(&items);
    let conflicts = sheet.conflicts(&items, &d.gaps());
    let list = d.sel();
    for (i, (r, p)) in items.iter().zip(d.placed()).enumerate() {
        let rect = ERect::from_min_max(to_screen(r.x, r.y), to_screen(r.x + r.w, r.y + r.h));
        if !rect.intersects(area) {
            continue;
        }
        let Some(src) = d.sources.get(p.source) else { continue };
        let close = conflicts.get(i).copied().unwrap_or(false);
        if let Some(tex) = &src.tex {
            clip.add(image_shape(tex.id(), rect, p.orientation));
        }
        if d.settings.show_overlap && (close || overlaps.get(i).copied().unwrap_or(false)) {
            clip.rect_filled(rect, 0.0, BLUE.gamma_multiply(0.15));
            clip.rect_stroke(rect, 0.0, Stroke::new(2.0, BLUE), StrokeKind::Inside);
        } else if d.settings.show_res {
            clip.rect_stroke(rect, 0.0, Stroke::new(1.5, level_color(DpiLevel::of(src.dpi()))), StrokeKind::Inside);
        }
        let warn = (d.settings.warn_background && src.background) || (d.settings.warn_semi && src.semi > cmd::SEMI_TRANSPARENT_WARNING);
        if warn && rect.width() > 18.0 {
            let b = ERect::from_min_size(pos2(rect.right() - 17.0, rect.top() + 3.0), vec2(14.0, 14.0));
            clip.circle_filled(b.center(), 8.0, Color32::WHITE);
            crate::icons::image("triangle-alert", 13.0, t.warning).paint_at(ui, b);
        }
        if list.len() > 1 && list.contains(&i) {
            clip.rect_stroke(rect, 0.0, Stroke::new(1.0, t.text), StrokeKind::Outside);
        }
    }
    // Every group in a dashed frame.
    let mut ids: Vec<u32> = d.placed().iter().map(|p| p.group).filter(|g| *g != 0).collect();
    ids.sort_unstable();
    ids.dedup();
    let frame_of = |b: Item| ERect::from_min_max(to_screen(b.x, b.y), to_screen(b.x + b.w, b.y + b.h)).expand(3.0);
    for g in ids {
        let members: Vec<usize> = (0..d.placed().len()).filter(|i| d.placed().get(*i).is_some_and(|p| p.group == g)).collect();
        let r = frame_of(d.bbox(&members));
        clip.extend(Shape::dashed_line(
            &[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()],
            Stroke::new(1.0, t.text_dim),
            5.0,
            3.0,
        ));
    }
    // The selection's box and its corners.
    if !list.is_empty() {
        let b = d.bbox(&list);
        let rect = if list.len() > 1 { frame_of(b) } else { ERect::from_min_max(to_screen(b.x, b.y), to_screen(b.x + b.w, b.y + b.h)) };
        clip.rect_stroke(rect, 0.0, Stroke::new(1.5, t.text), StrokeKind::Outside);
        for c in [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()] {
            clip.circle_filled(c, 4.0, t.text);
        }
    }
    if let Some(m) = &d.marquee {
        let r = ERect::from_two_pos(to_screen(m.from.0, m.from.1), to_screen(m.to.0, m.to.1));
        clip.rect_filled(r, 0.0, BLUE.gamma_multiply(0.08));
        clip.rect_stroke(r, 0.0, Stroke::new(1.0, BLUE), StrokeKind::Inside);
    }
    // Rulers in centimetres.
    let top = ERect::from_min_max(pos2(area.left(), view.top()), pos2(view.right(), area.top()));
    let side = ERect::from_min_max(pos2(view.left(), area.top()), pos2(area.left(), view.bottom()));
    for r in [top, side, ERect::from_min_max(view.min, area.min)] {
        painter.rect_filled(r, 0.0, Color32::WHITE);
    }
    painter.line_segment([top.left_bottom(), top.right_bottom()], Stroke::new(1.0, t.separator));
    painter.line_segment([side.right_top(), side.right_bottom()], Stroke::new(1.0, t.separator));
    let label_every = [1, 2, 5, 10, 15, 20, 50].into_iter().find(|n| *n as f32 * px_per_cm >= 34.0).unwrap_or(100);
    let tick_every = if px_per_cm >= 6.0 { 1 } else { label_every };
    let top_p = painter.with_clip_rect(top);
    let side_p = painter.with_clip_rect(side);
    for c in (0..=(sheet.width / UNITS_PER_CM)).step_by(tick_every as usize) {
        let x = origin.x + c as f32 * px_per_cm;
        let long = c % label_every == 0;
        top_p.line_segment([pos2(x, top.bottom()), pos2(x, top.bottom() - if long { 8.0 } else { 4.0 })], Stroke::new(1.0, t.text_faint));
        if long {
            top_p.text(pos2(x + 2.0, top.top() + 1.0), Align2::LEFT_TOP, c.to_string(), FontId::proportional(9.0), t.text_dim);
        }
    }
    for c in (0..=(sheet.max_length / UNITS_PER_CM)).step_by(tick_every as usize) {
        let y = origin.y + c as f32 * px_per_cm;
        let long = c % label_every == 0;
        side_p.line_segment([pos2(side.right(), y), pos2(side.right() - if long { 8.0 } else { 4.0 }, y)], Stroke::new(1.0, t.text_faint));
        if long {
            side_p.text(pos2(side.left() + 1.0, y + 2.0), Align2::LEFT_TOP, c.to_string(), FontId::proportional(9.0), t.text_dim);
        }
    }
    overlay_card(ui, d, area, t);
    zoom_controls(ui, d, area, t);
}

/// The card at the top left of the sheet: line switches, legend, and the hovered or selected
/// visual's figures.
fn overlay_card(ui: &mut egui::Ui, d: &mut GangSheetDialog, area: ERect, t: &Tokens) {
    let r = ERect::from_min_size(area.min + vec2(10.0, 10.0), vec2(250.0, 0.0));
    let mut card = ui.new_child(egui::UiBuilder::new().max_rect(ERect::from_min_size(r.min, vec2(250.0, area.height() - 20.0))));
    let mut s = d.settings;
    card_frame(t).show(&mut card, |ui| {
        ui.set_width(228.0);
        ui.spacing_mut().item_spacing.y = 3.0;
        widgets::checkbox(ui, &mut s.show_overlap, tl!("Show Overlap Lines"));
        ui.add_space(2.0);
        widgets::checkbox(ui, &mut s.show_res, tl!("Show Resolution Lines"));
        if s.show_res || s.show_overlap {
            ui.add_space(4.0);
            let mut legend: Vec<(Color32, &str)> = Vec::new();
            if s.show_res {
                legend.extend([
                    (level_color(DpiLevel::Optimal), tl!("Optimal resolution >= 300 dpi")),
                    (level_color(DpiLevel::Good), tl!("Good resolution >= 250 dpi")),
                    (level_color(DpiLevel::Low), tl!("Poor resolution >= 200 dpi")),
                    (level_color(DpiLevel::Poor), tl!("Very poor resolution < 200 dpi")),
                ]);
            }
            if s.show_overlap {
                legend.push((BLUE, tl!("Overlapping or too close")));
            }
            for (c, label) in legend {
                ui.horizontal(|ui| {
                    let (b, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().rect_filled(b, 1.0, c);
                    ui.label(egui::RichText::new(label).size(11.0).color(t.text));
                });
            }
        }
        let list = d.sel();
        let shown = d.hovered.or(d.single()).filter(|i| *i < d.placed().len());
        if shown.is_none() && list.len() > 1 {
            // Several selected: their box.
            let b = d.bbox(&list);
            widgets::hairline(ui);
            ui.label(egui::RichText::new(crate::i18n::fmt(tl!("{n} visuals selected"), &[("n", &list.len().to_string())])).size(11.0).color(t.text));
            egui::Grid::new("gang-info-multi").num_columns(2).spacing(vec2(24.0, 2.0)).show(ui, |ui| {
                for (k, v) in [
                    (tl!("Left"), format!("{} cm", cm_text(b.x))),
                    (tl!("Top"), format!("{} cm", cm_text(b.y))),
                    (tl!("Width"), format!("{} cm", cm_text(b.w))),
                    (tl!("Height"), format!("{} cm", cm_text(b.h))),
                ] {
                    ui.label(egui::RichText::new(k).size(11.0).color(t.text));
                    ui.label(egui::RichText::new(v).size(11.0).color(t.text));
                    ui.end_row();
                }
            });
        }
        if let Some(i) = shown
            && let Some(src) = d.placed().get(i).and_then(|p| d.sources.get(p.source)).cloned()
        {
            let it = d.item(i);
            widgets::hairline(ui);
            egui::Grid::new("gang-info").num_columns(2).spacing(vec2(24.0, 2.0)).show(ui, |ui| {
                for (k, v) in [
                    (tl!("Left"), format!("{} cm", cm_text(it.x))),
                    (tl!("Top"), format!("{} cm", cm_text(it.y))),
                    (tl!("Width"), format!("{} cm", cm_text(it.w))),
                    (tl!("Height"), format!("{} cm", cm_text(it.h))),
                    (tl!("Resolution"), ppi_text(src.dpi())),
                ] {
                    ui.label(egui::RichText::new(k).size(11.0).color(t.text));
                    ui.label(egui::RichText::new(v).size(11.0).color(t.text));
                    ui.end_row();
                }
            });
            warnings_lines(ui, d, &src, t);
        }
    });
    d.set_settings(s);
}

fn zoom_controls(ui: &mut egui::Ui, d: &mut GangSheetDialog, area: ERect, t: &Tokens) {
    let r = ERect::from_min_size(pos2(area.right() - 190.0, area.bottom() - 40.0), vec2(180.0, 30.0));
    let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::right_to_left(egui::Align::Center)));
    bar.spacing_mut().item_spacing.x = 4.0;
    if tool_button(&mut bar, "zoom-in", tl!("Zoom In"), d.zoom < 12.0, 0.0, false).clicked() {
        d.zoom = (d.zoom * 1.25).min(12.0);
    }
    bar.label(egui::RichText::new(format!("{:.0} %", d.zoom * 100.0)).size(11.5).color(t.text));
    if tool_button(&mut bar, "minus", tl!("Zoom Out"), d.zoom > 1.0, 0.0, false).clicked() {
        d.zoom = (d.zoom / 1.25).max(1.0);
    }
    if tool_button(&mut bar, "maximize-2", tl!("Fit on Screen"), true, 0.0, false).clicked() {
        d.zoom = 1.0;
        d.scroll = Vec2::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(dir: &std::path::Path, name: &str, w: u32, h: u32) -> String {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": w, "height": h, "resolution": 300, "background": "transparent"})).unwrap();
        s.execute("edit.fill", json!({"color": "#c03020"})).unwrap();
        let path = dir.join(name).to_string_lossy().into_owned();
        s.execute("file.saveACopy", json!({"path": path})).unwrap();
        path
    }

    fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, ui: Value) -> Value {
        menu(app, ctx, cmd::GANG_SHEET, &json!({"ui": ui})).unwrap().unwrap()
    }

    fn reset_settings() {
        if let Ok(mut s) = LAST_SETTINGS.lock() {
            *s = None;
        }
    }

    #[test]
    fn layout_margins_tools_history_and_export() {
        reset_settings();
        let dir = std::env::temp_dir().join(format!("gang-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 1181 px at 300 ppi: 10 cm.
        let a = png(&dir, "a.png", 1181, 591);
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        let r = menu(&mut app, &ctx, cmd::GANG_SHEET, &json!({})).unwrap().unwrap();
        assert_eq!((r["filmWidth"].as_f64(), r["edgeMargin"].as_f64(), r["spacing"].as_f64()), (Some(56.0), Some(1.0), Some(1.0)));
        let r = invoke(&mut app, &ctx, json!({"addFiles": [a]}));
        assert_eq!(r["sources"][0]["widthCm"], 10.0);
        assert_eq!(r["sources"][0]["level"], "optimal");
        assert_eq!(r["placements"][0]["x"], 1.0, "placed inside the 1 cm edge");
        // Four copies on the sheet; 1 cm apart.
        let r = invoke(&mut app, &ctx, json!({"source": {"index": 0, "onSheet": 4}}));
        assert_eq!(r["placements"].as_array().unwrap().len(), 4);
        assert_eq!(r["conflicts"], 0);
        // Unlocked: the height alone changes.
        let r = invoke(&mut app, &ctx, json!({"source": {"index": 0, "lock": false, "heightCm": 8}}));
        assert_eq!((r["sources"][0]["widthCm"].as_f64(), r["sources"][0]["heightCm"].as_f64()), (Some(10.0), Some(8.0)));
        assert_eq!(r["sources"][0]["level"], "poor", "591 px over 8 cm: under 200 ppi");
        // A move onto another visual is refused.
        let first = r["placements"][0].clone();
        let bad = json!({"move": {"index": 1, "x": first["x"], "y": first["y"]}});
        assert!(menu(&mut app, &ctx, cmd::GANG_SHEET, &json!({"ui": bad})).unwrap().is_err());
        // Tools on the selected visual.
        let r = invoke(&mut app, &ctx, json!({"select": 0, "rotate": 1}));
        assert_eq!(r["placements"][0]["rotation"], 90);
        let r = invoke(&mut app, &ctx, json!({"mirror": "h"}));
        assert_eq!(r["placements"][0]["flipH"], true);
        let r = invoke(&mut app, &ctx, json!({"align": "left"}));
        assert_eq!(r["placements"][0]["x"], 1.0);
        let r = invoke(&mut app, &ctx, json!({"copy": true, "paste": true}));
        assert_eq!(r["placements"].as_array().unwrap().len(), 5);
        let r = invoke(&mut app, &ctx, json!({"delete": true}));
        assert_eq!(r["placements"].as_array().unwrap().len(), 4);
        // Undo and redo.
        let r = invoke(&mut app, &ctx, json!({"undo": true}));
        assert_eq!(r["placements"].as_array().unwrap().len(), 5);
        let r = invoke(&mut app, &ctx, json!({"redo": true}));
        assert_eq!(r["placements"].as_array().unwrap().len(), 4);
        // Own spacing: 3 cm for this visual makes its neighbours too close.
        let r = invoke(&mut app, &ctx, json!({"source": {"index": 0, "marginMm": 30}}));
        assert!(r["conflicts"].as_u64().unwrap() > 0);
        let r = invoke(&mut app, &ctx, json!({"source": {"index": 0, "marginMm": null}}));
        assert_eq!(r["conflicts"], 0);
        // Auto Build: 120 copies of 10 × 5 cm overflow into new sheets (about 80 fit on one).
        let r = invoke(&mut app, &ctx, json!({"source": {"index": 0, "lock": true, "widthCm": 10, "quantity": 120}, "autoBuild": true}));
        let sheets = r["sheets"].as_array().unwrap();
        assert!(sheets.len() >= 2, "{r}");
        assert_eq!(sheets.iter().map(|s| s["images"].as_u64().unwrap()).sum::<u64>(), 120);
        // Sheets: copies, duplicate, delete, new.
        let r = invoke(&mut app, &ctx, json!({"sheet": 0, "sheetCopies": 3, "renameSheet": "Client A"}));
        assert_eq!(r["sheets"][0]["copies"], 3);
        let n = r["sheets"].as_array().unwrap().len();
        let r = invoke(&mut app, &ctx, json!({"duplicateSheet": 0}));
        assert_eq!(r["sheets"].as_array().unwrap().len(), n + 1);
        let r = invoke(&mut app, &ctx, json!({"deleteSheet": 1}));
        assert_eq!(r["sheets"].as_array().unwrap().len(), n);
        // Fill the room left on a new sheet.
        let r = invoke(&mut app, &ctx, json!({"newSheet": true, "fill": 0}));
        assert!(r["placements"].as_array().unwrap().len() >= 12, "{}", r["placements"].as_array().unwrap().len());
        assert_eq!(r["conflicts"], 0);
        // Export: one PNG per sheet with visuals, each opened.
        let docs = app.session.documents().len();
        let out = dir.join("sheet.png").to_string_lossy().into_owned();
        let r = invoke(&mut app, &ctx, json!({"export": out}));
        let written = r["sheets"].as_array().unwrap().len();
        assert_eq!(written, n + 1);
        assert!(app.gang_sheet.is_none(), "closed once exported");
        assert_eq!(app.session.documents().len(), docs + written);
        assert!(dir.join("sheet 1 x3.png").exists());
        assert_eq!(app.session.documents()[docs].doc.name, "Client A");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Several visuals picked at once, grouped and ungrouped; a group moves, turns, mirrors,
    /// copies and deletes as one block and stays one in Auto Build.
    #[test]
    fn selection_and_groups_work_as_one_block() {
        reset_settings();
        let dir = std::env::temp_dir().join(format!("gang-groups-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = png(&dir, "a.png", 1181, 591);
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        invoke(&mut app, &ctx, json!({"addFiles": [a], "set": {"rotate": false}}));
        let r = invoke(&mut app, &ctx, json!({"source": {"index": 0, "onSheet": 4}}));
        assert_eq!(r["placements"].as_array().unwrap().len(), 4);
        let at = |r: &Value, i: usize| (r["placements"][i]["x"].as_f64().unwrap(), r["placements"][i]["y"].as_f64().unwrap());
        let offset = |r: &Value| {
            let (a, b) = (at(r, 0), at(r, 1));
            ((b.0 - a.0) * 100.0).round() / 100.0 + ((b.1 - a.1) * 100.0).round() / 100.0 * 1000.0
        };
        // Grouping takes two at least.
        assert!(menu(&mut app, &ctx, cmd::GANG_SHEET, &json!({"ui": {"select": 0, "group": true}})).unwrap().is_err());
        let r = invoke(&mut app, &ctx, json!({"select": [0, 1], "group": true}));
        let g = r["placements"][0]["group"].as_u64().unwrap();
        assert!(g > 0 && r["placements"][1]["group"] == g && r["placements"][2]["group"] == 0);
        // One member picks the whole group.
        let r = invoke(&mut app, &ctx, json!({"select": 1}));
        assert_eq!(r["selected"], json!([0, 1]));
        let before = offset(&r);
        // Pushed down as one block: the two keep their places to each other.
        let r = invoke(&mut app, &ctx, json!({"align": "bottom"}));
        assert_eq!(offset(&r), before);
        assert!(at(&r, 0).1 > 30.0, "{r}");
        assert_eq!(r["conflicts"], 0);
        // Turned as a block: the box's sides swap, each visual turns too.
        let d = app.gang_sheet.as_ref().unwrap();
        let b = d.bbox(&[0, 1]);
        let r = invoke(&mut app, &ctx, json!({"rotate": 1}));
        let d = app.gang_sheet.as_ref().unwrap();
        let t = d.bbox(&[0, 1]);
        assert_eq!((t.w, t.h), (b.h, b.w));
        assert_eq!((r["placements"][0]["rotation"].as_u64(), r["placements"][1]["rotation"].as_u64()), (Some(90), Some(90)));
        assert_eq!(r["conflicts"], 0);
        // Mirrored as a block: the two swap ends, each one mirrored.
        let y0 = at(&r, 0).1;
        let r = invoke(&mut app, &ctx, json!({"mirror": "v"}));
        assert!(at(&r, 0).1 > y0 && r["placements"][0]["flipV"] == true);
        // Stretching resizes the source of every copy: not for a group.
        assert!(menu(&mut app, &ctx, cmd::GANG_SHEET, &json!({"ui": {"stretch": "h"}})).unwrap().is_err());
        // Copy and paste: two more, in a new group of their own.
        let r = invoke(&mut app, &ctx, json!({"copy": true, "paste": true}));
        let p = r["placements"].as_array().unwrap();
        assert_eq!(p.len(), 6);
        assert!(p[4]["group"] == p[5]["group"] && p[4]["group"] != g && p[4]["group"].as_u64() > Some(0));
        assert_eq!(r["selected"], json!([4, 5]));
        // Delete the pasted group, then ungroup the first.
        let r = invoke(&mut app, &ctx, json!({"delete": true}));
        assert_eq!(r["placements"].as_array().unwrap().len(), 4);
        let r = invoke(&mut app, &ctx, json!({"select": 0, "ungroup": true}));
        assert!(r["placements"].as_array().unwrap().iter().all(|p| p["group"] == 0));
        assert_eq!(r["selected"], json!([0, 1]), "still selected");
        invoke(&mut app, &ctx, json!({"undo": true}));
        let r = invoke(&mut app, &ctx, json!({"select": 0}));
        assert_eq!(r["selected"], json!([0, 1]), "undo brings the group back");
        let kept = offset(&r);
        // Select all and delete, then undo.
        let r = invoke(&mut app, &ctx, json!({"selectAll": true, "delete": true}));
        assert_eq!(r["placements"].as_array().unwrap().len(), 0);
        let r = invoke(&mut app, &ctx, json!({"undo": true}));
        assert_eq!(r["placements"].as_array().unwrap().len(), 4);
        // Auto Build: the group stays as it is, its two visuals among the five copies.
        let r = invoke(&mut app, &ctx, json!({"source": {"index": 0, "quantity": 5}, "autoBuild": true}));
        let p = r["placements"].as_array().unwrap();
        assert_eq!(p.len(), 5, "{r}");
        assert_eq!(p.iter().filter(|p| p["group"] != 0).count(), 2);
        assert_eq!(offset(&r), kept, "the group first, as it was");
        assert_eq!(r["conflicts"], 0);
        let _ = std::fs::remove_dir_all(dir);
        reset_settings();
    }

    /// Aligning on the selection's box and distributing evenly; a group moves as one block; an
    /// alignment that would bring two visuals too close moves nothing.
    #[test]
    fn the_selection_aligns_and_distributes() {
        reset_settings();
        let dir = std::env::temp_dir().join(format!("gang-align-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 5 × 2.5 cm.
        let a = png(&dir, "a.png", 591, 295);
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        invoke(&mut app, &ctx, json!({"addFiles": [a], "set": {"rotate": false}}));
        invoke(&mut app, &ctx, json!({"source": {"index": 0, "onSheet": 3}}));
        // Three in a staircase: x 1, 10, 30 cm; y 1, 6, 12 cm.
        for (i, (x, y)) in [(1.0, 1.0), (10.0, 6.0), (30.0, 12.0)].iter().enumerate() {
            invoke(&mut app, &ctx, json!({"move": {"index": i, "x": x, "y": y}}));
        }
        let xy = |r: &Value, i: usize| (r["placements"][i]["x"].as_f64().unwrap(), r["placements"][i]["y"].as_f64().unwrap());
        // Top edges: all at 1 cm, side by side.
        let r = invoke(&mut app, &ctx, json!({"selectAll": true, "alignSelection": "top"}));
        assert_eq!((xy(&r, 0).1, xy(&r, 1).1, xy(&r, 2).1), (1.0, 1.0, 1.0));
        // Distributed across: equal gaps, the first and last in place.
        let r = invoke(&mut app, &ctx, json!({"distribute": "h"}));
        let (x0, x1, x2) = (xy(&r, 0).0, xy(&r, 1).0, xy(&r, 2).0);
        assert_eq!((x0, x2), (1.0, 30.0));
        assert!(((x1 - x0) - (x2 - x1)).abs() < 0.02, "{x0} {x1} {x2}");
        // Left edges would stack them on each other: refused, nothing moves.
        assert!(menu(&mut app, &ctx, cmd::GANG_SHEET, &json!({"ui": {"alignSelection": "left"}})).unwrap().is_err());
        let r = invoke(&mut app, &ctx, json!({}));
        assert_eq!(xy(&r, 0).0, x0);
        assert_eq!(xy(&r, 1).0, x1);
        // A group is one block: 0 and 1 grouped, aligned on the bottom with 2 pushed lower.
        invoke(&mut app, &ctx, json!({"move": {"index": 2, "x": 30, "y": 20}}));
        invoke(&mut app, &ctx, json!({"select": [0, 1], "group": true}));
        let before = xy(&invoke(&mut app, &ctx, json!({})), 1).0 - xy(&invoke(&mut app, &ctx, json!({})), 0).0;
        let r = invoke(&mut app, &ctx, json!({"selectAll": true, "alignSelection": "bottom"}));
        assert_eq!((xy(&r, 0).1, xy(&r, 1).1, xy(&r, 2).1), (20.0, 20.0, 20.0));
        assert_eq!(xy(&r, 1).0 - xy(&r, 0).0, before, "the group keeps its layout");
        // Distributing takes three blocks: a group and one visual are only two.
        assert!(menu(&mut app, &ctx, cmd::GANG_SHEET, &json!({"ui": {"distribute": "h"}})).unwrap().is_err());
        let _ = std::fs::remove_dir_all(dir);
        reset_settings();
    }

    /// Shift+click adds to the selection, a rectangle from empty film picks what it touches, and
    /// dragging one selected visual moves them all.
    #[test]
    fn the_pointer_picks_several_and_drags_them_together() {
        use egui_kittest::Harness;
        reset_settings();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 1181, "height": 1181, "resolution": 300, "background": "transparent"})).unwrap();
        app.run("edit.fill", json!({"color": "#c03020"})).unwrap();
        let ctx = egui::Context::default();
        invoke(&mut app, &ctx, json!({"set": {"rotate": false}, "addDocument": true, "source": {"index": 0, "onSheet": 3}}));
        let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_ui_state(
            |ui, s: &mut (PhotocraftApp, bool)| {
                // Drawn once the app's fonts are in.
                if s.1 {
                    show(&mut s.0, ui.ctx());
                }
            },
            (app, false),
        );
        PhotocraftApp::setup_context(&h.ctx, Default::default());
        h.state_mut().1 = true;
        h.run_steps(3);
        // Where each visual is on screen: the film fits the canvas, so read it back from a frame.
        let screen = |h: &Harness<'_, (PhotocraftApp, bool)>, i: usize| -> Pos2 {
            let d = h.state().0.gang_sheet.as_ref().unwrap();
            let r = d.item(i);
            let view = ERect::from_min_max(pos2(RAIL_W + LEFT_W, HEADER_H + TOOLBAR_H), pos2(1600.0 - RIGHT_W, 1000.0));
            let area = ERect::from_min_max(view.min + vec2(RULER, RULER), view.max);
            let sheet = d.sheet();
            let beside = if area.width() > CARD_W + 320.0 { CARD_W + 20.0 } else { 0.0 };
            let fit = ((area.width() - beside - 40.0) / sheet.width as f32).min((area.height() - 40.0) / sheet.max_length as f32);
            let origin = area.min + d.scroll;
            pos2(origin.x + (r.x + r.w / 2) as f32 * fit, origin.y + (r.y + r.h / 2) as f32 * fit)
        };
        let click = |h: &mut Harness<'_, (PhotocraftApp, bool)>, at: Pos2, mods: egui::Modifiers| {
            h.event(egui::Event::ModifiersChanged(mods));
            h.event(egui::Event::PointerMoved(at));
            h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: mods });
            h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: mods });
            h.run_steps(2);
            h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        };
        let (p0, p1, p2) = (screen(&h, 0), screen(&h, 1), screen(&h, 2));
        click(&mut h, p0, egui::Modifiers::NONE);
        click(&mut h, p1, egui::Modifiers::SHIFT);
        assert_eq!(h.state().0.gang_sheet.as_ref().unwrap().sel(), vec![0, 1]);
        click(&mut h, p0, egui::Modifiers::SHIFT);
        assert_eq!(h.state().0.gang_sheet.as_ref().unwrap().sel(), vec![1], "Shift+click takes one out");
        // A rectangle from the empty film below the visuals up over the first two.
        let below = pos2(p0.x - 40.0, p0.y.max(p1.y).max(p2.y) + 300.0);
        let over = p1;
        h.drag_at(below);
        h.run();
        for k in 1..=6 {
            h.event(egui::Event::PointerMoved(below + (over - below) * (k as f32 / 6.0)));
            h.run();
        }
        h.drop_at(over);
        h.run_steps(2);
        let sel = h.state().0.gang_sheet.as_ref().unwrap().sel();
        assert!(sel.contains(&0) && sel.contains(&1) && !sel.contains(&2), "{sel:?}");
        // Dragging one of them moves both by the same amount; the third stays.
        let before: Vec<Item> = (0..3).map(|i| h.state().0.gang_sheet.as_ref().unwrap().item(i)).collect();
        let from = screen(&h, 0);
        h.drag_at(from);
        h.run();
        for k in 1..=6 {
            h.event(egui::Event::PointerMoved(from + vec2(0.0, 40.0 * k as f32)));
            h.run();
        }
        h.drop_at(from + vec2(0.0, 240.0));
        h.run_steps(2);
        let d = h.state().0.gang_sheet.as_ref().unwrap();
        let after: Vec<Item> = (0..3).map(|i| d.item(i)).collect();
        assert!(after[0].y > before[0].y, "{before:?} {after:?}");
        assert_eq!(after[0].y - before[0].y, after[1].y - before[1].y);
        assert_eq!(after[2], before[2]);
        reset_settings();
    }

    /// Screenshots for review (`GANG_VISUALS=<folder>`): a group selected beside another, in
    /// French, at two window widths.
    #[test]
    fn capture_gang_sheet_groups() {
        let Some(dir) = std::env::var_os("GANG_VISUALS").map(std::path::PathBuf::from) else { return };
        std::fs::create_dir_all(&dir).unwrap();
        let _fr = crate::i18n::language_scope(crate::i18n::Lang::from_pref("fr"));
        for width in [1366.0, 1600.0] {
            reset_settings();
            let mut h = egui_kittest::Harness::builder().with_size(vec2(width, 900.0)).with_max_steps(64).wgpu().build_eframe(move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
                crate::dtf_panel::dtf_craft_startup(&mut app);
                app.ui.dtf_panel.beta_notice = false;
                app.run("file.new", json!({"width": 900, "height": 600, "resolution": 300, "background": "transparent"})).unwrap();
                app.run("edit.fill", json!({"color": "#f2c200"})).unwrap();
                let ctx = cc.egui_ctx.clone();
                invoke(&mut app, &ctx, json!({"addDocument": true, "source": {"index": 0, "onSheet": 8}}));
                invoke(&mut app, &ctx, json!({"select": [2, 3], "group": true}));
                invoke(&mut app, &ctx, json!({"select": [0, 1], "group": true}));
                invoke(&mut app, &ctx, json!({"select": [0, 1, 2, 3, 4]}));
                app
            });
            h.state_mut().session.prefs.edit(|p| p.interface.language = "fr".into());
            h.run_steps(8);
            h.render().unwrap().save(dir.join(format!("gang_groupes_{width}.png"))).unwrap();
        }
        reset_settings();
    }

    #[test]
    fn open_document_settings_and_params() {
        reset_settings();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        app.run("file.new", json!({"width": 600, "height": 300, "resolution": 300})).unwrap();
        let r = invoke(&mut app, &ctx, json!({"addDocument": true}));
        assert_eq!(r["sources"][0]["pixels"], json!([600, 300]));
        assert_eq!(r["sources"][0]["background"], true, "a white document has its background");
        let r = invoke(&mut app, &ctx, json!({"set": {"edgeMargin": 0, "spacing": 0.5, "snap": true, "grid": true, "filmWidth": 30}}));
        assert_eq!((r["edgeMargin"].as_f64(), r["spacing"].as_f64(), r["filmWidth"].as_f64()), (Some(0.0), Some(0.5), Some(30.0)));
        let d = app.gang_sheet.as_ref().unwrap();
        let p = d.params(&app, 0).unwrap();
        assert_eq!(p["sources"][0]["document"], 0);
        assert_eq!(p["edgeMargin"], 0.0);
        assert_eq!(p["spacing"], 5.0);
        assert_eq!(p["placements"].as_array().unwrap().len(), 1);
        invoke(&mut app, &ctx, json!({"cancel": true}));
        assert!(app.gang_sheet.is_none());
        // Params other than `ui` run the command itself.
        assert!(menu(&mut app, &ctx, cmd::GANG_SHEET, &json!({"sources": [{"document": 0}]})).is_none());
        reset_settings();
    }

    #[test]
    fn sheet_paths() {
        assert_eq!(sheet_path("/a/Gang sheet.png", 0, 1, 1), "/a/Gang sheet.png");
        assert_eq!(sheet_path("/a/Gang sheet.png", 1, 3, 1), "/a/Gang sheet 2.png");
        assert_eq!(sheet_path("/a/x.png", 0, 1, 4), "/a/x x4.png");
    }

    #[test]
    fn image_orientation_uv() {
        // 90° clockwise: the top-left corner shows the source's bottom-left.
        let Shape::Mesh(m) =
            image_shape(egui::TextureId::default(), ERect::from_min_size(Pos2::ZERO, vec2(10.0, 10.0)), Orientation { quarter: 1, ..Default::default() })
        else {
            panic!("a mesh")
        };
        assert_eq!(m.vertices[0].uv, pos2(0.0, 1.0));
        let Shape::Mesh(m) =
            image_shape(egui::TextureId::default(), ERect::from_min_size(Pos2::ZERO, vec2(10.0, 10.0)), Orientation { flip_h: true, ..Default::default() })
        else {
            panic!("a mesh")
        };
        assert_eq!(m.vertices[0].uv, pos2(1.0, 0.0));
    }

    #[test]
    fn the_window_draws_light_and_gives_the_theme_back() {
        reset_settings();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, ThemeKind::ProMedium);
        app.run("file.new", json!({"width": 400, "height": 200, "resolution": 300})).unwrap();
        invoke(&mut app, &ctx, json!({"addDocument": true, "source": {"index": 0, "onSheet": 3}}));
        for tab in [Tab::Home, Tab::Settings] {
            if let Some(d) = app.gang_sheet.as_mut() {
                d.tab = tab;
                d.selected = vec![0];
            }
            ctx.run_ui(Default::default(), |ui| show(&mut app, ui.ctx())).textures_delta.clear();
        }
        if let Some(d) = app.gang_sheet.as_mut() {
            d.auto_view = true;
        }
        ctx.run_ui(Default::default(), |ui| show(&mut app, ui.ctx())).textures_delta.clear();
        assert!(app.gang_sheet.as_ref().is_some_and(|d| d.sources[0].tex.is_some()));
        assert_eq!(Tokens::get(&ctx).kind, ThemeKind::ProMedium, "the app keeps its theme");
        reset_settings();
    }
}
