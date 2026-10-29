// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Color Analyzer: a full-screen module over the main window, like Camera Raw. It reads the
//! visual's main colours (sRGB, transparency ignored), matches them with the swatch book the user
//! loads (.ase, .aco, .csv; ΔE 2000), shows colour patches beside the visual for checking on
//! screen, writes an A4 colour sheet (PDF) and exports a PNG with the colours replaced by the
//! swatch colours, opened in a new tab. The document is never changed.
//!
//! Color Range, as Photoshop's: a click picks a colour on the visual (Shift+click adds one,
//! Alt+click takes one out), the tolerance (0–100, 20 by default) and "Contiguous Pixels" set what
//! is taken, shown on the preview (the rest greyed); Apply replaces those pixels by a swatch colour
//! after the colours of the table. The ranges go into the PNG export and the PDF sheet.
//!
//! The analysis runs on a copy of the visual of at most [`PREVIEW_MAX`] px a side (every pixel up
//! to that size, a regular sample above); the PNG export replaces the colours at full size
//! (`dtf.colorReplace`).

use std::collections::HashMap;

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};
use photocraft_algo::dtf_color::{self, ColorRange, MainColor, Swatch};
use photocraft_color::convert::{lab_to_srgb, srgb_to_lab};
use photocraft_engine::dtf_color_cmds as cmds;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::color_sheet::{self, ColorSheet, SheetRef, SheetRow, decimal};
use crate::theme::Tokens;

/// The analysis copy's longest side, in px.
pub const PREVIEW_MAX: usize = 2000;
/// The colour sheet's visual, longest side in px.
const SHEET_IMAGE_MAX: usize = 800;
const PANEL_W: f32 = 420.0;
const SETTINGS_PREF: &str = "dtfCraft.colorAnalyzer";
const BOOK_PREF: &str = "dtfCraft.swatchBook";
/// The profile the RGB values are read in.
pub const PROFILE: &str = "sRGB IEC61966-2.1";

/// What the preview shows: the visual as it is, or with the swatch colours and their patches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Original,
    Replaced,
}

/// The module's settings, remembered with the preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub max_colors: u32,
    pub merge_delta_e: f32,
    /// Leave out the colours under 0.5 % of the visible pixels.
    pub ignore_small: bool,
    pub patches_below: bool,
    /// Patch side on screen, in points.
    pub patch_size: f32,
    pub show_reference: bool,
    pub show_hex: bool,
    pub show_rgb: bool,
    pub client: String,
    /// Color Range: tolerance (0–100) and contiguous pixels only.
    pub range_tolerance: f32,
    pub range_contiguous: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_colors: 8,
            merge_delta_e: 5.0,
            ignore_small: true,
            patches_below: false,
            patch_size: 46.0,
            show_reference: true,
            show_hex: true,
            show_rgb: false,
            client: String::new(),
            range_tolerance: 20.0,
            range_contiguous: false,
        }
    }
}

/// The loaded swatch book.
#[derive(Clone, Debug, PartialEq)]
pub struct Book {
    pub name: String,
    pub swatches: Vec<Swatch>,
}

/// A colour's own choices, kept across analyses by its hex.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Choice {
    /// The swatch picked with "Change…" (`None`: the nearest).
    pub(crate) swatch: Option<usize>,
    /// Replaced by its swatch in the PNG export (`None`: when the swatch is picked by hand or
    /// within ΔE 5 — a far swatch, often an antialiased edge, is left alone unless checked).
    pub(crate) replace: Option<bool>,
}

impl Choice {
    /// The swatch of `book` for the colour `lab` (picked, or the nearest) with its ΔE 2000, and
    /// whether the colour is replaced by it.
    pub(crate) fn resolve(&self, book: Option<&Book>, lab: [f32; 3]) -> (Option<(usize, f32)>, bool) {
        let swatch = book.and_then(|b| match self.swatch.filter(|s| *s < b.swatches.len()) {
            Some(s) => Some((s, dtf_color::delta_e2000(lab, b.swatches[s].lab))),
            None => dtf_color::nearest_swatch(&b.swatches, lab),
        });
        let replace = swatch.is_some_and(|(_, de)| self.replace.unwrap_or(self.swatch.is_some() || de <= AUTO_REPLACE_DE));
        (swatch, replace)
    }
}

/// Above this difference a nearest swatch is not replaced unless asked.
const AUTO_REPLACE_DE: f32 = 5.0;

/// One colour of the table.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub hex: String,
    pub rgb: [u8; 3],
    pub lab: [f32; 3],
    /// % of the visible pixels.
    pub share: f32,
    /// The swatch and its ΔE 2000.
    pub swatch: Option<(usize, f32)>,
    pub replace: bool,
    /// Index in the colours added with the eyedropper.
    pub extra: Option<usize>,
    /// Index in the colour ranges applied (a range's row: its first picked colour).
    pub range: Option<usize>,
}

/// A colour range applied: the pixels it takes, the swatch they become and their share of the
/// visible pixels (%).
#[derive(Clone, Debug, PartialEq)]
pub struct AppliedRange {
    pub range: ColorRange,
    pub swatch: usize,
    pub share: f32,
    /// Checked: replaced in the preview, the PNG export and on the sheet (unchecked: kept aside).
    pub on: bool,
}

pub struct ColorAnalyzer {
    pub doc: photocraft_doc::DocId,
    pub name: String,
    pub path: Option<String>,
    pub size: (u32, u32),
    pub dpi: f32,
    pub mode: String,
    /// The analysis copy: width, height, straight RGBA.
    pub w: usize,
    pub h: usize,
    px: Vec<[f32; 4]>,
    replaced: Option<Vec<[f32; 4]>>,
    pub colors: Vec<MainColor>,
    pub extra: Vec<[f32; 3]>,
    pub view: View,
    /// The Color Range tool is on (the eyedropper is then off), the range being built and the
    /// swatch it becomes (`None`: the nearest to its first colour).
    pub range_tool: bool,
    pub range: ColorRange,
    pub range_swatch: Option<usize>,
    pub ranges: Vec<AppliedRange>,
    /// The range being built changed since its overlay was drawn.
    range_rev: u64,
    range_overlay: Option<(u64, egui::TextureHandle, f32)>,
    /// "Change…" for the range being built.
    pub changing_range: bool,
    pub settings: Settings,
    pub book: Option<Book>,
    choices: HashMap<String, Choice>,
    /// The texture and what it shows: (replaced?, revision).
    tex: Option<(bool, u64, egui::TextureHandle)>,
    revision: u64,
    /// The row whose reference is being changed.
    pub changing: Option<usize>,
    search: String,
    /// The settings changed: analyse again (once the pointer is released).
    dirty: bool,
    /// The help sheet is open.
    help: bool,
}

fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn rgb8(rgb: [f32; 3]) -> [u8; 3] {
    rgb.map(to8)
}

fn c32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// A swatch's colour as it is drawn and exported: sRGB on 8-bit steps.
pub(crate) fn swatch8(s: &Swatch) -> [u8; 3] {
    rgb8(dtf_color::swatch_rgb(s))
}

pub(crate) fn lab8(rgb: [u8; 3]) -> [f32; 3] {
    srgb_to_lab(rgb.map(|v| f32::from(v) / 255.0))
}

/// An L*a*b* colour as 8-bit sRGB.
fn lab_rgb8(lab: [f32; 3]) -> [u8; 3] {
    rgb8(lab_to_srgb(lab).map(|v| v.clamp(0.0, 1.0)))
}

/// A nearest-neighbour copy of `px` (`w` × `h`) of at most `max` px a side: the exact colours.
pub fn downsample(w: usize, h: usize, px: &[[f32; 4]], max: usize) -> (usize, usize, Vec<[f32; 4]>) {
    let step = w.max(h).div_ceil(max.max(1)).max(1);
    if step == 1 {
        return (w, h, px.to_vec());
    }
    let (sw, sh) = (w.div_ceil(step), h.div_ceil(step));
    let mut out = Vec::with_capacity(sw * sh);
    for y in 0..sh {
        for x in 0..sw {
            out.push(px[(y * step).min(h - 1) * w + (x * step).min(w - 1)]);
        }
    }
    (sw, sh, out)
}

fn file_name(name: &str) -> String {
    std::path::Path::new(name).file_name().and_then(|n| n.to_str()).unwrap_or(name).to_string()
}

impl ColorAnalyzer {
    /// The module for the visual `px` (`size`, straight RGBA) of document `doc`.
    pub fn new(
        doc: photocraft_doc::DocId,
        name: &str,
        path: Option<String>,
        size: (u32, u32),
        dpi: f32,
        mode: String,
        px: &[[f32; 4]],
        settings: Settings,
        book: Option<Book>,
    ) -> Self {
        let (w, h, px) = downsample(size.0 as usize, size.1 as usize, px, PREVIEW_MAX);
        let mut a = Self {
            doc,
            name: name.to_string(),
            path,
            size,
            dpi,
            mode,
            w,
            h,
            px,
            replaced: None,
            colors: Vec::new(),
            extra: Vec::new(),
            view: View::Replaced,
            range_tool: false,
            range: ColorRange { tolerance: settings.range_tolerance, contiguous: settings.range_contiguous, ..Default::default() },
            range_swatch: None,
            ranges: Vec::new(),
            range_rev: 0,
            range_overlay: None,
            changing_range: false,
            settings,
            book,
            choices: HashMap::new(),
            tex: None,
            revision: 0,
            changing: None,
            search: String::new(),
            dirty: false,
            help: false,
        };
        a.analyze();
        a
    }

    /// Reads the main colours again (settings or colours added by hand changed).
    pub fn analyze(&mut self) {
        let min = if self.settings.ignore_small { 0.005 } else { 0.0 };
        self.colors = cmds::colors(&self.px, self.settings.max_colors as usize, self.settings.merge_delta_e, min, &self.extra);
        self.dirty = false;
        self.changed();
    }

    /// The replacement changed: the replaced preview is computed again when shown.
    fn changed(&mut self) {
        self.replaced = None;
        self.revision += 1;
    }

    /// The rows with a patch on screen: the colours replaced (checked, with a reference), or every
    /// colour when no swatch book is loaded.
    pub fn patch_rows(&self) -> Vec<Row> {
        let rows = self.rows();
        if self.book.is_none() {
            return rows;
        }
        let mut out: Vec<Row> = rows.into_iter().filter(|r| r.replace && r.swatch.is_some()).collect();
        // Then the colour ranges, each as its first colour and its swatch.
        for (i, r) in self.ranges.iter().enumerate().filter(|(_, r)| r.on) {
            let Some(((lab, _), sw)) = r.range.add.first().zip(self.swatch(r.swatch)) else { continue };
            let rgb = lab_rgb8(*lab);
            out.push(Row {
                hex: color_sheet::hex(rgb),
                rgb,
                lab: *lab,
                share: r.share,
                swatch: Some((r.swatch, dtf_color::delta_e2000(*lab, sw.lab))),
                replace: true,
                extra: None,
                range: Some(i),
            });
        }
        out
    }

    // ---------------------------------------------------------------- colour range

    /// The colour under the analysis copy's point (`x`, `y` fractions of its size), as L*a*b* on
    /// 8-bit steps.
    fn pick_lab(&self, x: f32, y: f32) -> Option<[f32; 3]> {
        let px = ((x * self.w as f32).floor().max(0.0) as usize).min(self.w.saturating_sub(1));
        let py = ((y * self.h as f32).floor().max(0.0) as usize).min(self.h.saturating_sub(1));
        self.pick(px, py).map(|rgb| lab8(rgb8(rgb)))
    }

    /// A click with Color Range at (`x`, `y`) (fractions of the visual's size): a new range from
    /// that colour, or (`add`) one more colour in it, or (`subtract`) one taken out.
    pub fn range_click(&mut self, x: f32, y: f32, add: bool, subtract: bool) -> bool {
        let Some(lab) = self.pick_lab(x, y) else { return false };
        if subtract {
            if self.range.add.is_empty() {
                return false;
            }
            self.range.subtract.push(lab);
        } else {
            if !add {
                self.range.add.clear();
                self.range.subtract.clear();
                self.range_swatch = None;
            }
            self.range.add.push((lab, [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)]));
        }
        self.range_rev += 1;
        true
    }

    /// The tolerance or "Contiguous Pixels" changed.
    pub fn set_range_options(&mut self, tolerance: f32, contiguous: bool) {
        let tolerance = tolerance.clamp(0.0, 100.0);
        if (tolerance, contiguous) != (self.range.tolerance, self.range.contiguous) {
            self.range.tolerance = tolerance;
            self.range.contiguous = contiguous;
            self.settings.range_tolerance = tolerance;
            self.settings.range_contiguous = contiguous;
            self.range_rev += 1;
        }
    }

    pub fn clear_range(&mut self) {
        self.range.add.clear();
        self.range.subtract.clear();
        self.range_swatch = None;
        self.range_rev += 1;
    }

    /// The swatch the range being built becomes: the one picked, else the nearest to its first
    /// colour.
    pub fn range_target(&self) -> Option<(usize, f32)> {
        let (lab, _) = self.range.add.first()?;
        let b = self.book.as_ref()?;
        match self.range_swatch.filter(|s| *s < b.swatches.len()) {
            Some(s) => Some((s, dtf_color::delta_e2000(*lab, b.swatches[s].lab))),
            None => dtf_color::nearest_swatch(&b.swatches, *lab),
        }
    }

    /// The share (%) of the visible pixels the range takes (partly taken pixels in part).
    fn range_share(&self, range: &ColorRange) -> f32 {
        let visible = self.px.iter().filter(|q| q[3] > 0.0).count().max(1) as f32;
        range.weights(&self.px, self.w, self.h).iter().sum::<f32>() / visible * 100.0
    }

    /// Apply: the range being built joins the ranges replaced, and a new one starts.
    pub fn apply_range(&mut self) -> bool {
        let Some((swatch, _)) = self.range_target() else { return false };
        let range = self.range.clone();
        let share = self.range_share(&range);
        self.ranges.push(AppliedRange { range, swatch, share, on: true });
        self.clear_range();
        self.changed();
        true
    }

    /// Checks or unchecks an applied range.
    pub fn set_range_on(&mut self, i: usize, on: bool) {
        if let Some(r) = self.ranges.get_mut(i)
            && r.on != on
        {
            r.on = on;
            self.changed();
        }
    }

    pub fn remove_range(&mut self, i: usize) {
        if i < self.ranges.len() {
            self.ranges.remove(i);
            self.changed();
        }
    }

    /// The overlay greying what the range being built leaves out, and the share it takes.
    fn range_overlay(&mut self, ctx: &egui::Context) -> Option<(egui::TextureHandle, f32)> {
        if self.range.add.is_empty() {
            return None;
        }
        if let Some((rev, tex, share)) = &self.range_overlay
            && *rev == self.range_rev
        {
            return Some((tex.clone(), *share));
        }
        let weights = self.range.weights(&self.px, self.w, self.h);
        let visible = self.px.iter().filter(|q| q[3] > 0.0).count().max(1) as f32;
        let share = weights.iter().sum::<f32>() / visible * 100.0;
        let rgba: Vec<u8> =
            self.px.iter().zip(&weights).flat_map(|(q, w)| if q[3] <= 0.0 { [0, 0, 0, 0] } else { [40, 40, 40, ((1.0 - w) * 200.0).round() as u8] }).collect();
        let image = egui::ColorImage::from_rgba_unmultiplied([self.w.max(1), self.h.max(1)], &rgba);
        let tex = ctx.load_texture("dtf-color-analyzer-range", image, egui::TextureOptions::LINEAR);
        self.range_overlay = Some((self.range_rev, tex.clone(), share));
        Some((tex, share))
    }

    pub fn rows(&self) -> Vec<Row> {
        let main = self.colors.len() - self.extra.len().min(self.colors.len());
        self.colors
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let hex = dtf_color::hex(c.rgb);
                let choice = self.choices.get(&hex).copied().unwrap_or_default();
                let (swatch, replace) = choice.resolve(self.book.as_ref(), c.lab);
                Row {
                    hex,
                    rgb: rgb8(c.rgb),
                    lab: c.lab,
                    share: c.share * 100.0,
                    swatch,
                    replace,
                    extra: (i >= main).then(|| i - main),
                    range: None,
                }
            })
            .collect()
    }

    fn swatch(&self, i: usize) -> Option<&Swatch> {
        self.book.as_ref().and_then(|b| b.swatches.get(i))
    }

    /// Every colour with the colour it becomes (`None`: kept), as `dtf.colorReplace` takes them.
    pub fn replace_params(&self) -> Value {
        let colors: Vec<Value> = self
            .rows()
            .iter()
            .map(|r| {
                let to = r.swatch.filter(|_| r.replace).and_then(|(s, _)| self.swatch(s)).map(|s| color_sheet::hex(swatch8(s)));
                json!({"from": r.hex, "to": to})
            })
            .collect();
        let ranges: Vec<Value> = self
            .ranges
            .iter()
            .filter(|r| r.on)
            .filter_map(|r| {
                let to = color_sheet::hex(swatch8(self.swatch(r.swatch)?));
                let add: Vec<Value> = r.range.add.iter().map(|(lab, at)| json!({"hex": color_sheet::hex(lab_rgb8(*lab)), "x": at[0], "y": at[1]})).collect();
                let subtract: Vec<String> = r.range.subtract.iter().map(|lab| color_sheet::hex(lab_rgb8(*lab))).collect();
                Some(json!({"add": add, "subtract": subtract, "tolerance": r.range.tolerance, "contiguous": r.range.contiguous, "to": to}))
            })
            .collect();
        json!({"colors": colors, "ranges": ranges})
    }

    pub fn any_replaced(&self) -> bool {
        self.rows().iter().any(|r| r.replace) || self.ranges.iter().any(|r| r.on)
    }

    /// `px` (`w` × `h`) with the colours replaced as the PNG export replaces them.
    fn replace_in(&self, px: &[[f32; 4]], w: usize, h: usize) -> Vec<[f32; 4]> {
        let centers: Vec<([f32; 3], Option<[f32; 3]>)> =
            self.rows().iter().map(|r| (r.lab, r.swatch.filter(|_| r.replace).and_then(|(s, _)| self.swatch(s)).map(|s| lab8(swatch8(s))))).collect();
        let mut out = px.to_vec();
        dtf_color::replace(&mut out, &centers);
        let ranges: Vec<(ColorRange, [f32; 3])> =
            self.ranges.iter().filter(|r| r.on).filter_map(|r| self.swatch(r.swatch).map(|s| (r.range.clone(), lab8(swatch8(s))))).collect();
        dtf_color::replace_ranges(&mut out, px, w, h, &ranges);
        out
    }

    /// The analysis copy with the colours replaced (the PNG export's result, at preview size).
    pub fn replaced_pixels(&mut self) -> &[[f32; 4]] {
        if self.replaced.is_none() {
            self.replaced = Some(self.replace_in(&self.px, self.w, self.h));
        }
        self.replaced.as_deref().unwrap_or(&self.px)
    }

    /// The colour under the analysis copy's pixel (`None`: transparent or outside).
    pub fn pick(&self, x: usize, y: usize) -> Option<[f32; 3]> {
        let q = self.px.get(y.checked_mul(self.w)?.checked_add(x)?).filter(|_| x < self.w && y < self.h)?;
        (q[3] >= 0.5).then_some([q[0], q[1], q[2]])
    }

    /// Adds a colour picked with the eyedropper, unless it is one of the colours already (ΔE < 1).
    pub fn add_color(&mut self, rgb: [f32; 3]) -> bool {
        let rgb = rgb8(rgb).map(|v| f32::from(v) / 255.0);
        let lab = srgb_to_lab(rgb);
        if self.colors.iter().any(|c| dtf_color::delta_e2000(c.lab, lab) < 1.0) {
            return false;
        }
        self.extra.push(rgb);
        self.analyze();
        true
    }

    pub fn remove_extra(&mut self, i: usize) {
        if i < self.extra.len() {
            self.extra.remove(i);
            self.analyze();
        }
    }

    pub fn set_swatch(&mut self, hex: &str, swatch: Option<usize>) {
        self.choices.entry(hex.to_string()).or_default().swatch = swatch;
        self.changed();
    }

    pub fn set_replace(&mut self, hex: &str, on: bool) {
        self.choices.entry(hex.to_string()).or_default().replace = Some(on);
        self.changed();
    }

    pub fn set_book(&mut self, book: Option<Book>) {
        self.book = book;
        // The ranges' swatches belong to the previous book too.
        self.ranges.clear();
        self.range_swatch = None;
        // Picked references belong to the previous book.
        for c in self.choices.values_mut() {
            c.swatch = None;
        }
        self.changed();
    }

    /// The colour sheet of the analysis: the colours with a patch on screen.
    pub fn sheet(&self) -> ColorSheet {
        let (w, h, px) = downsample(self.w, self.h, &self.px, SHEET_IMAGE_MAX);
        let bytes = |px: &[[f32; 4]]| -> Vec<u8> { px.iter().flat_map(|q| [to8(q[0]), to8(q[1]), to8(q[2]), to8(q[3])]).collect() };
        let image = bytes(&px);
        // Beside it, the visual as the PNG export recolours it.
        let replaced = (self.book.is_some() && self.any_replaced()).then(|| (w, h, bytes(&self.replace_in(&px, w, h))));
        let rows = self
            .patch_rows()
            .iter()
            .map(|r| SheetRow {
                rgb: r.rgb,
                share: r.share,
                range: r.range.is_some(),
                reference: r.swatch.and_then(|(s, de)| self.swatch(s).map(|sw| SheetRef { name: sw.name.clone(), rgb: swatch8(sw), delta_e: de })),
            })
            .collect();
        ColorSheet {
            file_name: self.name.clone(),
            client: self.settings.client.clone(),
            date: color_sheet::today(),
            profile: PROFILE.into(),
            swatch_book: self.book.as_ref().map(|b| b.name.clone()),
            size_px: self.size,
            dpi: self.dpi,
            image: (w, h, image),
            replaced,
            replaced_label: None,
            rows,
        }
    }

    fn texture(&mut self, ctx: &egui::Context, replaced: bool) -> egui::TextureHandle {
        if let Some((r, rev, tex)) = &self.tex
            && *r == replaced
            && (*rev == self.revision || !replaced)
        {
            return tex.clone();
        }
        let (w, h) = (self.w, self.h);
        let px: Vec<u8> = if replaced { self.replaced_pixels().to_vec() } else { self.px.clone() }
            .iter()
            .flat_map(|q| [to8(q[0]), to8(q[1]), to8(q[2]), to8(q[3])])
            .collect();
        let image = egui::ColorImage::from_rgba_unmultiplied([w.max(1), h.max(1)], &px);
        let tex = ctx.load_texture("dtf-color-analyzer", image, egui::TextureOptions::LINEAR);
        self.tex = Some((replaced, self.revision, tex.clone()));
        tex
    }
}

// ---------------------------------------------------------------- preferences

fn load_settings(app: &PhotocraftApp) -> Settings {
    app.session.prefs().dialogs.get(SETTINGS_PREF).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
}

fn save_settings(app: &mut PhotocraftApp, s: &Settings) {
    let v = serde_json::to_value(s).unwrap_or(Value::Null);
    if app.session.prefs().dialogs.get(SETTINGS_PREF) != Some(&v) {
        app.session.prefs.edit(|prefs| prefs.dialogs.insert(SETTINGS_PREF.into(), v));
    }
}

/// The swatch book is remembered with the preferences: `{"name", "colors": [[ref, L, a, b]]}`.
pub(crate) fn load_book(app: &PhotocraftApp) -> Option<Book> {
    let v = app.session.prefs().dialogs.get(BOOK_PREF)?;
    let name = v.get("name")?.as_str()?.to_string();
    let swatches: Vec<Swatch> = v
        .get("colors")?
        .as_array()?
        .iter()
        .filter_map(|c| {
            let a = c.as_array()?;
            let n = |i: usize| a.get(i).and_then(Value::as_f64).map(|x| x as f32);
            Some(Swatch { name: a.first()?.as_str()?.to_string(), lab: [n(1)?, n(2)?, n(3)?] })
        })
        .collect();
    (!swatches.is_empty()).then_some(Book { name, swatches })
}

pub(crate) fn save_book(app: &mut PhotocraftApp, book: &Book) {
    let colors: Vec<Value> = book.swatches.iter().map(|s| json!([s.name, s.lab[0], s.lab[1], s.lab[2]])).collect();
    let v = json!({"name": book.name, "colors": colors});
    app.session.prefs.edit(|prefs| prefs.dialogs.insert(BOOK_PREF.into(), v));
}

// ---------------------------------------------------------------- opening and actions

/// DTF › Color Analyzer…: opens the module on the active document.
pub fn open(app: &mut PhotocraftApp) -> Result<Value, String> {
    let st = app.session.active().ok_or("no document")?;
    let (w, h, px) = cmds::visual_pixels(&st.doc).map_err(|e| e.to_string())?;
    if !px.iter().any(|q| q[3] >= 0.5) {
        return Err(tl!("No visible pixels to analyze.").to_string());
    }
    let bits = match st.doc.depth {
        photocraft_color::SampleType::U8 => 8,
        photocraft_color::SampleType::U16 => 16,
        photocraft_color::SampleType::F32 => 32,
    };
    let mode = crate::i18n::fmt(tl!("RGB {bits} bits"), &[("bits", &bits.to_string())]);
    let (doc, name, path, dpi) = (st.doc.id, st.doc.name.clone(), st.path.clone(), st.doc.resolution_dpi);
    let a = ColorAnalyzer::new(doc, &name, path, (w, h), dpi, mode, &px, load_settings(app), load_book(app));
    let n = a.colors.len();
    app.color_analyzer = Some(a);
    Ok(json!({"colors": n}))
}

/// Menu entry point: DTF › Color Analyzer… without parameters opens the module; with parameters
/// the command runs (the analysis as JSON, for automation).
pub fn menu(app: &mut PhotocraftApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    (id == cmds::ANALYZER && params.as_object().is_none_or(|o| o.is_empty())).then(|| open(app))
}

pub fn close(app: &mut PhotocraftApp) {
    if let Some(a) = app.color_analyzer.take() {
        save_settings(app, &a.settings);
    }
}

/// "Load…": a swatch book (.ase, .aco, .csv), remembered and shared by the Color Analyzer and the
/// Vectorization module.
pub(crate) fn load_swatch_book(app: &mut PhotocraftApp) -> Result<Value, String> {
    app.pick_file_bytes_filtered(&["ase", "aco", "csv"], |app, name, bytes| {
        let file = file_name(&name);
        let swatches = dtf_color::parse_swatch_book(&file, &bytes)?;
        let book = Book { name: file.clone(), swatches };
        save_book(app, &book);
        let n = book.swatches.len();
        if let Some(v) = app.vectorizer.as_mut() {
            v.set_book(Some(book.clone()));
        }
        if let Some(a) = app.color_analyzer.as_mut() {
            a.set_book(Some(book));
        }
        app.ui.status = crate::i18n::fmt(tl!("Swatch book loaded: {name}, {n} colors"), &[("name", &file), ("n", &n.to_string())]);
        Ok(json!({"name": file, "colors": n}))
    })
}

/// "Export PNG…": where to save, then the copy with the colours replaced (a new tab), saved there
/// as PNG. The module closes on the new tab.
pub fn export_png(app: &mut PhotocraftApp) -> Result<Value, String> {
    let a = app.color_analyzer.as_ref().ok_or("the Color Analyzer is not open")?;
    let params = a.replace_params();
    let (_, hint) = cmds::replaced_name(&a.name, a.path.as_deref());
    save_settings(app, &a.settings.clone());
    app.pick_save(&hint, move |app, path| {
        let path = std::path::Path::new(&path).with_extension("png").to_string_lossy().into_owned();
        app.run(cmds::REPLACE, params)?;
        let settings = app.save_settings();
        app.write_document(path.clone(), &settings, false)?;
        app.color_analyzer = None;
        Ok(json!({"path": path}))
    })
}

/// "PDF Sheet…": the colour sheet, written where the user chooses.
pub fn export_pdf(app: &mut PhotocraftApp) -> Result<Value, String> {
    let a = app.color_analyzer.as_ref().ok_or("the Color Analyzer is not open")?;
    let sheet = a.sheet();
    let stem = std::path::Path::new(&a.name).file_stem().and_then(|s| s.to_str()).unwrap_or(&a.name).to_string();
    let suggested = format!("{}.pdf", crate::i18n::fmt(tl!("{name} color sheet"), &[("name", &stem)]));
    app.pick_save(&suggested, move |app, path| {
        let path = std::path::Path::new(&path).with_extension("pdf").to_string_lossy().into_owned();
        let bytes = color_sheet::pdf(&sheet);
        let write = app.services.write.as_mut().ok_or("no writer configured")?;
        write(&path, &bytes)?;
        app.ui.status = crate::i18n::fmt(tl!("Color sheet saved: {path}"), &[("path", &path)]);
        Ok(json!({"path": path}))
    })
}

// ---------------------------------------------------------------- drawing

enum Action {
    Close,
    LoadBook,
    Pdf,
    Png,
}

/// The module, over the main window while it is open.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(a) = app.color_analyzer.as_ref() else { return };
    // Its document was closed.
    if !app.session.documents().iter().any(|d| d.doc.id == a.doc) {
        app.color_analyzer = None;
        return;
    }
    let before = a.settings.clone();
    let mut action = None;
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("dtf-color-analyzer")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let Some(a) = app.color_analyzer.as_mut() else { return };
        action = draw(ui, a, screen);
    });
    if let Some(a) = app.color_analyzer.as_mut() {
        if a.dirty && !ctx.input(|i| i.pointer.any_down()) {
            a.analyze();
        }
        if a.changing.is_some() || a.changing_range {
            change_reference(ctx, a);
        }
    }
    let escape = app.color_analyzer.as_ref().is_some_and(|a| a.changing.is_none() && !a.changing_range)
        && app.file_dialog.is_none()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        action = Some(Action::Close);
    }
    if let Some(a) = app.color_analyzer.as_ref()
        && a.settings != before
    {
        let s = a.settings.clone();
        save_settings(app, &s);
    }
    let result = match action {
        None => return,
        Some(Action::Close) => {
            close(app);
            return;
        }
        Some(Action::LoadBook) => load_swatch_book(app),
        Some(Action::Pdf) => export_pdf(app),
        Some(Action::Png) => export_png(app),
    };
    if let Err(e) = result {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

fn draw(ui: &mut egui::Ui, a: &mut ColorAnalyzer, screen: Rect) -> Option<Action> {
    let t = Tokens::get(ui.ctx());
    let (full, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
    let painter = ui.painter().clone();
    painter.rect_filled(full, 0.0, t.chrome);
    // Title bar.
    let title = Rect::from_min_size(full.min, vec2(full.width(), 40.0));
    painter.rect_filled(title, 0.0, t.dock);
    painter.line_segment([title.left_bottom(), title.right_bottom()], Stroke::new(1.0, t.separator));
    let galley = painter.layout_no_wrap(tl!("Color Analyzer").to_string(), crate::theme::semibold(15.0), t.text);
    let title_w = galley.size().x;
    painter.galley(pos2(title.left() + 16.0, title.center().y - galley.size().y / 2.0), galley, t.text);
    let info = format!("{} · {} · {} · {} × {} px · {}", a.name, a.mode, "sRGB", a.size.0, a.size.1, tl!("the document is not modified"));
    painter.text(pos2(title.left() + 32.0 + title_w, title.center().y), Align2::LEFT_CENTER, info, FontId::proportional(12.5), t.text_dim);
    let footer_h = 56.0;
    let body = Rect::from_min_max(pos2(full.left(), title.bottom()), pos2(full.right(), full.bottom() - footer_h));
    let right = Rect::from_min_max(pos2(body.right() - PANEL_W, body.top()), body.max);
    let left = Rect::from_min_max(body.min, pos2(right.left(), body.bottom()));
    painter.rect_filled(right, 0.0, t.dock);
    painter.line_segment([right.left_top(), right.left_bottom()], Stroke::new(1.0, t.separator));
    preview(ui, a, left);
    let mut action = None;
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(right.shrink2(vec2(0.0, 8.0))));
    egui::ScrollArea::vertical().id_salt("dtf-color-analyzer-panel").auto_shrink([false, false]).show(&mut panel, |ui| {
        ui.add_space(4.0);
        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 0)).show(ui, |ui| {
            ui.set_width(PANEL_W - 40.0);
            if settings_ui(ui, a) {
                action = Some(Action::LoadBook);
            }
        });
    });
    // Footer.
    let foot = Rect::from_min_max(pos2(full.left(), body.bottom()), full.max);
    painter.rect_filled(foot, 0.0, t.dock);
    painter.line_segment([foot.left_top(), foot.right_top()], Stroke::new(1.0, t.separator));
    let mut buttons = ui.new_child(egui::UiBuilder::new().max_rect(foot.shrink2(vec2(16.0, 10.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
    let png = crate::widgets::primary_button(&mut buttons, tl!("Export PNG…"), 120.0)
        .on_hover_text(tl!("A copy with the checked colors replaced by their references, saved as PNG and opened in a new tab."));
    if png.clicked() {
        action = Some(Action::Png);
    }
    if buttons.button(tl!("PDF Sheet…")).on_hover_text(tl!("An A4 sheet with the visual, its colors and their references.")).clicked() {
        action = Some(Action::Pdf);
    }
    if buttons.button(tl!("Close")).clicked() {
        action = Some(Action::Close);
    }
    action
}

/// The preview: its toolbar (views, eyedropper) and the visual fitted below.
fn preview(ui: &mut egui::Ui, a: &mut ColorAnalyzer, area: Rect) {
    let t = Tokens::get(ui.ctx());
    let bar = Rect::from_min_size(area.min + vec2(16.0, 10.0), vec2(area.width() - 32.0, 30.0));
    let mut tools = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
    // The replaced colours only (no view of the original, no eyedropper), and Color Range.
    tools.add(egui::Button::new(tl!("Replaced Colors")).selected(true));
    tools.add_space(16.0);
    let range = tools
        .add(egui::Button::new(tl!("Color Range")).selected(a.range_tool))
        .on_hover_text(tl!("Click: a color · Shift+click: add · Alt+click: remove"));
    if range.clicked() {
        a.range_tool = !a.range_tool;
    }
    let view = Rect::from_min_max(pos2(area.left() + 16.0, bar.bottom() + 10.0), area.max - vec2(16.0, 16.0));
    ui.painter().rect_filled(view, 4.0, t.canvas);
    if view.width() < 40.0 || view.height() < 40.0 {
        return;
    }
    let inner = view.shrink(16.0);
    let (iw, ih) = (a.w.max(1) as f32, a.h.max(1) as f32);
    let fit = |space: Rect| {
        let k = (space.width() / iw).min(space.height() / ih);
        Rect::from_center_size(space.center(), vec2(iw * k, ih * k))
    };
    let rows = a.patch_rows();
    // The replaced colours come with their patches.
    let (image_rect, patches) = if a.view == View::Replaced && !rows.is_empty() {
        let size = a.settings.patch_size.clamp(16.0, 120.0);
        if a.settings.patches_below {
            let cell = vec2(size + 150.0, size + 12.0);
            let per_line = ((inner.width() / cell.x).floor() as usize).max(1);
            let lines = rows.len().div_ceil(per_line);
            let h = (lines as f32 * cell.y + 24.0).min(inner.height() * 0.6);
            let img = Rect::from_min_max(inner.min, pos2(inner.right(), inner.bottom() - h - 12.0));
            (fit(img), Some(Rect::from_min_max(pos2(inner.left(), inner.bottom() - h), inner.max)))
        } else {
            let w = (size + 170.0).min(inner.width() * 0.5);
            let img = Rect::from_min_max(inner.min, pos2(inner.right() - w - 16.0, inner.bottom()));
            (fit(img), Some(Rect::from_min_max(pos2(inner.right() - w, inner.top()), inner.max)))
        }
    } else {
        (fit(inner), None)
    };
    let replaced = a.view == View::Replaced;
    let tex = a.texture(ui.ctx(), replaced);
    crate::widgets::checker(ui.painter(), image_rect, 8.0);
    ui.painter().image(tex.id(), image_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    // The range being built: what it leaves out is greyed.
    if a.range_tool
        && let Some((overlay, _)) = a.range_overlay(ui.ctx())
    {
        ui.painter().image(overlay.id(), image_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    if let Some(r) = patches {
        draw_patches(ui, a, &rows, r);
    }
    // Zoom of the visual on screen, against its full size.
    let zoom = image_rect.width() * ui.ctx().pixels_per_point() / a.size.0.max(1) as f32 * 100.0;
    ui.painter().text(
        pos2(view.right() - 10.0, view.bottom() - 8.0),
        Align2::RIGHT_BOTTOM,
        format!("{} %", zoom.round()),
        FontId::proportional(11.5),
        t.text_faint,
    );
    // Color Range's clicks.
    let response = ui.interact(image_rect, ui.id().with("dtf-color-analyzer-image"), Sense::click());
    if a.range_tool && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if a.range_tool
        && response.clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        let m = ui.input(|i| i.modifiers);
        a.range_click((p.x - image_rect.left()) / image_rect.width(), (p.y - image_rect.top()) / image_rect.height(), m.shift, m.alt);
    }
}

/// The patches on a white card: the reference's colour (the visual's when it has none), its
/// reference, hex and RGB as chosen.
fn draw_patches(ui: &egui::Ui, a: &ColorAnalyzer, rows: &[Row], area: Rect) {
    let painter = ui.painter();
    let ink = Color32::from_rgb(0x1c, 0x1c, 0x1c);
    let dim = Color32::from_rgb(0x55, 0x55, 0x55);
    let s = &a.settings;
    let mut size = s.patch_size.clamp(16.0, 120.0);
    let cell_h = |size: f32| size + 10.0;
    let cell_w = size + 150.0;
    let per_line = if s.patches_below { ((area.width() - 24.0) / cell_w).floor().max(1.0) as usize } else { 1 };
    let lines = rows.len().div_ceil(per_line) as f32;
    // Smaller patches when they don't fit.
    while size > 16.0 && lines * cell_h(size) + 24.0 > area.height() {
        size -= 2.0;
    }
    let card_h = (lines * cell_h(size) + 14.0).min(area.height());
    let card = Rect::from_min_size(area.min, vec2(if s.patches_below { area.width() } else { area.width().min(size + 170.0) }, card_h));
    painter.rect_filled(card, 2.0, Color32::WHITE);
    let clip = painter.with_clip_rect(card);
    for (i, r) in rows.iter().enumerate() {
        let (col, line) = (i % per_line, i / per_line);
        let at = card.min + vec2(12.0 + col as f32 * cell_w, 7.0 + line as f32 * cell_h(size));
        let sw = r.swatch.and_then(|(k, _)| a.swatch(k));
        let fill = sw.map_or(r.rgb, swatch8);
        let patch = Rect::from_min_size(at, vec2(size, size));
        clip.rect_filled(patch, 0.0, c32(fill));
        clip.rect_stroke(patch, 0.0, Stroke::new(1.0, Color32::from_gray(0xc8)), egui::StrokeKind::Inside);
        let mut lines: Vec<(String, FontId, Color32)> = Vec::new();
        if s.show_reference {
            lines.push((sw.map_or_else(|| r.hex.clone(), |w| w.name.clone()), crate::theme::semibold(12.0), ink));
        }
        if s.show_hex {
            lines.push((color_sheet::hex(fill), crate::theme::mono(11.0), ink));
        }
        if s.show_rgb {
            lines.push((format!("{} {} · {} · {}", tl!("RGB"), fill[0], fill[1], fill[2]), crate::theme::mono(11.0), ink));
        }
        if sw.is_some() {
            let what = if r.range.is_some() { tl!("Range") } else { tl!("Visual") };
            lines.push((format!("{what} {}", r.hex), crate::theme::mono(10.5), dim));
        }
        let mut y = at.y + (size - 14.0 * lines.len() as f32).max(0.0) / 2.0;
        for (text, font, colour) in lines {
            clip.text(pos2(at.x + size + 10.0, y), Align2::LEFT_TOP, text, font, colour);
            y += 14.0;
        }
    }
}

/// The settings panel. True when "Load…" was clicked.
fn settings_ui(ui: &mut egui::Ui, a: &mut ColorAnalyzer) -> bool {
    let t = Tokens::get(ui.ctx());
    let section = |ui: &mut egui::Ui, title: &str| {
        ui.add_space(10.0);
        ui.label(RichText::new(title).font(crate::theme::semibold(13.0)));
        ui.add_space(2.0);
        crate::widgets::hairline(ui);
        ui.add_space(4.0);
    };
    crate::dtf_help::sheet_toggle(ui, &mut a.help, cmds::ANALYZER);
    section(ui, tl!("Source"));
    egui::Grid::new("dtf-color-analyzer-source").num_columns(2).spacing(vec2(12.0, 4.0)).show(ui, |ui| {
        for (k, v) in [
            (tl!("Color Profile"), PROFILE.to_string()),
            (tl!("Mode"), a.mode.clone()),
            (tl!("Analyzed Pixels"), tl!("Visible, transparency ignored").to_string()),
        ] {
            ui.label(RichText::new(k).color(t.text_dim));
            ui.label(v);
            ui.end_row();
        }
    });
    section(ui, tl!("Analysis"));
    let mut changed = false;
    egui::Grid::new("dtf-color-analyzer-analysis").num_columns(2).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
        ui.label(tl!("Maximum Colors"));
        changed |= ui.add(egui::DragValue::new(&mut a.settings.max_colors).range(1..=32)).changed();
        ui.end_row();
        ui.label(tl!("Grouping (ΔE)")).on_hover_text(tl!("Colors closer than this difference are counted as one."));
        changed |= ui.add(egui::DragValue::new(&mut a.settings.merge_delta_e).range(0.0..=30.0).speed(0.1).fixed_decimals(1)).changed();
        ui.end_row();
    });
    changed |= ui.checkbox(&mut a.settings.ignore_small, tl!("Ignore colors under 0.5 % of the area")).changed();
    if changed {
        a.dirty = true;
    }
    section(ui, tl!("Reference Swatch Book"));
    let mut load = false;
    ui.horizontal(|ui| {
        let text = match &a.book {
            Some(b) => crate::i18n::fmt(tl!("{name} · {n} colors"), &[("name", &b.name), ("n", &b.swatches.len().to_string())]),
            None => tl!("None loaded").to_string(),
        };
        ui.add(egui::Label::new(text).truncate()).on_hover_text(a.book.as_ref().map_or("", |b| b.name.as_str()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            load = ui.button(tl!("Load…")).clicked();
        });
    });
    ui.label(RichText::new(tl!("Formats: .ase, .aco, .csv (ref; R; G; B or L*a*b*). Difference measured in ΔE 2000.")).small().color(t.text_faint));
    section(ui, &crate::i18n::fmt(tl!("Detected Colors: {n}"), &[("n", &a.colors.len().to_string())]));
    colors_table(ui, a);
    section(ui, tl!("Color Range"));
    range_ui(ui, a);
    section(ui, tl!("Patches · on-screen check and PDF sheet"));
    egui::Grid::new("dtf-color-analyzer-patches").num_columns(2).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
        ui.label(tl!("Position"));
        ui.horizontal(|ui| {
            ui.radio_value(&mut a.settings.patches_below, false, tl!("Right"));
            ui.radio_value(&mut a.settings.patches_below, true, tl!("Below"));
        });
        ui.end_row();
        ui.label(tl!("Size"));
        ui.add(egui::Slider::new(&mut a.settings.patch_size, 24.0..=96.0).suffix(" pt"));
        ui.end_row();
        ui.label(tl!("Show"));
        ui.horizontal(|ui| {
            ui.checkbox(&mut a.settings.show_reference, tl!("Reference"));
            ui.checkbox(&mut a.settings.show_hex, tl!("Hex"));
            ui.checkbox(&mut a.settings.show_rgb, tl!("RGB"));
        });
        ui.end_row();
        ui.label(tl!("Client"));
        ui.add(egui::TextEdit::singleline(&mut a.settings.client).hint_text(tl!("On the PDF sheet")).desired_width(200.0));
        ui.end_row();
    });
    ui.add_space(12.0);
    load
}

/// Color Range in the panel: the colours picked, tolerance, contiguous pixels, the swatch they
/// become, Apply; then the ranges applied.
fn range_ui(ui: &mut egui::Ui, a: &mut ColorAnalyzer) {
    let t = Tokens::get(ui.ctx());
    if !a.range_tool {
        ui.label(RichText::new(tl!("Turn on Color Range above the visual, then click a color.")).small().color(t.text_faint));
    } else {
        ui.label(RichText::new(tl!("Click: a color · Shift+click: add · Alt+click: remove")).small().color(t.text_faint));
        let mut tolerance = a.range.tolerance;
        let mut contiguous = a.range.contiguous;
        egui::Grid::new("dtf-color-analyzer-range").num_columns(2).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
            ui.label(tl!("Tolerance"));
            ui.add(egui::Slider::new(&mut tolerance, 0.0..=100.0).fixed_decimals(0));
            ui.end_row();
        });
        ui.checkbox(&mut contiguous, tl!("Contiguous Pixels"));
        a.set_range_options(tolerance.round(), contiguous);
        if a.range.add.is_empty() {
            ui.label(RichText::new(tl!("Pick a color on the visual.")).color(t.text_dim));
        } else {
            ui.horizontal_wrapped(|ui| {
                for (lab, _) in &a.range.add {
                    patch(ui, lab_rgb8(*lab), 20.0).on_hover_text(color_sheet::hex(lab_rgb8(*lab)));
                }
                for lab in &a.range.subtract {
                    let r = patch(ui, lab_rgb8(*lab), 20.0).on_hover_text(format!("− {}", color_sheet::hex(lab_rgb8(*lab))));
                    ui.painter().line_segment([r.rect.left_center() + vec2(4.0, 0.0), r.rect.right_center() - vec2(4.0, 0.0)], Stroke::new(2.0, Color32::WHITE));
                }
            });
            if let Some((_, share)) = a.range_overlay(ui.ctx()) {
                ui.label(RichText::new(crate::i18n::fmt(tl!("{p} % of the visible pixels"), &[("p", &decimal(f64::from(share), 1))])).small().color(t.text_dim));
            }
            ui.label(RichText::new(tl!("Replacement Color")).color(t.text_dim));
            match a.range_target().and_then(|(k, de)| a.swatch(k).map(|s| (s.clone(), de))) {
                Some((s, de)) => {
                    ui.horizontal(|ui| {
                        patch(ui, swatch8(&s), 24.0);
                        ui.vertical(|ui| {
                            ui.set_max_width(160.0);
                            ui.add(egui::Label::new(RichText::new(&s.name).strong()).truncate()).on_hover_text(&s.name);
                            if ui.link(RichText::new(tl!("Change…")).small()).clicked() {
                                a.changing_range = true;
                                a.search.clear();
                            }
                        });
                        let (bg, fg) = de_badge(de);
                        let galley = ui.painter().layout_no_wrap(decimal(f64::from(de), 1), crate::theme::mono(11.0), fg);
                        let (r, _) = ui.allocate_exact_size(galley.size() + vec2(10.0, 4.0), Sense::hover());
                        ui.painter().rect_filled(r, 3.0, bg);
                        ui.painter().galley(r.center() - galley.size() / 2.0, galley, fg);
                    });
                }
                None => {
                    ui.label(RichText::new(tl!("Load a swatch book to find the references.")).color(t.warning));
                }
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(a.range_target().is_some(), egui::Button::new(tl!("Apply"))).clicked() {
                    a.apply_range();
                }
                if ui.button(tl!("Clear")).clicked() {
                    a.clear_range();
                }
            });
        }
    }
    if a.ranges.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.label(RichText::new(tl!("Applied Ranges")).color(t.text_dim));
    let mut remove = None;
    let mut toggle = None;
    egui::Grid::new("dtf-color-analyzer-ranges").num_columns(5).spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        for (i, r) in a.ranges.iter().enumerate() {
            let mut on = r.on;
            if ui.add(egui::Checkbox::without_text(&mut on)).on_hover_text(tl!("Replaced in the PNG export")).changed() {
                toggle = Some((i, on));
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (lab, _) in r.range.add.iter().take(3) {
                    patch(ui, lab_rgb8(*lab), 18.0);
                }
                ui.label("→");
                if let Some(s) = a.swatch(r.swatch) {
                    patch(ui, swatch8(s), 18.0);
                }
            });
            ui.vertical(|ui| {
                ui.set_max_width(120.0);
                let name = a.swatch(r.swatch).map_or("—", |s| s.name.as_str());
                ui.add(egui::Label::new(RichText::new(name).strong()).truncate()).on_hover_text(name);
                let mut how = format!("{} {}", tl!("Tolerance"), r.range.tolerance.round());
                if r.range.contiguous {
                    how.push_str(&format!(" · {}", tl!("Contiguous")));
                }
                ui.label(RichText::new(how).small().color(t.text_faint));
            });
            ui.label(RichText::new(format!("{} %", decimal(f64::from(r.share), 1))).font(crate::theme::mono(11.0)));
            if ui.small_button("×").on_hover_text(tl!("Remove this range")).clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some((i, on)) = toggle {
        a.set_range_on(i, on);
    }
    if let Some(i) = remove {
        a.remove_range(i);
    }
}

pub(crate) fn de_badge(de: f32) -> (Color32, Color32) {
    if de <= 2.0 {
        (Color32::from_rgb(0x1e, 0x4d, 0x2b), Color32::from_rgb(0x9b, 0xe3, 0xb0))
    } else if de <= 5.0 {
        (Color32::from_rgb(0x5a, 0x3d, 0x0c), Color32::from_rgb(0xff, 0xd2, 0x7a))
    } else {
        (Color32::from_rgb(0x5c, 0x1f, 0x1f), Color32::from_rgb(0xff, 0xb3, 0xb3))
    }
}

fn patch(ui: &mut egui::Ui, rgb: [u8; 3], size: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().rect_filled(r, 2.0, c32(rgb));
    ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, Tokens::get(ui.ctx()).field_border), egui::StrokeKind::Inside);
    resp
}

fn colors_table(ui: &mut egui::Ui, a: &mut ColorAnalyzer) {
    let t = Tokens::get(ui.ctx());
    let rows = a.rows();
    let mut replace: Option<(String, bool)> = None;
    let mut remove = None;
    let mut change = None;
    egui::Grid::new("dtf-color-analyzer-colors").num_columns(5).spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        // Codes and figures stay on one line: a wrapped hex would stretch the row.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.add(crate::icons::image("arrow-left-right", 14.0, t.text_dim)).on_hover_text(tl!("Replaced in the PNG export"));
        ui.label(RichText::new(tl!("Visual")).small().color(t.text_dim));
        ui.label(RichText::new(tl!("Area")).small().color(t.text_dim));
        ui.label(RichText::new(tl!("Match")).small().color(t.text_dim));
        ui.label(RichText::new("ΔE").small().color(t.text_dim));
        ui.end_row();
        for (i, r) in rows.iter().enumerate() {
            let mut on = r.replace;
            let check = ui.add_enabled(r.swatch.is_some(), egui::Checkbox::without_text(&mut on));
            if check.changed() {
                replace = Some((r.hex.clone(), on));
            }
            ui.horizontal(|ui| {
                patch(ui, r.rgb, 24.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(&r.hex).font(crate::theme::mono(11.5)));
                    ui.label(RichText::new(format!("{} · {} · {}", r.rgb[0], r.rgb[1], r.rgb[2])).font(crate::theme::mono(10.0)).color(t.text_faint));
                });
                if let Some(k) = r.extra
                    && ui.small_button("×").on_hover_text(tl!("Remove this color")).clicked()
                {
                    remove = Some(k);
                }
            });
            ui.label(RichText::new(format!("{} %", decimal(f64::from(r.share), 1))).font(crate::theme::mono(11.5)));
            match r.swatch.and_then(|(k, de)| a.swatch(k).map(|s| (s, de))) {
                Some((s, de)) => {
                    ui.horizontal(|ui| {
                        patch(ui, swatch8(s), 24.0);
                        ui.vertical(|ui| {
                            // Long swatch names are cut so the ΔE column stays in the panel.
                            ui.set_max_width(108.0);
                            ui.add(egui::Label::new(RichText::new(&s.name).strong()).truncate()).on_hover_text(&s.name);
                            if ui.link(RichText::new(tl!("Change…")).small()).clicked() {
                                change = Some(i);
                            }
                        });
                    });
                    // A badge sized to its figure, centred in the row.
                    let (bg, fg) = de_badge(de);
                    let galley = ui.painter().layout_no_wrap(decimal(f64::from(de), 1), crate::theme::mono(11.0), fg);
                    let (r, _) = ui.allocate_exact_size(galley.size() + vec2(10.0, 4.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.0, bg);
                    ui.painter().galley(r.center() - galley.size() / 2.0, galley, fg);
                }
                None => {
                    ui.label(RichText::new("—").color(t.text_faint));
                    ui.label("");
                }
            }
            ui.end_row();
        }
    });
    if a.book.is_none() {
        ui.label(RichText::new(tl!("Load a swatch book to find the references.")).color(t.warning));
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(tl!("Checked: the visual's color is replaced by the reference in the PNG export. ΔE: up to 2 = imperceptible · 2 to 5 = visible up close · above = another hue"))
            .small()
            .color(t.text_faint),
    );
    if let Some((hex, on)) = replace {
        a.set_replace(&hex, on);
    }
    if let Some(k) = remove {
        a.remove_extra(k);
    }
    if change.is_some() {
        a.changing = change;
        a.search.clear();
    }
}

/// What the swatch picker came to.
pub(crate) enum Pick {
    /// Still open.
    Open,
    /// Cancelled or closed.
    Close,
    /// A swatch picked (`None`: the nearest, automatically).
    Chosen(Option<usize>),
}

/// The swatch picker ("Change…"): the swatch-book colours nearest to `lab` first, with a search.
pub(crate) fn swatch_picker(ctx: &egui::Context, id: &str, rgb: [u8; 3], lab: [f32; 3], current: Option<usize>, book: &Book, search: &mut String) -> Pick {
    let hex = color_sheet::hex(rgb);
    let mut pick = Pick::Open;
    let needle = search.to_lowercase();
    let mut list: Vec<(usize, f32)> = book
        .swatches
        .iter()
        .enumerate()
        .filter(|(_, s)| needle.is_empty() || s.name.to_lowercase().contains(&needle))
        .map(|(k, s)| (k, dtf_color::delta_e2000(lab, s.lab)))
        .collect();
    list.sort_by(|x, y| x.1.total_cmp(&y.1));
    list.truncate(200);
    let modal = egui::Modal::new(egui::Id::new(id)).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.horizontal(|ui| {
            patch(ui, rgb, 28.0);
            ui.label(RichText::new(crate::i18n::fmt(tl!("Reference for {hex}"), &[("hex", &hex)])).font(crate::theme::semibold(14.0)));
        });
        ui.add_space(6.0);
        ui.add(egui::TextEdit::singleline(search).hint_text(tl!("Search")).desired_width(f32::INFINITY));
        ui.add_space(4.0);
        if ui.button(tl!("Nearest (automatic)")).clicked() {
            pick = Pick::Chosen(None);
        }
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (k, de) in &list {
                let s = &book.swatches[*k];
                let resp = ui
                    .horizontal(|ui| {
                        patch(ui, swatch8(s), 20.0);
                        let r = ui.selectable_label(current == Some(*k), &s.name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("ΔE {}", decimal(f64::from(*de), 1))).font(crate::theme::mono(11.0)));
                        });
                        r
                    })
                    .inner;
                if resp.clicked() {
                    pick = Pick::Chosen(Some(*k));
                }
            }
        });
        ui.add_space(6.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tl!("Cancel")).clicked() && matches!(pick, Pick::Open) {
                pick = Pick::Close;
            }
        });
    });
    if modal.should_close() && matches!(pick, Pick::Open) {
        pick = Pick::Close;
    }
    pick
}

/// "Change…": the reference of a row of the table, or of the range being built.
fn change_reference(ctx: &egui::Context, a: &mut ColorAnalyzer) {
    // The colour the reference is for: a row of the table, or the range being built.
    let target = match a.changing {
        Some(i) => a.rows().get(i).map(|r| (r.rgb, r.lab, r.swatch.map(|(k, _)| k), Some(r.hex.clone()))),
        None if a.changing_range => a.range.add.first().map(|(lab, _)| (lab_rgb8(*lab), *lab, a.range_target().map(|(k, _)| k), None)),
        None => return,
    };
    let (Some((rgb, lab, current, hex)), Some(book)) = (target, a.book.as_ref()) else {
        a.changing = None;
        a.changing_range = false;
        return;
    };
    let mut search = std::mem::take(&mut a.search);
    let pick = swatch_picker(ctx, "dtf-color-analyzer-change", rgb, lab, current, book, &mut search);
    a.search = search;
    match pick {
        Pick::Open => return,
        Pick::Close => {}
        Pick::Chosen(choice) => match hex {
            Some(hex) => a.set_swatch(&hex, choice),
            None => a.range_swatch = choice,
        },
    }
    a.changing = None;
    a.changing_range = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    fn analyzer(book: Option<Book>) -> ColorAnalyzer {
        // 40 × 20: blue left, orange right, transparent top rows.
        let mut px = Vec::new();
        for y in 0..20 {
            for x in 0..40 {
                px.push(if y < 4 {
                    [0.0; 4]
                } else if x < 20 {
                    [31.0 / 255.0, 58.0 / 255.0, 147.0 / 255.0, 1.0]
                } else {
                    [228.0 / 255.0, 87.0 / 255.0, 46.0 / 255.0, 1.0]
                });
            }
        }
        ColorAnalyzer::new(photocraft_doc::DocId::fresh(), "logo.png", None, (40, 20), 300.0, "RVB 8 bits".into(), &px, Settings::default(), book)
    }

    fn book() -> Book {
        Book {
            name: "Ex.csv".into(),
            swatches: dtf_color::parse_swatch_book("Ex.csv", b"Ref;R;V;B\nEX-201 Bleu roi;30;59;150\nEX-114 Orange vif;226;94;43\nNoir;0;0;0\n").unwrap(),
        }
    }

    #[test]
    fn rows_match_the_swatch_book_and_follow_the_choices() {
        let mut a = analyzer(Some(book()));
        let rows = a.rows();
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].hex.as_str(), rows[0].share.round()), ("#1F3A93", 50.0));
        assert_eq!(a.book.as_ref().unwrap().swatches[rows[0].swatch.unwrap().0].name, "EX-201 Bleu roi");
        assert!(rows.iter().all(|r| r.replace));
        // A far nearest swatch is left unchecked; picked by hand or checked, it is replaced.
        let far = Book { name: "Far.csv".into(), swatches: dtf_color::parse_swatch_book("Far.csv", b"Ref;R;V;B\nNoir;0;0;0\n").unwrap() };
        let mut b = analyzer(Some(far));
        assert!(b.rows().iter().all(|r| r.swatch.is_some() && !r.replace));
        b.set_swatch("#1F3A93", Some(0));
        assert!(b.rows()[0].replace && !b.rows()[1].replace);
        // Only the replaced colours get a patch on screen.
        assert_eq!(b.patch_rows().iter().map(|r| r.hex.as_str()).collect::<Vec<_>>(), ["#1F3A93"]);
        b.set_replace("#E4572E", true);
        assert!(b.rows()[1].replace);
        assert_eq!(b.patch_rows().len(), 2);
        // Another reference, then not replaced.
        a.set_swatch("#1F3A93", Some(2));
        assert_eq!(a.rows()[0].swatch.unwrap().0, 2);
        a.set_replace("#1F3A93", false);
        let p = a.replace_params();
        assert_eq!(p["colors"][0], json!({"from": "#1F3A93", "to": null}));
        assert_eq!(p["colors"][1], json!({"from": "#E4572E", "to": "#E25E2B"}));
        // Without a swatch book nothing is replaced.
        a.set_book(None);
        assert!(!a.any_replaced());
    }

    #[test]
    fn the_replaced_preview_takes_the_swatch_colours() {
        let mut a = analyzer(Some(book()));
        let px = a.replaced_pixels().to_vec();
        let hex = |q: [f32; 4]| dtf_color::hex([q[0], q[1], q[2]]);
        assert_eq!(hex(px[10 * 40 + 5]), "#1E3B96");
        assert_eq!(hex(px[10 * 40 + 35]), "#E25E2B");
        assert_eq!(px[0][3], 0.0);
    }

    #[test]
    fn the_eyedropper_adds_a_colour_once() {
        let mut a = analyzer(None);
        assert_eq!(a.pick(5, 0), None, "transparent");
        assert!(!a.add_color(a.pick(5, 10).unwrap()), "already a main colour");
        assert!(a.add_color([1.0, 1.0, 1.0]));
        assert_eq!((a.colors.len(), a.rows()[2].extra), (3, Some(0)));
        assert_eq!(a.rows()[2].share, 0.0);
        a.remove_extra(0);
        assert_eq!(a.colors.len(), 2);
    }

    #[test]
    fn large_visuals_are_analysed_on_an_exact_colour_copy() {
        let px = vec![[0.5, 0.25, 0.75, 1.0]; 5000 * 10];
        let (w, h, small) = downsample(5000, 10, &px, PREVIEW_MAX);
        assert_eq!((w, h), (1667, 4));
        assert!(small.iter().all(|q| *q == [0.5, 0.25, 0.75, 1.0]));
    }

    /// An app with a 40 × 20 visual, the example swatch book remembered, and the files written.
    fn app() -> (PhotocraftApp, std::rc::Rc<std::cell::RefCell<Vec<(String, Vec<u8>)>>>) {
        let written = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let files = written.clone();
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            crate::Services {
                export: Some(Box::new(|doc, path, _| {
                    photocraft_io::export(doc, path, &Default::default()).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string())
                })),
                write: Some(Box::new(move |path, bytes| {
                    files.borrow_mut().push((path.to_string(), bytes.to_vec()));
                    Ok(())
                })),
                ..Default::default()
            },
        );
        app.run("file.new", json!({"width": 40, "height": 20, "resolution": 300, "background": "transparent", "name": "logo.png"})).unwrap();
        app.session
            .edit("Paint", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                for y in 4..20 {
                    for x in 0..40 {
                        let c = if x < 20 { [31.0, 58.0, 147.0] } else { [228.0, 87.0, 46.0] };
                        surf.write_pixel(x, y, &[c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, 1.0]);
                    }
                }
                Ok(())
            })
            .unwrap();
        save_book(&mut app, &book());
        (app, written)
    }

    #[test]
    fn the_menu_opens_the_module_with_the_remembered_swatch_book() {
        let (mut app, _) = app();
        let ctx = egui::Context::default();
        crate::menus::invoke(&mut app, &ctx, cmds::ANALYZER, json!({})).unwrap();
        let a = app.color_analyzer.as_ref().unwrap();
        assert_eq!((a.colors.len(), a.book.as_ref().map(|b| b.swatches.len())), (2, Some(3)));
        // The other menus wait while it is open.
        assert!(!crate::menus::modal_allows(&app, "image.imageSize"));
        // With parameters the command answers the analysis.
        app.color_analyzer = None;
        let r = crate::menus::invoke(&mut app, &ctx, cmds::ANALYZER, json!({"maxColors": 4})).unwrap();
        assert_eq!(r["colors"].as_array().map(Vec::len), Some(2));
        assert!(app.color_analyzer.is_none());
    }

    #[test]
    fn the_png_export_saves_a_replaced_copy_in_a_new_tab() {
        let (mut app, written) = app();
        open(&mut app).unwrap();
        app.color_analyzer.as_mut().unwrap().set_replace("#E4572E", false);
        let (dialog, _) = crate::file_dialog::fake(vec![Some(crate::file_dialog::FileDialogAnswer::SaveTo("out/logo nuancier.png".into()))]);
        app.services.file_dialog = Some(dialog);
        export_png(&mut app).unwrap();
        app.poll_file_dialog(&egui::Context::default(), None);
        assert!(app.color_analyzer.is_none(), "closed on the new tab");
        assert_eq!(app.session.documents().len(), 2);
        let copy = app.session.active().unwrap();
        assert_eq!((copy.doc.name.as_str(), copy.path.as_deref()), ("logo nuancier.png", Some("out/logo nuancier.png")));
        let files = written.borrow();
        assert_eq!(files.len(), 1);
        let png = photocraft_io::import("x.png", &files[0].1).unwrap().document;
        assert_eq!((png.size.width, png.size.height), (40, 20));
        let (_, _, px) = cmds::visual_pixels(&png).unwrap();
        let hex = |q: [f32; 4]| dtf_color::hex([q[0], q[1], q[2]]);
        assert_eq!((hex(px[10 * 40 + 5]), hex(px[10 * 40 + 35]), px[0][3]), ("#1E3B96".into(), "#E4572E".into(), 0.0));
        // The original keeps its colours.
        let (_, _, orig) = cmds::visual_pixels(&app.session.documents()[0].doc).unwrap();
        assert_eq!(hex(orig[10 * 40 + 5]), "#1F3A93");
    }

    #[test]
    fn the_pdf_sheet_is_written_where_chosen() {
        let (mut app, written) = app();
        open(&mut app).unwrap();
        let (dialog, _) = crate::file_dialog::fake(vec![Some(crate::file_dialog::FileDialogAnswer::SaveTo("fiche".into()))]);
        app.services.file_dialog = Some(dialog);
        export_pdf(&mut app).unwrap();
        app.poll_file_dialog(&egui::Context::default(), None);
        let files = written.borrow();
        assert_eq!(files[0].0, "fiche.pdf");
        assert!(files[0].1.starts_with(b"%PDF"));
        assert!(app.color_analyzer.is_some(), "the module stays open");
    }

    #[test]
    fn the_module_draws_over_the_window_and_closes_with_escape() {
        let (mut app, _) = app();
        open(&mut app).unwrap();
        app.color_analyzer.as_mut().unwrap().view = View::Replaced;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                let ready = egui::Id::new("analyzer-test-fonts");
                if !ui.ctx().data(|d| d.get_temp::<bool>(ready)).unwrap_or(false) {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ALL[0]);
                    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
                    return;
                }
                show(app, ui.ctx());
            },
            app,
        );
        h.run_steps(3);
        assert!(h.query_by_label("Export PNG…").is_some());
        assert!(h.query_by_label_contains("EX-201 Bleu roi").is_some());
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(h.state().color_analyzer.is_none());
    }

    /// Color Range: a click picks the orange, Apply turns it black (a swatch picked by hand)
    /// over the table's replacement; the range has its patch, its PDF row and goes to the export.
    #[test]
    fn a_colour_range_is_picked_applied_and_exported() {
        let mut a = analyzer(Some(book()));
        assert!(!a.range_click(0.5, 0.05, false, false), "transparent: nothing to pick");
        assert!(a.range_click(0.75, 0.5, false, false));
        // Alt+click takes the blue out.
        assert!(a.range_click(0.25, 0.5, false, true));
        assert_eq!(a.range.subtract.len(), 1);
        a.clear_range();
        a.range_click(0.75, 0.5, false, false);
        // The nearest swatch first, then black picked by hand.
        assert_eq!(a.range_target().unwrap().0, 1);
        a.range_swatch = Some(2);
        let ctx = egui::Context::default();
        let (_, share) = a.range_overlay(&ctx).unwrap();
        assert!((share - 50.0).abs() < 0.5, "{share}");
        assert!(a.apply_range());
        assert!(a.range.add.is_empty(), "a new range starts");
        // The replaced preview: orange is black, blue is its swatch.
        let w = a.w;
        let px = a.replaced_pixels().to_vec();
        assert_eq!(color_sheet::hex(rgb8([px[10 * w + 30][0], px[10 * w + 30][1], px[10 * w + 30][2]])), "#000000");
        assert_eq!(color_sheet::hex(rgb8([px[10 * w + 5][0], px[10 * w + 5][1], px[10 * w + 5][2]])), "#1E3B96");
        // Its patch and its sheet row.
        let rows = a.patch_rows();
        assert_eq!(rows.last().unwrap().range, Some(0));
        assert_eq!(a.book.as_ref().unwrap().swatches[rows.last().unwrap().swatch.unwrap().0].name, "Noir");
        let sheet = a.sheet();
        assert!(sheet.rows.last().unwrap().range);
        // The sheet shows the recoloured visual beside the original: orange is black there.
        let (rw, _, rpx) = sheet.replaced.as_ref().unwrap();
        let at = (10 * rw + 30) * 4;
        assert_eq!(&rpx[at..at + 3], &[0, 0, 0]);
        // The export's params.
        let p = a.replace_params();
        assert_eq!(p["ranges"][0]["to"], "#000000");
        assert_eq!(p["ranges"][0]["add"][0]["hex"], "#E4572E");
        assert_eq!(p["ranges"][0]["tolerance"], 20.0);
        // Unchecked: kept in the list, out of the preview, the patches, the sheet and the export.
        a.set_range_on(0, false);
        assert!(a.replace_params()["ranges"].as_array().unwrap().is_empty());
        assert!(a.patch_rows().iter().all(|r| r.range.is_none()));
        let px = a.replaced_pixels().to_vec();
        assert_ne!(color_sheet::hex(rgb8([px[10 * w + 30][0], px[10 * w + 30][1], px[10 * w + 30][2]])), "#000000");
        a.set_range_on(0, true);
        assert_eq!(a.replace_params()["ranges"].as_array().unwrap().len(), 1);
        a.remove_range(0);
        assert!(a.replace_params()["ranges"].as_array().unwrap().is_empty());
    }

    #[test]
    fn the_sheet_lists_the_rows_with_their_references() {
        let a = analyzer(Some(book()));
        let s = a.sheet();
        assert_eq!((s.rows.len(), s.size_px, s.image.0, s.image.1), (2, (40, 20), 40, 20));
        assert_eq!(s.rows[0].reference.as_ref().unwrap().name, "EX-201 Bleu roi");
        assert_eq!(s.swatch_book.as_deref(), Some("Ex.csv"));
        // An unchecked colour leaves the sheet, as it leaves the patches.
        let mut a = a;
        a.set_replace("#E4572E", false);
        assert_eq!(a.sheet().rows.len(), 1);
    }
}
