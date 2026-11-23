// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Mockup (beta)…: the customer's visuals shown on a photo of the garment, at their real
//! print size.
//!
//! * Header: the DTF Craft logo, "DTF Mockup" and its BETA badge, then Report a Bug, Cancel and
//!   Export….
//! * Left column, in three numbered steps: **Product Photo** (a PNG or JPG of the garment),
//!   **Scale** (a line drawn on the photo along something whose length is known, the chest width
//!   say, and that length in cm: the photo's pixels per cm, and rulers in cm round it) and
//!   **Visuals** (PNG or SVG files, or the open document; each arrives at its print size, its
//!   pixels at its resolution). Visuals wait for the scale: without it their size means nothing.
//! * Canvas: the photo with the visuals on it as they would print (fabric shadows and folds), the
//!   reference line (its ends can be dragged) and the rulers. A click picks a visual, a drag moves
//!   it; its size in cm is written under it.
//! * Right column, in folding sections: **Selected Visual** (width and height in cm, proportions
//!   kept or not, rotation, centre on the reference line, delete), **Rendering** (fabric shadows,
//!   folds, ink opacity) and **Export** (PNG or JPG), folded.
//! * Status bar: the scale, the photo's size in cm, the visuals, the last message.
//!
//! The sizes are the mockup's only: the visuals' files are not changed. Export renders the mockup
//! at the photo's resolution through `dtf.mockup` (the same render as the preview, see
//! [`photocraft_engine::mockup_cmds::composite`]), writes it and opens it as a new document. In
//! beta: no project is saved, the export is the result.
//!
//! The window is light, as Gang Sheet's ([`crate::gang_sheet_ui::light_scope`]).
//!
//! Control channel: `ui.menu.invoke {"id":"dtf.mockup","params":{"ui":{…}}}` with `photo` (a
//! path), `scale` (`from`, `to`: photo pixels, `cm`), `draw` (arm the line), `addFiles`,
//! `addDocument`, `select` (index or `null`), `visual` (`index`, `x`, `y`, `widthCm`, `heightCm`,
//! `lock`, `rotation`), `center`, `remove` (index), `render` (`shadows`, `folds`, `opacity`),
//! `format` (`png`/`jpg`), `export` (path), `cancel`.

use egui::{Align2, Color32, FontId, Pos2, Rect as ERect, Sense, Shape, Stroke, StrokeKind, TextureHandle, pos2, vec2};
use photocraft_algo::transform::Homography;
use photocraft_doc::DocId;
use photocraft_engine::gang_sheet_cmds::{self as gang, Visual};
use photocraft_engine::mockup_cmds::{self as cmd, Bend, Light, Photo, Placed, Placement, Print, Render};
use photocraft_geom::warp::WarpStyle;
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::file_dialog::{FileDialogAnswer, FileDialogRequest};
use crate::gang_sheet_ui::{BLUE, MAGENTA, blue_button, card_frame, light_scope, number_row, preview_image, switch_row};
use crate::theme::Tokens;
use crate::widgets;

const HEADER_H: f32 = 52.0;
const STATUS_H: f32 = 30.0;
const LEFT_W: f32 = 320.0;
const RIGHT_W: f32 = 320.0;
const RULER: f32 = 20.0;
/// The preview is rendered on the photo reduced to this many pixels on its longest side.
const PREVIEW_SIDE: u32 = 1400;
/// Most visuals on a mockup.
const MAX_ITEMS: usize = 50;
/// A warp style's bend when it is picked.
const DEFAULT_BEND: f64 = 50.0;
/// Where a new visual lands, after the previous one: this far right and down (cm).
const CASCADE_CM: f64 = 2.0;
/// An end of the reference line is grabbed this close (screen points).
const GRAB: f32 = 10.0;
const GREEN: Color32 = Color32::from_rgb(22, 140, 72);

const SECTION_VISUAL: usize = 0;
const SECTION_TRANSFORM: usize = 1;
const SECTION_RENDER: usize = 2;
const SECTION_EXPORT: usize = 3;
/// The transform palette's width on the photo.
const PALETTE_W: f32 = 150.0;
/// The closest zoom: screen points per photo pixel.
const MAX_ZOOM: f32 = 8.0;

// ---------------------------------------------------------------- state

/// Where a visual comes from: the export reloads it from there.
#[derive(Clone, Debug, PartialEq)]
enum Origin {
    File(String),
    Document(DocId),
}

/// Edit › Transform's tools, in its menu order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    Scale,
    Rotate,
    Skew,
    Distort,
    Perspective,
    Warp,
}

impl Tool {
    const ALL: [Tool; 6] = [Tool::Scale, Tool::Rotate, Tool::Skew, Tool::Distort, Tool::Perspective, Tool::Warp];

    fn label(self) -> &'static str {
        match self {
            Tool::Scale => tl!("Scale"),
            Tool::Rotate => tl!("Rotate"),
            Tool::Skew => tl!("Skew"),
            Tool::Distort => tl!("Distort"),
            Tool::Perspective => tl!("Perspective"),
            Tool::Warp => tl!("Warp"),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Tool::Scale => "scale",
            Tool::Rotate => "rotate",
            Tool::Skew => "skew",
            Tool::Distort => "distort",
            Tool::Perspective => "perspective",
            Tool::Warp => "warp",
        }
    }

    fn parse(s: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.id().eq_ignore_ascii_case(s))
    }

    /// Its icon in the palette.
    fn icon(self) -> &'static str {
        match self {
            Tool::Scale => "maximize-2",
            Tool::Rotate => "rotate-cw",
            Tool::Skew => "square-dashed",
            Tool::Distort => "vector-shape",
            Tool::Perspective => "perspective-crop",
            Tool::Warp => "grid-3x3",
        }
    }
}

/// The warp style's name in the window.
fn style_label(s: WarpStyle) -> &'static str {
    match s {
        WarpStyle::None => tl!("None"),
        WarpStyle::Custom => tl!("Custom"),
        WarpStyle::Arc => tl!("Arc"),
        WarpStyle::Arch => tl!("Arch"),
        WarpStyle::Bulge => tl!("Bulge"),
        WarpStyle::Flag => tl!("Flag"),
        WarpStyle::Inflate => tl!("Inflate"),
        _ => s.label(),
    }
}

/// One visual on the photo.
struct Item {
    origin: Origin,
    visual: Visual,
    /// Its corners on the photo (photo pixels, clockwise from the top left) and its warp.
    placement: Placement,
    lock: bool,
    thumb: Option<egui::ColorImage>,
    tex: Option<TextureHandle>,
    /// It drawn at the preview's size (unplaced), and that size.
    cache: Option<(i64, i64, Surface)>,
    /// It placed on the preview.
    placed: Option<Placed>,
    /// Which product zone masks it.
    zone: ZoneChoice,
}

/// The product zone that masks a visual.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ZoneChoice {
    /// The zone under its centre (none there: not masked).
    Auto,
    /// Not masked.
    Off,
    Zone(usize),
}

/// A product zone (a view of the garment): its name and coverage over the photo (0..255, the
/// photo's size), and the same on the preview.
struct Zone {
    name: String,
    mask: Vec<u8>,
    preview: Vec<u8>,
}

/// How a closed lasso changes the zones: a new zone, or added to / taken from the chosen one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ZoneMode {
    New,
    Add,
    Subtract,
}

/// The zones' colours on the photo.
const ZONE_COLORS: [Color32; 6] = [
    Color32::from_rgb(192, 58, 168),
    Color32::from_rgb(47, 111, 224),
    Color32::from_rgb(22, 140, 72),
    Color32::from_rgb(222, 120, 20),
    Color32::from_rgb(120, 70, 200),
    Color32::from_rgb(200, 40, 40),
];

fn zone_color(k: usize) -> Color32 {
    ZONE_COLORS[k % ZONE_COLORS.len()]
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn mid(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

/// The affine frame of a quad: its top left and its top and left sides.
struct Frame {
    o: [f64; 2],
    u: [f64; 2],
    v: [f64; 2],
}

impl Frame {
    fn of(c: &[[f64; 2]; 4]) -> Frame {
        Frame { o: c[0], u: [c[1][0] - c[0][0], c[1][1] - c[0][1]], v: [c[3][0] - c[0][0], c[3][1] - c[0][1]] }
    }

    fn to_local(&self, p: [f64; 2]) -> [f64; 2] {
        let det = self.u[0] * self.v[1] - self.u[1] * self.v[0];
        if det.abs() < 1e-9 {
            return [0.0, 0.0];
        }
        let (dx, dy) = (p[0] - self.o[0], p[1] - self.o[1]);
        [(dx * self.v[1] - dy * self.v[0]) / det, (self.u[0] * dy - self.u[1] * dx) / det]
    }

    fn from_local(&self, a: [f64; 2]) -> [f64; 2] {
        [self.o[0] + a[0] * self.u[0] + a[1] * self.v[0], self.o[1] + a[0] * self.u[1] + a[1] * self.v[1]]
    }
}

fn unit(v: [f64; 2]) -> [f64; 2] {
    let n = v[0].hypot(v[1]).max(1e-9);
    [v[0] / n, v[1] / n]
}

impl Item {
    fn corners(&self) -> [[f64; 2]; 4] {
        self.placement.corners
    }

    fn center(&self) -> [f64; 2] {
        let c = self.corners();
        [(c[0][0] + c[1][0] + c[2][0] + c[3][0]) / 4.0, (c[0][1] + c[1][1] + c[2][1] + c[3][1]) / 4.0]
    }

    /// Width and height in photo pixels: the mean of opposite sides.
    fn size_px(&self) -> (f64, f64) {
        let c = self.corners();
        ((dist(c[0], c[1]) + dist(c[3], c[2])) / 2.0, (dist(c[0], c[3]) + dist(c[1], c[2])) / 2.0)
    }

    /// Degrees, clockwise: the top side's direction.
    fn rotation(&self) -> f64 {
        let c = self.corners();
        (c[1][1] - c[0][1]).atan2(c[1][0] - c[0][0]).to_degrees()
    }

    fn translate(&mut self, d: [f64; 2]) {
        for p in &mut self.placement.corners {
            p[0] += d[0];
            p[1] += d[1];
        }
    }

    /// Scaled about its centre along its own sides.
    fn scale(&mut self, sx: f64, sy: f64) {
        let f = Frame::of(&self.placement.corners);
        let c = f.to_local(self.center());
        for p in &mut self.placement.corners {
            let a = f.to_local(*p);
            *p = f.from_local([c[0] + (a[0] - c[0]) * sx, c[1] + (a[1] - c[1]) * sy]);
        }
    }

    /// Turned `deg` clockwise about its centre.
    fn rotate(&mut self, deg: f64) {
        let (s, co) = deg.to_radians().sin_cos();
        let c = self.center();
        for p in &mut self.placement.corners {
            let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
            *p = [c[0] + dx * co - dy * s, c[1] + dx * s + dy * co];
        }
    }

    /// The warp's control grid (box units), `None` when nothing bends.
    fn grid(&self) -> Option<[[f64; 2]; 16]> {
        let (w, h) = self.placement.rendition_size();
        self.placement.bend.grid(w as f64, h as f64)
    }

    /// Its outline on the photo (photo pixels), bent.
    fn outline(&self) -> Vec<[f64; 2]> {
        let g = self.grid();
        let n = if g.is_some() { 16 } else { 1 };
        let mut pts = Vec::with_capacity(4 * n + 1);
        let edge = |k: usize, t: f64| match k {
            0 => (t, 0.0),
            1 => (1.0, t),
            2 => (1.0 - t, 1.0),
            _ => (0.0, 1.0 - t),
        };
        for k in 0..4 {
            for i in 0..n {
                let (u, v) = edge(k, i as f64 / n as f64);
                if let Some(p) = self.placement.map(g.as_ref(), u, v) {
                    pts.push(p);
                }
            }
        }
        if let Some(first) = pts.first().copied() {
            pts.push(first);
        }
        pts
    }

    /// Whether `q` (photo pixels) is inside its outline.
    fn contains(&self, q: [f64; 2]) -> bool {
        let poly = self.outline();
        let mut inside = false;
        for w in poly.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a[1] > q[1]) != (b[1] > q[1]) && q[0] < a[0] + (q[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                inside = !inside;
            }
        }
        inside
    }

    /// The handles of `tool` (photo pixels): for Warp the 16 control points, else the corners
    /// (0..4) then the sides' middles (4..8: top, right, bottom, left).
    fn handles(&self, tool: Tool) -> Vec<[f64; 2]> {
        let c = self.corners();
        match tool {
            Tool::Warp => {
                let g = self.grid().unwrap_or_else(cmd::rest_points);
                let Some(h) = Homography::rect_to_quad([0.0, 0.0, 1.0, 1.0], c) else { return Vec::new() };
                g.iter().map(|p| h.apply(p[0], p[1])).map(|(x, y)| [x, y]).collect()
            }
            _ => vec![c[0], c[1], c[2], c[3], mid(c[0], c[1]), mid(c[1], c[2]), mid(c[2], c[3]), mid(c[3], c[0])],
        }
    }

    /// Back to its own print size, upright, unbent, where it is.
    fn reset(&mut self, ppcm: f64) {
        let (w, h) = cmd::natural_size_cm(&self.visual);
        self.placement = Placement { corners: cmd::rect_corners(self.center(), w * ppcm, h * ppcm, 0.0), bend: Bend::default() };
    }
}

/// What a drag on a handle started from.
#[derive(Clone, Debug, PartialEq)]
struct HandleDrag {
    index: usize,
    tool: Tool,
    handle: usize,
    from: [f64; 2],
    start: Placement,
}

/// A drag of `tool`'s handle `handle` from `from` to `q` (photo pixels), on the placement it
/// started from; `prop` keeps the proportions, `centered` scales about the centre, `snap` turns
/// in 15° steps.
fn drag_handle(d: &HandleDrag, q: [f64; 2], prop: bool, centered: bool, snap: bool) -> Placement {
    let mut out = d.start.clone();
    let c0 = d.start.corners;
    let delta = [q[0] - d.from[0], q[1] - d.from[1]];
    let center = [(c0[0][0] + c0[1][0] + c0[2][0] + c0[3][0]) / 4.0, (c0[0][1] + c0[1][1] + c0[2][1] + c0[3][1]) / 4.0];
    match d.tool {
        Tool::Scale => {
            let f = Frame::of(&c0);
            let p: Vec<[f64; 2]> = c0.iter().map(|c| f.to_local(*c)).collect();
            let m = |a: usize, b: usize| [(p[a][0] + p[b][0]) / 2.0, (p[a][1] + p[b][1]) / 2.0];
            let handle = if d.handle < 4 { p[d.handle] } else { m(d.handle - 4, (d.handle - 3) % 4) };
            let anchor = if centered {
                f.to_local(center)
            } else if d.handle < 4 {
                p[(d.handle + 2) % 4]
            } else {
                m((d.handle - 2) % 4, (d.handle - 1) % 4)
            };
            let qq = f.to_local(q);
            let ratio = |i: usize| {
                let den = handle[i] - anchor[i];
                if den.abs() < 1e-9 { 1.0 } else { ((qq[i] - anchor[i]) / den).max(0.01) }
            };
            let (mut sx, mut sy) = match d.handle {
                4 | 6 => (1.0, ratio(1)),
                5 | 7 => (ratio(0), 1.0),
                _ => (ratio(0), ratio(1)),
            };
            if prop {
                let s = match d.handle {
                    4 | 6 => sy,
                    5 | 7 => sx,
                    _ => {
                        if (sx - 1.0).abs() > (sy - 1.0).abs() {
                            sx
                        } else {
                            sy
                        }
                    }
                };
                (sx, sy) = (s, s);
            }
            for (k, c) in out.corners.iter_mut().enumerate() {
                *c = f.from_local([anchor[0] + (p[k][0] - anchor[0]) * sx, anchor[1] + (p[k][1] - anchor[1]) * sy]);
            }
        }
        Tool::Rotate => {
            let a0 = (d.from[1] - center[1]).atan2(d.from[0] - center[0]);
            let a1 = (q[1] - center[1]).atan2(q[0] - center[0]);
            let mut deg = (a1 - a0).to_degrees();
            if snap {
                deg = (deg / 15.0).round() * 15.0;
            }
            let (s, co) = deg.to_radians().sin_cos();
            for (k, c) in out.corners.iter_mut().enumerate() {
                let (dx, dy) = (c0[k][0] - center[0], c0[k][1] - center[1]);
                *c = [center[0] + dx * co - dy * s, center[1] + dx * s + dy * co];
            }
        }
        Tool::Skew => {
            let (u, v) = (unit([c0[1][0] - c0[0][0], c0[1][1] - c0[0][1]]), unit([c0[3][0] - c0[0][0], c0[3][1] - c0[0][1]]));
            let along = |dir: [f64; 2]| {
                let t = delta[0] * dir[0] + delta[1] * dir[1];
                [dir[0] * t, dir[1] * t]
            };
            let (moved, by): (&[usize], [f64; 2]) = match d.handle {
                4 => (&[0, 1], along(u)),
                6 => (&[2, 3], along(u)),
                5 => (&[1, 2], along(v)),
                7 => (&[3, 0], along(v)),
                k => (
                    if k == 0 {
                        &[0]
                    } else if k == 1 {
                        &[1]
                    } else if k == 2 {
                        &[2]
                    } else {
                        &[3]
                    },
                    along(u),
                ),
            };
            for &k in moved {
                out.corners[k] = [c0[k][0] + by[0], c0[k][1] + by[1]];
            }
        }
        Tool::Distort => {
            let moved: &[usize] = match d.handle {
                4 => &[0, 1],
                5 => &[1, 2],
                6 => &[2, 3],
                7 => &[3, 0],
                0 => &[0],
                1 => &[1],
                2 => &[2],
                _ => &[3],
            };
            for &k in moved {
                out.corners[k] = [c0[k][0] + delta[0], c0[k][1] + delta[1]];
            }
        }
        Tool::Perspective => {
            if d.handle < 4 {
                let k = d.handle;
                let (u, v) = (unit([c0[1][0] - c0[0][0], c0[1][1] - c0[0][1]]), unit([c0[3][0] - c0[0][0], c0[3][1] - c0[0][1]]));
                let (du, dv) = (delta[0] * u[0] + delta[1] * u[1], delta[0] * v[0] + delta[1] * v[1]);
                let (dir, t, partner) = if du.abs() >= dv.abs() { (u, du, [1, 0, 3, 2][k]) } else { (v, dv, [3, 2, 1, 0][k]) };
                out.corners[k] = [c0[k][0] + dir[0] * t, c0[k][1] + dir[1] * t];
                out.corners[partner] = [c0[partner][0] - dir[0] * t, c0[partner][1] - dir[1] * t];
            }
        }
        Tool::Warp => {
            let Some(inv) = Homography::rect_to_quad([0.0, 0.0, 1.0, 1.0], c0).and_then(|h| h.inverse()) else { return out };
            let (w, h) = d.start.rendition_size();
            let mut pts = d.start.bend.grid(w as f64, h as f64).unwrap_or_else(cmd::rest_points);
            let (a, b) = (inv.apply(d.from[0], d.from[1]), inv.apply(q[0], q[1]));
            if let Some(p) = pts.get_mut(d.handle) {
                p[0] += b.0 - a.0;
                p[1] += b.1 - a.1;
            }
            out.bend = Bend { style: WarpStyle::Custom, bend: 0.0, vertical: d.start.bend.vertical, points: Some(pts) };
        }
    }
    out
}

/// The loaded photo: its file and size, the preview's reduced copy and its light.
struct PhotoState {
    path: String,
    name: String,
    width: u32,
    height: u32,
    preview: Photo,
    /// The preview's pixels per photo pixel.
    k: f64,
    light: Light,
    /// The photo itself (Select the Product works on it).
    full: Photo,
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    /// Drawing the reference line from its first end.
    Line,
    /// Moving an end of the line.
    End(usize),
    /// Moving a visual, grabbed this far from its centre (photo pixels).
    Move { index: usize, grab: [f64; 2] },
    /// A transform tool's handle.
    Handle(HandleDrag),
}

pub struct MockupDialog {
    photo: Option<PhotoState>,
    /// The reference line's ends, in photo pixels.
    line: Option<[[f64; 2]; 2]>,
    /// Its real length.
    length_cm: f32,
    /// The scale the visuals were last sized at: when it changes they keep their size in cm.
    sized_at: Option<f64>,
    /// The next drag on the photo draws the line.
    drawing: bool,
    items: Vec<Item>,
    selected: Option<usize>,
    /// The transform tool in use on the selected visual (none: a drag moves it).
    tool: Option<Tool>,
    shadows: f32,
    folds: f32,
    opacity: f32,
    jpg: bool,
    closed: [bool; 4],
    status: String,
    drag: Option<Drag>,
    /// The preview needs rendering again.
    dirty: bool,
    shown: Option<TextureHandle>,
    /// The product zones, the chosen one (Add / Subtract, Invert), how a lasso changes them.
    zones: Vec<Zone>,
    zone_sel: Option<usize>,
    zone_mode: ZoneMode,
    /// The polygonal lasso is on; its points so far (photo pixels).
    lassoing: bool,
    lasso: Vec<[f64; 2]>,
    /// The zones' edges drawn over the preview, rebuilt when they change.
    overlay: Option<TextureHandle>,
    overlay_dirty: bool,
    /// The zoomed view (none: the photo fitted) and the canvas it was last drawn in.
    view: Option<View>,
    canvas_rect: Option<ERect>,
    /// Zoom with Scroll Wheel, Zoom with Trackpad Pinch, Animated Zoom, Enable Gestures (the
    /// canvas's preferences).
    nav: [bool; 4],
}

impl MockupDialog {
    fn new() -> Self {
        MockupDialog {
            photo: None,
            line: None,
            length_cm: 50.0,
            sized_at: None,
            drawing: false,
            items: Vec::new(),
            selected: None,
            tool: None,
            shadows: cmd::DEFAULT_SHADOWS as f32,
            folds: cmd::DEFAULT_FOLDS as f32,
            opacity: cmd::DEFAULT_OPACITY as f32,
            jpg: false,
            closed: [false, false, false, true],
            status: String::new(),
            drag: None,
            dirty: true,
            shown: None,
            zones: Vec::new(),
            zone_sel: None,
            zone_mode: ZoneMode::New,
            lassoing: false,
            lasso: Vec::new(),
            overlay: None,
            overlay_dirty: true,
            view: None,
            canvas_rect: None,
            nav: [false, true, true, true],
        }
    }

    /// The photo's pixels per cm, once the line and its length are given.
    fn ppcm(&self) -> Option<f64> {
        let [a, b] = self.line?;
        cmd::pixels_per_cm(a, b, f64::from(self.length_cm))
    }

    /// A new scale: the visuals keep their size in cm (scaled about their centres).
    fn keep_sizes(&mut self) {
        let now = self.ppcm();
        if let (Some(old), Some(new)) = (self.sized_at, now)
            && (old - new).abs() > 1e-9
        {
            let k = new / old;
            for it in &mut self.items {
                it.scale(k, k);
            }
            self.dirty = true;
        }
        if now.is_some() {
            self.sized_at = now;
        }
    }

    fn render(&self) -> Render {
        Render { shadows: self.shadows / 100.0, folds: self.folds / 100.0, opacity: self.opacity / 100.0 }
    }

    /// A photo in place of the current one: the scale starts again (drawing armed), the visuals
    /// stay, at the same place on the new photo.
    fn set_photo(&mut self, path: &str, photo: Photo) {
        let (w, h) = (photo.width, photo.height);
        if let Some(old) = &self.photo {
            let (kx, ky) = (f64::from(w) / f64::from(old.width.max(1)), f64::from(h) / f64::from(old.height.max(1)));
            for it in &mut self.items {
                for p in &mut it.placement.corners {
                    *p = [p[0] * kx, p[1] * ky];
                }
            }
        }
        let (preview, k) = photo.reduced(PREVIEW_SIDE);
        let light = Light::new(&preview.rgba, preview.width as usize, preview.height as usize);
        self.photo = Some(PhotoState { path: path.to_string(), name: photo.name.clone(), width: w, height: h, preview, k, light, full: photo });
        self.line = None;
        self.sized_at = None;
        // The zones were drawn on the old photo.
        self.zones.clear();
        self.zone_sel = None;
        self.lasso.clear();
        self.lassoing = false;
        self.overlay_dirty = true;
        self.view = None;
        for it in &mut self.items {
            it.zone = ZoneChoice::Auto;
        }
        self.drawing = true;
        self.status = tl!("Draw the reference line on the photo").to_string();
        self.dirty = true;
    }

    /// Where a new visual lands: on the middle of the line (else the photo's), after the others.
    fn landing(&self) -> [f64; 2] {
        let Some(ph) = &self.photo else { return [0.0, 0.0] };
        let base = match self.line {
            Some([a, b]) => mid(a, b),
            None => [f64::from(ph.width) / 2.0, f64::from(ph.height) / 2.0],
        };
        let step = self.ppcm().unwrap_or(0.0) * CASCADE_CM * self.items.len() as f64;
        [(base[0] + step).min(f64::from(ph.width)), (base[1] + step).min(f64::from(ph.height))]
    }

    fn add_visual(&mut self, origin: Origin, v: Visual) -> Result<(), String> {
        let Some(ppcm) = self.ppcm() else { return Err(tl!("Set the scale first").to_string()) };
        if self.items.len() >= MAX_ITEMS {
            return Err(crate::i18n::fmt(tl!("At most {n} visuals"), &[("n", &MAX_ITEMS.to_string())]));
        }
        let (w, h) = cmd::natural_size_cm(&v);
        let corners = cmd::rect_corners(self.landing(), w * ppcm, h * ppcm, 0.0);
        self.items.push(Item {
            origin,
            thumb: Some(preview_image(&v)),
            visual: v,
            placement: Placement { corners, bend: Bend::default() },
            lock: true,
            tex: None,
            cache: None,
            placed: None,
            zone: ZoneChoice::Auto,
        });
        self.sized_at = Some(ppcm);
        self.selected = Some(self.items.len() - 1);
        self.closed[SECTION_VISUAL] = false;
        self.dirty = true;
        Ok(())
    }

    fn remove(&mut self, i: usize) {
        if i < self.items.len() {
            self.items.remove(i);
            self.selected = match self.selected {
                Some(s) if s == i => None,
                Some(s) if s > i => Some(s - 1),
                s => s,
            };
            if self.selected.is_none() {
                self.tool = None;
            }
            self.dirty = true;
        }
    }

    /// The visual's centre on the reference line's middle (left to right), else the photo's.
    fn center(&mut self, i: usize) {
        let x = match (self.line, &self.photo) {
            (Some([a, b]), _) => (a[0] + b[0]) / 2.0,
            (None, Some(ph)) => f64::from(ph.width) / 2.0,
            _ => return,
        };
        if let Some(it) = self.items.get_mut(i) {
            let c = it.center();
            it.translate([x - c[0], 0.0]);
            self.dirty = true;
        }
    }

    /// A transform tool on the selected visual (the last one when none is).
    fn set_tool(&mut self, tool: Option<Tool>) {
        if tool.is_some() && self.selected.is_none() && !self.items.is_empty() {
            self.selected = Some(self.items.len() - 1);
        }
        self.tool = tool.filter(|_| self.selected.is_some());
        if self.tool.is_some() {
            self.closed[SECTION_TRANSFORM] = false;
        }
    }

    /// The selected visual back to its own size, upright and unbent.
    fn reset(&mut self, i: usize) {
        let k = self.ppcm().unwrap_or(1.0);
        if let Some(it) = self.items.get_mut(i) {
            it.reset(k);
            self.dirty = true;
        }
    }

    /// The top-most visual under `q` (photo pixels).
    fn hit(&self, q: [f64; 2]) -> Option<usize> {
        (0..self.items.len()).rev().find(|i| self.items[*i].contains(q))
    }

    /// The zone masking item `i`: its own choice, else the zone under its centre.
    fn zone_of(&self, i: usize) -> Option<usize> {
        let it = self.items.get(i)?;
        match it.zone {
            ZoneChoice::Off => None,
            ZoneChoice::Zone(k) => (k < self.zones.len()).then_some(k),
            ZoneChoice::Auto => {
                let ph = self.photo.as_ref()?;
                let masks: Vec<&[u8]> = self.zones.iter().map(|z| z.mask.as_slice()).collect();
                cmd::zone_at(&masks, ph.width, ph.height, it.center())
            }
        }
    }

    /// A new zone (full-size coverage); its name the next free "Zone n".
    fn push_zone(&mut self, mask: Vec<u8>) {
        let Some(ph) = &self.photo else { return };
        if self.zones.len() >= cmd::MAX_ZONES || !mask.iter().any(|v| *v >= 128) {
            return;
        }
        let preview = cmd::reduce_zone(&mask, ph.width, ph.height, ph.preview.width, ph.preview.height);
        let mut n = self.zones.len() + 1;
        while self.zones.iter().any(|z| z.name == zone_default_name(n)) {
            n += 1;
        }
        self.zones.push(Zone { name: zone_default_name(n), mask, preview });
        self.zone_sel = Some(self.zones.len() - 1);
        self.zones_changed();
    }

    /// The zone `k`'s coverage replaced (empty: the zone goes).
    fn set_zone_mask(&mut self, k: usize, mask: Vec<u8>) {
        let Some(ph) = &self.photo else { return };
        if !mask.iter().any(|v| *v >= 128) {
            self.remove_zone(k);
            return;
        }
        let preview = cmd::reduce_zone(&mask, ph.width, ph.height, ph.preview.width, ph.preview.height);
        if let Some(z) = self.zones.get_mut(k) {
            z.mask = mask;
            z.preview = preview;
        }
        self.zones_changed();
    }

    fn remove_zone(&mut self, k: usize) {
        if k >= self.zones.len() {
            return;
        }
        self.zones.remove(k);
        for it in &mut self.items {
            it.zone = match it.zone {
                ZoneChoice::Zone(j) if j == k => ZoneChoice::Auto,
                ZoneChoice::Zone(j) if j > k => ZoneChoice::Zone(j - 1),
                z => z,
            };
        }
        self.zone_sel = match self.zone_sel {
            Some(s) if s == k => None,
            Some(s) if s > k => Some(s - 1),
            s => s,
        };
        self.zones_changed();
    }

    fn clear_zones(&mut self) {
        while !self.zones.is_empty() {
            self.remove_zone(self.zones.len() - 1);
        }
    }

    /// The zone `k` turned inside out (over the photo).
    fn invert_zone(&mut self, k: usize) {
        if let Some(z) = self.zones.get(k) {
            let inv: Vec<u8> = z.mask.iter().map(|v| 255 - v).collect();
            self.set_zone_mask(k, inv);
        }
    }

    /// The closed lasso applied as the mode says.
    fn close_lasso(&mut self) {
        let pts = std::mem::take(&mut self.lasso);
        // One zone a lasso: the button turns off.
        self.lassoing = false;
        let Some(ph) = &self.photo else { return };
        if pts.len() < 3 {
            return;
        }
        let poly = cmd::zone_from_polygon(&pts, ph.width, ph.height);
        match (self.zone_mode, self.zone_sel.filter(|k| *k < self.zones.len())) {
            (ZoneMode::Add, Some(k)) | (ZoneMode::Subtract, Some(k)) => {
                let mut m = self.zones[k].mask.clone();
                cmd::combine_zone(&mut m, &poly, self.zone_mode == ZoneMode::Subtract);
                self.set_zone_mask(k, m);
            }
            (ZoneMode::Subtract, None) => {}
            _ => self.push_zone(poly),
        }
    }

    /// The product(s) found on the photo, one zone each, in place of the zones.
    fn auto_zones(&mut self) -> usize {
        let Some(ph) = &self.photo else { return 0 };
        let found = cmd::product_zones(&ph.full);
        self.clear_zones();
        let n = found.len();
        for z in found {
            self.push_zone(z);
        }
        n
    }

    fn zones_changed(&mut self) {
        self.overlay_dirty = true;
        self.dirty = true;
    }

    /// The zones' edges (and the chosen zone lightly tinted) at the preview's size.
    fn refresh_overlay(&mut self, ctx: &egui::Context) {
        if !self.overlay_dirty {
            return;
        }
        self.overlay_dirty = false;
        let Some(ph) = &self.photo else {
            self.overlay = None;
            return;
        };
        if self.zones.is_empty() {
            self.overlay = None;
            return;
        }
        let (w, h) = (ph.preview.width as usize, ph.preview.height as usize);
        let mut px = vec![Color32::TRANSPARENT; w * h];
        // The edge a few preview pixels thick: still a line once the photo is fitted on screen.
        let r = (w.max(h) / 500).max(1);
        for (k, z) in self.zones.iter().enumerate() {
            let c = zone_color(k);
            let on = self.zone_sel == Some(k);
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    if z.preview[i] < 128 {
                        continue;
                    }
                    let edge = x < r
                        || y < r
                        || x + r >= w
                        || y + r >= h
                        || z.preview[i - r] < 128
                        || z.preview[i + r] < 128
                        || z.preview[i - r * w] < 128
                        || z.preview[i + r * w] < 128;
                    if edge {
                        px[i] = c;
                    } else if on {
                        px[i] = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 12);
                    }
                }
            }
        }
        let img = egui::ColorImage { size: [w, h], source_size: egui::vec2(w as f32, h as f32), pixels: px };
        match &mut self.overlay {
            Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
            None => self.overlay = Some(ctx.load_texture("mockup-zones", img, egui::TextureOptions::LINEAR)),
        }
    }

    /// The preview rendered again, if anything changed.
    fn refresh(&mut self, ctx: &egui::Context) {
        self.refresh_overlay(ctx);
        for (i, it) in self.items.iter_mut().enumerate() {
            if let Some(img) = it.thumb.take() {
                it.tex = Some(ctx.load_texture(format!("mockup-thumb-{i}-{}", it.visual.name), img, egui::TextureOptions::LINEAR));
            }
        }
        self.keep_sizes();
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let ppcm = self.ppcm();
        let Some(ph) = &self.photo else {
            self.shown = None;
            return;
        };
        let pp = ppcm.unwrap_or(0.0) * ph.k;
        let mut errors = Vec::new();
        for it in &mut self.items {
            it.placed = None;
            if pp <= 0.0 {
                continue;
            }
            // On the preview: the corners scaled down with the photo.
            let at = Placement { corners: it.placement.corners.map(|p| [p[0] * ph.k, p[1] * ph.k]), bend: it.placement.bend.clone() };
            let (w, h) = at.rendition_size();
            if it.cache.as_ref().is_none_or(|c| (c.0, c.1) != (w, h)) {
                it.cache = match cmd::rendition(&it.visual, w, h) {
                    Ok(s) => Some((w, h, s)),
                    Err(e) => {
                        errors.push(e.to_string());
                        None
                    }
                };
            }
            if let Some((w, h, src)) = &it.cache {
                match cmd::place_rendition(src, *w, *h, &at, ph.preview.width, ph.preview.height) {
                    Ok(p) => it.placed = p,
                    Err(e) => errors.push(e.to_string()),
                }
            }
        }
        let zone_of: Vec<Option<usize>> = (0..self.items.len()).map(|i| self.zone_of(i)).collect();
        let prints: Vec<Print> = self
            .items
            .iter()
            .zip(&zone_of)
            .filter_map(|(it, z)| it.placed.as_ref().map(|p| Print::from(p).in_zone(z.and_then(|k| self.zones.get(k)).map(|z| z.preview.as_slice()))))
            .collect();
        let (w, h) = (ph.preview.width as usize, ph.preview.height as usize);
        let out = cmd::composite(&ph.preview.rgba, w, h, &ph.light, &prints, self.render(), pp);
        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &out);
        match &mut self.shown {
            Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
            None => self.shown = Some(ctx.load_texture("mockup-preview", img, egui::TextureOptions::LINEAR)),
        }
        if !errors.is_empty() {
            self.status = errors.join(" · ");
        }
    }

    /// The export's params (without `output`).
    fn params(&self, app: &PhotocraftApp) -> Result<Value, String> {
        let ph = self.photo.as_ref().ok_or(tl!("Load a photo first"))?;
        let ppcm = self.ppcm().ok_or(tl!("Set the scale first"))?;
        if self.items.is_empty() {
            return Err(tl!("Add a visual first").to_string());
        }
        let mut visuals = Vec::new();
        let zone_of: Vec<Option<usize>> = (0..self.items.len()).map(|i| self.zone_of(i)).collect();
        for (k, it) in self.items.iter().enumerate() {
            let mut v = match &it.origin {
                Origin::File(path) => json!({"path": path}),
                Origin::Document(id) => {
                    let i = app
                        .session
                        .documents()
                        .iter()
                        .position(|d| d.doc.id == *id)
                        .ok_or_else(|| format!("{}: {}", it.visual.name, tl!("the document was closed")))?;
                    json!({"document": i})
                }
            };
            v["corners"] = json!(it.placement.corners);
            v["warp"] = bend_json(&it.placement.bend);
            v["zone"] = json!(zone_of[k]);
            visuals.push(v);
        }
        Ok(json!({
            "photo": ph.path,
            "pixelsPerCm": ppcm,
            "visuals": visuals,
            "shadows": self.shadows,
            "folds": self.folds,
            "opacity": self.opacity,
            "name": format!("{} mockup", ph.name),
        }))
    }

    fn describe(&self) -> Value {
        let k = self.ppcm().unwrap_or(1.0);
        json!({
            "photo": self.photo.as_ref().map(|p| json!({"name": p.name, "pixels": [p.width, p.height]})),
            "line": self.line,
            "lengthCm": self.length_cm,
            "pixelsPerCm": self.ppcm(),
            "drawing": self.drawing,
            "visuals": self.items.iter().enumerate().map(|(i, it)| {
                let (w, h) = it.size_px();
                let c = it.center();
                let choice = match it.zone { ZoneChoice::Auto => json!("auto"), ZoneChoice::Off => json!("none"), ZoneChoice::Zone(z) => json!(z) };
                json!({"name": it.visual.name, "x": c[0], "y": c[1], "widthCm": w / k, "heightCm": h / k, "lock": it.lock, "rotation": it.rotation(), "corners": it.placement.corners, "warp": bend_json(&it.placement.bend), "zoneChoice": choice, "zone": self.zone_of(i)})
            }).collect::<Vec<_>>(),
            "zones": self.zones.iter().map(|z| json!({"name": z.name})).collect::<Vec<_>>(),
            "zoneSelected": self.zone_sel,
            "lasso": {"on": self.lassoing, "points": self.lasso.len()},
            "selected": self.selected,
            "tool": self.tool.map(Tool::id),
            "render": {"shadows": self.shadows, "folds": self.folds, "opacity": self.opacity},
            "format": if self.jpg { "jpg" } else { "png" },
            "status": self.status,
        })
    }
}

fn zone_default_name(n: usize) -> String {
    crate::i18n::fmt(tl!("Zone {n}"), &[("n", &n.to_string())])
}

fn bend_json(b: &Bend) -> Value {
    let mut v = json!({"style": b.style.id(), "bend": b.bend, "vertical": b.vertical});
    if let Some(p) = b.points {
        v["points"] = json!(p);
    }
    v
}

// ---------------------------------------------------------------- opening, files, export

pub fn open(app: &mut PhotocraftApp) {
    if app.mockup.is_none() {
        app.mockup = Some(MockupDialog::new());
        // In beta: its window, the first time in this version.
        crate::beta::opened(app, cmd::MOCKUP);
    }
}

fn set_status(app: &mut PhotocraftApp, msg: String) {
    if let Some(d) = app.mockup.as_mut() {
        d.status = msg;
    }
}

/// The garment's photo, from a PNG or JPG file.
pub fn load_photo(app: &mut PhotocraftApp, path: &str) {
    match cmd::load_photo(path) {
        Ok(p) => {
            if let Some(d) = app.mockup.as_mut() {
                d.set_photo(path, p);
            }
        }
        Err(e) => set_status(app, e.to_string()),
    }
}

/// The screenshot on the clipboard as the photo (kept as a PNG in the temporary folder, for the
/// export to read).
pub fn paste_photo(app: &mut PhotocraftApp) {
    let Some(get) = app.services.clipboard_get_image.as_mut() else {
        set_status(app, tl!("The clipboard can't be read here").to_string());
        return;
    };
    let image = match get() {
        Ok(Some((w, h, bytes))) => (w, h, bytes),
        Ok(None) => {
            set_status(app, tl!("The clipboard holds no image").to_string());
            return;
        }
        Err(e) => {
            set_status(app, e);
            return;
        }
    };
    let (w, h, bytes) = image;
    let r = cmd::photo_from_rgba(tl!("Screenshot"), w, h, bytes).and_then(|photo| {
        static PASTES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = PASTES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("dtfcraft-mockup-{}-{n}.png", std::process::id())).to_string_lossy().into_owned();
        cmd::write_photo_png(&photo, &path)?;
        Ok((path, photo))
    });
    match r {
        Ok((path, photo)) => {
            if let Some(d) = app.mockup.as_mut() {
                d.set_photo(&path, photo);
            }
        }
        Err(e) => set_status(app, e.to_string()),
    }
}

/// The open document's selection as zones (one per object): the document must be the photo's size.
pub fn zones_from_document(app: &mut PhotocraftApp) -> Result<usize, String> {
    let st = app.session.active().ok_or(tl!("Open a document first"))?;
    let d = app.mockup.as_ref().ok_or("Mockup isn't open")?;
    let ph = d.photo.as_ref().ok_or(tl!("Load a photo first"))?;
    if (st.doc.size.width, st.doc.size.height) != (ph.width, ph.height) {
        return Err(crate::i18n::fmt(
            tl!("The open document must be the photo's size ({w} × {h} px)"),
            &[("w", &ph.width.to_string()), ("h", &ph.height.to_string())],
        ));
    }
    let zone = cmd::selection_zone(&st.doc).ok_or(tl!("The open document has no selection"))?;
    let (w, h) = (ph.width, ph.height);
    let parts = cmd::split_objects(&zone, w, h);
    let n = parts.len();
    if let Some(d) = app.mockup.as_mut() {
        for z in parts {
            d.push_zone(z);
        }
    }
    Ok(n)
}

/// PNG and SVG visuals from disk; failures go to the status line.
pub fn add_files(app: &mut PhotocraftApp, paths: &[String]) {
    let Some(d) = app.mockup.as_mut() else { return };
    let mut errors = Vec::new();
    for path in paths {
        match gang::load_file(path) {
            Ok(v) => {
                if let Err(e) = d.add_visual(Origin::File(path.clone()), v) {
                    errors.push(e);
                    break;
                }
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    if !errors.is_empty() {
        d.status = errors.join(" · ");
    }
}

/// The active document, flattened, as a visual.
pub fn add_document(app: &mut PhotocraftApp) -> Result<(), String> {
    let st = app.session.active().ok_or(tl!("Open a document first"))?;
    let v = gang::load_document(&st.doc, &st.doc.name).map_err(|e| e.to_string())?;
    let id = st.doc.id;
    app.mockup.as_mut().map_or(Ok(()), |d| d.add_visual(Origin::Document(id), v))
}

/// Files dropped on the window: the photo first (PNG or JPG), then the visuals (PNG or SVG).
pub fn drop_files(app: &mut PhotocraftApp, files: &[egui::DroppedFileHandle]) {
    let paths: Vec<String> = files.iter().map(|f| f.path()).filter(|p| p.is_absolute()).map(|p| p.to_string_lossy().into_owned()).collect();
    let mut visuals = Vec::new();
    for path in paths {
        let photo_first = app.mockup.as_ref().is_some_and(|d| d.photo.is_none());
        let jpg = matches!(std::path::Path::new(&path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("jpg" | "jpeg"));
        if cmd::is_photo_file(&path) && (photo_first || jpg) {
            load_photo(app, &path);
        } else {
            visuals.push(path);
        }
    }
    add_files(app, &visuals);
}

fn pick_photo(app: &mut PhotocraftApp) {
    let initial_dir = crate::file_dialog::last_used_dir(&app.ui.recent_files);
    let r = app.ask_file(
        FileDialogRequest::Open { multiple: false, initial_dir, extensions: Some(vec!["png".into(), "jpg".into(), "jpeg".into()]) },
        |app, answer| match answer {
            FileDialogAnswer::Paths(paths) => {
                if let Some(p) = paths.first() {
                    load_photo(app, p);
                }
                Ok(Value::Null)
            }
            _ => Err("The photo is opened from a folder on this computer".to_string()),
        },
    );
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
    {
        set_status(app, e);
    }
}

fn pick_visuals(app: &mut PhotocraftApp) {
    let initial_dir = crate::file_dialog::last_used_dir(&app.ui.recent_files);
    let r = app.ask_file(
        FileDialogRequest::Open { multiple: true, initial_dir, extensions: Some(vec!["png".into(), "svg".into(), "svgz".into()]) },
        |app, answer| match answer {
            FileDialogAnswer::Paths(paths) => {
                add_files(app, &paths);
                Ok(Value::Null)
            }
            _ => Err("PNG and SVG files are added from a folder on this computer".to_string()),
        },
    );
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
    {
        set_status(app, e);
    }
}

/// Renders the mockup at the photo's resolution, writes it to `path` and opens it; the window
/// closes on success.
fn export_to(app: &mut PhotocraftApp, path: &str) -> Result<Value, String> {
    let d = app.mockup.as_ref().ok_or("Mockup isn't open")?;
    let mut params = d.params(app)?;
    // The zones go to the render as PNG masks in the temporary folder.
    if let Some(ph) = &d.photo
        && !d.zones.is_empty()
    {
        let mut paths = Vec::new();
        for (k, z) in d.zones.iter().enumerate() {
            let path = std::env::temp_dir().join(format!("dtfcraft-mockup-zone-{}-{k}.png", std::process::id())).to_string_lossy().into_owned();
            cmd::write_zone_png(&z.mask, ph.width, ph.height, &path).map_err(|e| e.to_string())?;
            paths.push(path);
        }
        params["zones"] = json!(paths);
    }
    params["output"] = json!(path);
    match app.run(cmd::MOCKUP, params) {
        Ok(v) => {
            app.mockup = None;
            Ok(v)
        }
        Err(e) => {
            set_status(app, e.clone());
            Err(e)
        }
    }
}

fn export(app: &mut PhotocraftApp) {
    let Some(d) = app.mockup.as_ref() else { return };
    if let Err(e) = d.params(app) {
        set_status(app, e);
        return;
    }
    let name = d.photo.as_ref().map_or("Mockup".to_string(), |p| format!("{} mockup", p.name));
    let suggested = format!("{name}.{}", if d.jpg { "jpg" } else { "png" });
    let r = app.pick_save(&suggested, |app, path| export_to(app, &path));
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
    {
        set_status(app, e);
    }
}

// ---------------------------------------------------------------- control channel

/// Menu / control routing. `Some` when this module handles the invocation.
pub fn menu(app: &mut PhotocraftApp, _ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id != cmd::MOCKUP {
        return None;
    }
    if params.as_object().is_none_or(|o| o.is_empty()) {
        open(app);
        return Some(Ok(app.mockup.as_ref().map(MockupDialog::describe).unwrap_or(Value::Null)));
    }
    let ui = params.get("ui")?;
    open(app);
    Some(control(app, ui))
}

fn point(v: Option<&Value>) -> Option<[f64; 2]> {
    let a = v?.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
}

fn control(app: &mut PhotocraftApp, ui: &Value) -> Result<Value, String> {
    let f32_of = |v: Option<&Value>| v.and_then(Value::as_f64).filter(|f| f.is_finite()).map(|f| f as f32);
    if let Some(path) = ui.get("photo").and_then(Value::as_str) {
        load_photo(app, path);
    }
    if let Some(sc) = ui.get("scale")
        && let Some(d) = app.mockup.as_mut()
    {
        if let (Some(a), Some(b)) = (point(sc.get("from")), point(sc.get("to"))) {
            d.line = Some([a, b]);
            d.drawing = false;
        }
        if let Some(cm) = f32_of(sc.get("cm")) {
            d.length_cm = cm.clamp(0.1, 1000.0);
        }
        if d.ppcm().is_some() {
            d.status.clear();
        }
        d.dirty = true;
    }
    if let Some(on) = ui.get("draw").and_then(Value::as_bool)
        && let Some(d) = app.mockup.as_mut()
    {
        d.drawing = on && d.photo.is_some();
    }
    if let Some(paths) = ui.get("addFiles").and_then(Value::as_array) {
        let paths: Vec<String> = paths.iter().filter_map(Value::as_str).map(str::to_string).collect();
        add_files(app, &paths);
    }
    if ui.get("addDocument").and_then(Value::as_bool) == Some(true)
        && let Err(e) = add_document(app)
    {
        set_status(app, e);
    }
    if ui.get("paste").and_then(Value::as_bool) == Some(true) {
        paste_photo(app);
    }
    if ui.get("zonesFromDocument").and_then(Value::as_bool) == Some(true)
        && let Err(e) = zones_from_document(app)
    {
        set_status(app, e);
    }
    if let Some(z) = ui.get("zones")
        && let Some(d) = app.mockup.as_mut()
    {
        if z.get("auto").and_then(Value::as_bool) == Some(true) {
            let n = d.auto_zones();
            d.status = crate::i18n::fmt(tl!("{n} zones found"), &[("n", &n.to_string())]);
        }
        if let Some(m) = z.get("mode").and_then(Value::as_str) {
            d.zone_mode = match m {
                "add" => ZoneMode::Add,
                "subtract" => ZoneMode::Subtract,
                _ => ZoneMode::New,
            };
        }
        if let Some(k) = z.get("select") {
            d.zone_sel = k.as_u64().map(|k| k as usize).filter(|k| *k < d.zones.len());
            d.overlay_dirty = true;
        }
        if let Some(pts) = z.get("lasso").and_then(Value::as_array) {
            d.lasso = pts.iter().filter_map(|p| point(Some(p))).collect();
            d.close_lasso();
        }
        if let Some(on) = z.get("lassoOn").and_then(Value::as_bool) {
            d.lassoing = on && d.photo.is_some();
            d.lasso.clear();
        }
        if let Some(k) = z.get("invert").and_then(Value::as_u64) {
            d.invert_zone(k as usize);
        }
        if let Some(k) = z.get("remove").and_then(Value::as_u64) {
            d.remove_zone(k as usize);
        }
        if z.get("clear").and_then(Value::as_bool) == Some(true) {
            d.clear_zones();
        }
        if let (Some(k), Some(name)) =
            (z.get("rename").and_then(|r| r.get("index")).and_then(Value::as_u64), z.get("rename").and_then(|r| r.get("name")).and_then(Value::as_str))
            && let Some(zone) = d.zones.get_mut(k as usize)
        {
            zone.name = name.to_string();
        }
    }
    let Some(d) = app.mockup.as_mut() else { return Ok(Value::Null) };
    if let Some(sel) = ui.get("select") {
        d.selected = sel.as_u64().map(|v| v as usize).filter(|i| *i < d.items.len());
    }
    if let Some(v) = ui.get("visual") {
        let i = v.get("index").and_then(Value::as_u64).map(|v| v as usize).or(d.selected).ok_or("no visual")?;
        let k = d.ppcm().unwrap_or(1.0);
        let it = d.items.get_mut(i).ok_or("no such visual")?;
        if let Some(l) = v.get("lock").and_then(Value::as_bool) {
            it.lock = l;
        }
        if let Some(c) = v.get("corners").and_then(Value::as_array) {
            let pts: Vec<[f64; 2]> = c.iter().filter_map(|p| point(Some(p))).collect();
            if pts.len() == 4 {
                it.placement.corners = [pts[0], pts[1], pts[2], pts[3]];
            }
        }
        let (w, h) = it.size_px();
        let w_cm = f32_of(v.get("widthCm")).map(|x| f64::from(x.clamp(0.1, 500.0)));
        let h_cm = f32_of(v.get("heightCm")).map(|x| f64::from(x.clamp(0.1, 500.0)));
        match (w_cm, h_cm) {
            (Some(wc), Some(hc)) => it.scale(wc * k / w.max(1e-9), hc * k / h.max(1e-9)),
            (Some(wc), None) => {
                let sx = wc * k / w.max(1e-9);
                it.scale(sx, if it.lock { sx } else { 1.0 });
            }
            (None, Some(hc)) => {
                let sy = hc * k / h.max(1e-9);
                it.scale(if it.lock { sy } else { 1.0 }, sy);
            }
            (None, None) => {}
        }
        if let Some(r) = v.get("rotation").and_then(Value::as_f64).filter(|r| r.is_finite()) {
            let now = it.rotation();
            it.rotate(r.clamp(-180.0, 180.0) - now);
        }
        if let (Some(x), Some(y)) = (v.get("x").and_then(Value::as_f64), v.get("y").and_then(Value::as_f64)) {
            let c = it.center();
            it.translate([x - c[0], y - c[1]]);
        }
        if let Some(z) = v.get("zone") {
            it.zone = match z {
                Value::Number(n) => ZoneChoice::Zone(n.as_u64().unwrap_or(0) as usize),
                Value::String(s) if s == "none" => ZoneChoice::Off,
                _ => ZoneChoice::Auto,
            };
        }
        if let Some(w) = v.get("warp") {
            let mut b = cmd::Bend::from_json(w).map_err(|e| e.to_string())?;
            if b.style.is_preset() && b.bend == 0.0 && w.get("bend").is_none() {
                b.bend = DEFAULT_BEND;
            }
            it.placement.bend = b;
        }
        d.selected = Some(i);
        d.dirty = true;
    }
    if let Some(t) = ui.get("tool") {
        d.set_tool(t.as_str().and_then(Tool::parse));
    }
    if ui.get("reset").and_then(Value::as_bool) == Some(true)
        && let Some(i) = d.selected
    {
        d.reset(i);
    }
    if ui.get("center").and_then(Value::as_bool) == Some(true)
        && let Some(i) = d.selected
    {
        d.center(i);
    }
    if let Some(i) = ui.get("remove").and_then(Value::as_u64) {
        d.remove(i as usize);
    }
    if let Some(r) = ui.get("render") {
        for (k, slot) in [("shadows", &mut d.shadows), ("folds", &mut d.folds), ("opacity", &mut d.opacity)] {
            if let Some(v) = f32_of(r.get(k)) {
                *slot = v.clamp(0.0, 100.0);
            }
        }
        d.dirty = true;
    }
    if let Some(f) = ui.get("format").and_then(Value::as_str) {
        d.jpg = f.eq_ignore_ascii_case("jpg") || f.eq_ignore_ascii_case("jpeg");
    }
    if let Some(path) = ui.get("export").and_then(Value::as_str) {
        return export_to(app, path);
    }
    if ui.get("cancel").and_then(Value::as_bool) == Some(true) {
        app.mockup = None;
        return Ok(Value::Null);
    }
    Ok(app.mockup.as_ref().map(MockupDialog::describe).unwrap_or(Value::Null))
}

// ---------------------------------------------------------------- the window

#[derive(Default)]
struct Actions {
    zones_from_document: bool,
    load_photo: bool,
    paste: bool,
    add_files: bool,
    add_document: bool,
    export: bool,
    cancel: bool,
    report_bug: bool,
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let _dtf = crate::dtf_i18n::scope();
    if app.mockup.is_none() {
        return;
    }
    let has_doc =
        (app.session.active().is_some(), app.services.clipboard_get_image.is_some(), app.session.active().is_some_and(|st| st.doc.selection.is_some()));
    let mut act = Actions::default();
    // Ctrl+V (⌘V): a screenshot as the photo.
    act.paste = !ctx.text_edit_focused()
        && ctx.input(|i| {
            i.events.iter().any(|e| match e {
                egui::Event::Paste(_) | egui::Event::PasteImage(_) => true,
                egui::Event::Key { key: egui::Key::V, pressed: true, modifiers, .. } => modifiers.command,
                _ => false,
            })
        });
    let prefs = app.session.prefs();
    let nav = [
        prefs.general.zoom_with_scroll_wheel,
        prefs.enhanced_controls.zoom_with_trackpad_pinch,
        prefs.general.animated_zoom,
        prefs.enhanced_controls.touch_gestures,
    ];
    if let Some(d) = app.mockup.as_mut() {
        d.nav = nav;
    }
    light_scope(ctx, || draw(app, ctx, has_doc, &mut act));
    if act.report_bug {
        crate::beta::report(app, Some(cmd::MOCKUP));
    }
    if act.load_photo {
        pick_photo(app);
    }
    if act.paste {
        paste_photo(app);
    }
    if act.zones_from_document {
        match zones_from_document(app) {
            Ok(n) => set_status(app, crate::i18n::fmt(tl!("{n} zones found"), &[("n", &n.to_string())])),
            Err(e) => set_status(app, e),
        }
    }
    if act.add_files {
        pick_visuals(app);
    }
    if act.add_document
        && let Err(e) = add_document(app)
    {
        set_status(app, e);
    }
    if act.export {
        export(app);
    }
    if act.cancel {
        app.mockup = None;
    }
}

struct Regions {
    header: ERect,
    left: ERect,
    canvas: ERect,
    right: ERect,
    status: ERect,
}

fn regions(full: ERect) -> Regions {
    let header = ERect::from_min_size(full.min, vec2(full.width(), HEADER_H));
    let status = ERect::from_min_max(pos2(full.left(), full.bottom() - STATUS_H), full.max);
    let left = ERect::from_min_max(pos2(full.left(), header.bottom()), pos2(full.left() + LEFT_W, status.top()));
    let right = ERect::from_min_max(pos2(full.right() - RIGHT_W, header.bottom()), pos2(full.right(), status.top()));
    let canvas = ERect::from_min_max(pos2(left.right(), header.bottom()), pos2(right.left(), status.top()));
    Regions { header, left, canvas, right, status }
}

fn draw(app: &mut PhotocraftApp, ctx: &egui::Context, has_doc: (bool, bool, bool), act: &mut Actions) {
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("dtf-mockup-dialog")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let Some(d) = app.mockup.as_mut() else { return };
        let t = Tokens::get(ctx);
        let (full, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(full, 0.0, t.canvas);
        let r = regions(full);
        header_bar(ui, d, r.header, &t, act);
        left_column(ui, d, r.left, &t, has_doc, act);
        canvas(ui, d, r.canvas, &t);
        right_column(ui, d, r.right, &t, act);
        status_bar(ui, d, r.status, &t);
        d.refresh(ctx);
        p.line_segment([r.header.left_bottom(), r.header.right_bottom()], Stroke::new(1.0, t.separator));
        p.line_segment([r.left.right_top(), r.left.right_bottom()], Stroke::new(1.0, t.separator));
        p.line_segment([r.right.left_top(), r.right.left_bottom()], Stroke::new(1.0, t.separator));
        p.line_segment([r.status.left_top(), r.status.right_top()], Stroke::new(1.0, t.separator));
    });
    if !ctx.text_edit_focused()
        && let Some(d) = app.mockup.as_mut()
    {
        keys(d, ctx);
    }
}

fn ready(d: &MockupDialog) -> bool {
    d.photo.is_some() && d.ppcm().is_some() && !d.items.is_empty()
}

/// The header: the logo, "DTF Mockup" (its last word in the logo's magenta) and the BETA badge,
/// then Report a Bug, Cancel and Export… on the right.
fn header_bar(ui: &mut egui::Ui, d: &mut MockupDialog, rect: ERect, t: &Tokens, act: &mut Actions) {
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, Color32::WHITE);
    let mark = ERect::from_center_size(pos2(rect.left() + 30.0, rect.center().y), vec2(28.0, 28.0));
    crate::brand::paint_mark(ui, mark);
    let title = tl!("DTF Mockup");
    let (first, last) = title.rsplit_once(' ').map_or((title, ""), |(a, b)| (a, b));
    let font = crate::theme::semibold(16.0);
    let ga = p.layout_no_wrap(format!("{first} "), font.clone(), t.text);
    let gb = p.layout_no_wrap(last.to_string(), font, MAGENTA);
    let y = rect.center().y - ga.size().y / 2.0;
    let x = mark.right() + 10.0;
    let (wa, wb) = (ga.size().x, gb.size().x);
    p.galley(pos2(x, y), ga, t.text);
    p.galley(pos2(x + wa, y), gb, MAGENTA);
    ui.interact(ERect::from_min_size(pos2(x, y), vec2(wa + wb, 20.0)), ui.scope_id().with("mockup-title"), Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, title));
    let mut bx = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(ERect::from_min_size(pos2(x + wa + wb + 8.0, rect.center().y - 10.0), vec2(60.0, 20.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    crate::beta::badge(&mut bx);
    let buttons = ERect::from_min_max(pos2(rect.right() - 470.0, rect.top()), rect.max).shrink2(vec2(16.0, 10.0));
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(buttons).layout(egui::Layout::right_to_left(egui::Align::Center)));
    let ok = ready(d);
    let r = b.add_enabled_ui(ok, |h| widgets::primary_button(h, tl!("Export…"), 120.0)).inner;
    if r.on_disabled_hover_text(tl!("A photo, its scale and at least one visual")).clicked() {
        act.export = true;
    }
    if widgets::secondary_button(&mut b, tl!("Cancel"), 90.0).clicked() {
        act.cancel = true;
    }
    b.add_space(8.0);
    if b.link(tl!("Report a Bug")).on_hover_text(tl!("A report to complete and copy into an e-mail")).clicked() {
        act.report_bug = true;
    }
}

/// A step's card title: its number in a magenta disc, then its name.
fn step_title(ui: &mut egui::Ui, n: u32, title: &str, t: &Tokens) {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let disc = pos2(r.left() + 9.0, r.center().y);
    ui.painter().circle_filled(disc, 9.0, MAGENTA);
    ui.painter().text(disc, Align2::CENTER_CENTER, n.to_string(), crate::theme::semibold(11.0), Color32::WHITE);
    ui.painter().text(pos2(r.left() + 26.0, r.center().y), Align2::LEFT_CENTER, title, crate::theme::semibold(13.0), t.text);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, title));
}

/// A wide button that stays pressed while `on` (magenta outline): Draw the Reference Line.
fn toggle_button(ui: &mut egui::Ui, label: &str, on: bool, width: f32) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    let enabled = ui.is_enabled();
    let (fill, stroke, text) = if on {
        (Color32::from_rgb(251, 234, 246), Stroke::new(1.5, MAGENTA), Color32::from_rgb(156, 31, 138))
    } else if resp.hovered() {
        (t.hover, Stroke::new(1.0, t.field_border), t.text)
    } else {
        (Color32::WHITE, Stroke::new(1.0, t.field_border), if enabled { t.text } else { t.text_faint })
    };
    ui.painter().rect(r, 15.0, fill, stroke, StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, crate::theme::medium(12.5), text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, label));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn num_text(v: f64, decimals: u32) -> String {
    widgets::fmt_num_prec(v, decimals)
}

/// The left column: the three steps (photo, scale, visuals).
fn left_column(ui: &mut egui::Ui, d: &mut MockupDialog, rect: ERect, t: &Tokens, (has_doc, can_paste, doc_sel): (bool, bool, bool), act: &mut Actions) {
    ui.painter().rect_filled(rect, 0.0, t.dock);
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 12.0))));
    panel.spacing_mut().item_spacing.y = 10.0;
    egui::ScrollArea::vertical().id_salt("mockup-left").auto_shrink([false, false]).show(&mut panel, |ui| {
        let w = ui.available_width();
        // 1. The photo.
        card_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            step_title(ui, 1, tl!("Product Photo"), t);
            if blue_button(ui, tl!("Load a Photo (PNG, JPG)"), ui.available_width(), false).clicked() {
                act.load_photo = true;
            }
            let pw = ui.available_width();
            let r = ui.add_enabled_ui(can_paste, |ui| widgets::secondary_button(ui, tl!("Paste a Screenshot"), pw)).inner;
            if r.on_hover_text(crate::dtf_keys::keys(tl!("The image on the clipboard (Ctrl+V)"))).clicked() {
                act.paste = true;
            }
            match &d.photo {
                Some(ph) => {
                    ui.label(egui::RichText::new(format!("{} · {} × {} px", ph.name, ph.width, ph.height)).size(11.5).color(t.text_faint));
                }
                None => {
                    ui.label(egui::RichText::new(tl!("The garment from the front, flat or worn, in good light.")).size(11.5).color(t.text_faint));
                }
            }
        });
        // 2. The scale.
        card_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            step_title(ui, 2, tl!("Photo Scale"), t);
            ui.add_enabled_ui(d.photo.is_some(), |ui| {
                let label = if d.line.is_some() && !d.drawing { tl!("Draw the Line Again") } else { tl!("Draw a Reference Line on the Photo") };
                if toggle_button(ui, label, d.drawing, ui.available_width())
                    .on_hover_text(tl!("Drag along something whose length you know: the chest width, say"))
                    .clicked()
                {
                    d.drawing = !d.drawing;
                }
                let before = d.length_cm;
                if number_row(ui, tl!("Real length:"), &mut d.length_cm, 0.1..=1000.0, "cm", 1) && (d.length_cm - before).abs() > f32::EPSILON {
                    d.dirty = true;
                }
            });
            match d.ppcm() {
                Some(k) => {
                    let text = crate::i18n::fmt(tl!("✓ 1 cm = {px} px on the photo · rulers in cm"), &[("px", &num_text(k, 1))]);
                    ui.label(egui::RichText::new(text).size(11.5).color(GREEN));
                }
                None => {
                    ui.label(
                        egui::RichText::new(tl!("Draw a line along a length you can measure on the garment, then type that length."))
                            .size(11.5)
                            .color(t.text_faint),
                    );
                }
            }
        });
        // 3. The product zones (optional).
        card_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            step_title(ui, 3, tl!("Product Zones (optional)"), t);
            zones_card(ui, d, t, doc_sel, act);
        });
        // 4. The visuals.
        card_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            step_title(ui, 4, tl!("Visuals"), t);
            let ppcm = d.ppcm();
            let scaled = ppcm.is_some();
            ui.add_enabled_ui(scaled, |ui| {
                if blue_button(ui, tl!("Import Images (PNG, SVG)"), ui.available_width(), false).clicked() {
                    act.add_files = true;
                }
                ui.add_enabled_ui(has_doc, |ui| {
                    if widgets::secondary_button(ui, tl!("Add Open Document"), ui.available_width()).clicked() {
                        act.add_document = true;
                    }
                });
            });
            if !scaled {
                ui.label(egui::RichText::new(tl!("Set the scale first")).size(11.5).color(t.warning));
            }
            let mut pick = None;
            let mut remove = None;
            for (i, it) in d.items.iter().enumerate() {
                let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                let on = d.selected == Some(i);
                let (fill, stroke) =
                    if on { (Color32::from_rgb(251, 234, 246), Stroke::new(1.5, MAGENTA)) } else { (Color32::WHITE, Stroke::new(1.0, t.card_border)) };
                ui.painter().rect(row, 8.0, fill, stroke, StrokeKind::Inside);
                let img = ERect::from_min_size(row.min + vec2(7.0, 7.0), vec2(32.0, 32.0));
                widgets::checker_rounded(ui.painter(), img, 5.0, 4);
                if let Some(tex) = &it.tex {
                    let s = tex.size_vec2();
                    let k = (img.width() / s.x.max(1.0)).min(img.height() / s.y.max(1.0));
                    ui.painter().image(
                        tex.id(),
                        ERect::from_center_size(img.center(), s * k),
                        ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                let text_w = row.right() - 30.0 - (img.right() + 8.0);
                let name = ui.painter().layout_no_wrap(it.visual.name.clone(), crate::theme::medium(12.5), t.text);
                let name = if name.size().x > text_w {
                    let short: String = it.visual.name.chars().take(((text_w / 7.0) as usize).max(4)).collect();
                    ui.painter().layout_no_wrap(format!("{short}…"), crate::theme::medium(12.5), t.text)
                } else {
                    name
                };
                ui.painter().galley(pos2(img.right() + 8.0, row.top() + 7.0), name, t.text);
                let dpi = crate::i18n::fmt(tl!("{dpi} ppi"), &[("dpi", &format!("{:.0}", it.visual.dpi))]);
                let (wp, hp) = it.size_px();
                let k = ppcm.unwrap_or(1.0);
                let sub = format!("{} × {} cm · {dpi}", num_text(wp / k, 1), num_text(hp / k, 1));
                ui.painter().text(pos2(img.right() + 8.0, row.top() + 27.0), Align2::LEFT_TOP, sub, FontId::proportional(11.0), t.text_faint);
                let x = ERect::from_center_size(pos2(row.right() - 16.0, row.center().y), vec2(18.0, 18.0));
                let xr = ui.interact(x, ui.scope_id().with(("mockup-x", i)), Sense::click());
                if xr.hovered() {
                    ui.painter().rect_filled(x, 4.0, t.danger);
                }
                crate::icons::image("x", 11.0, if xr.hovered() { Color32::WHITE } else { t.text_faint })
                    .paint_at(ui, ERect::from_center_size(x.center(), vec2(11.0, 11.0)));
                let remove_name = crate::i18n::fmt(tl!("Remove {name}"), &[("name", &it.visual.name)]);
                xr.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &remove_name));
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &it.visual.name));
                if xr.on_hover_text(tl!("Remove")).clicked() {
                    remove = Some(i);
                } else if resp.clicked() {
                    pick = Some(i);
                }
            }
            if !d.items.is_empty() {
                ui.label(egui::RichText::new(tl!("Each visual arrives at its print size: its pixels at its resolution.")).size(11.0).color(t.text_faint));
            }
            if let Some(i) = remove {
                d.remove(i);
            } else if let Some(i) = pick {
                d.selected = Some(i);
                d.closed[SECTION_VISUAL] = false;
            }
        });
        let _ = w;
    });
}

/// Step 3: Select the Product (one zone per object found), the polygonal lasso and its mode,
/// the open document's selection, the zones (renamed, removed), Invert and Clear.
fn zones_card(ui: &mut egui::Ui, d: &mut MockupDialog, t: &Tokens, doc_sel: bool, act: &mut Actions) {
    let has_photo = d.photo.is_some();
    ui.add_enabled_ui(has_photo, |ui| {
        let w = ui.available_width();
        if blue_button(ui, tl!("Select the Product (Auto)"), w, false).on_hover_text(tl!("One zone for each garment or view found on the photo")).clicked() {
            let n = d.auto_zones();
            d.status = crate::i18n::fmt(tl!("{n} zones found"), &[("n", &n.to_string())]);
        }
        let label = if d.lassoing { tl!("Polygonal Lasso: Click the Points") } else { tl!("Polygonal Lasso") };
        if toggle_button(ui, label, d.lassoing, w)
            .on_hover_text(tl!("A click for each point; a double click or the first point closes it; Esc cancels"))
            .clicked()
        {
            d.lassoing = !d.lassoing;
            d.lasso.clear();
            if d.lassoing {
                d.drawing = false;
                d.tool = None;
            }
        }
        ui.horizontal(|ui| {
            let bw = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0;
            for (mode, name) in [(ZoneMode::New, tl!("New")), (ZoneMode::Add, tl!("Add")), (ZoneMode::Subtract, tl!("Subtract"))] {
                let ok = mode == ZoneMode::New || d.zone_sel.is_some();
                if ui.add_enabled_ui(ok, |ui| toggle_button(ui, name, d.zone_mode == mode, bw)).inner.clicked() {
                    d.zone_mode = mode;
                }
            }
        });
        let r = ui.add_enabled_ui(doc_sel, |ui| widgets::secondary_button(ui, tl!("Use the Open Document's Selection"), w)).inner;
        if r.on_hover_text(tl!("A selection made in DTF Craft on the same photo")).clicked() {
            act.zones_from_document = true;
        }
    });
    let mut remove = None;
    let mut pick = None;
    let row_w = ui.available_width();
    for k in 0..d.zones.len() {
        let on = d.zone_sel == Some(k);
        ui.horizontal(|ui| {
            let (dot, resp) = ui.allocate_exact_size(vec2(18.0, 24.0), Sense::click());
            ui.painter().circle(dot.center(), if on { 7.0 } else { 5.5 }, zone_color(k), Stroke::new(if on { 2.0 } else { 0.0 }, Color32::WHITE));
            let pick_name = crate::i18n::fmt(tl!("Choose {name}"), &[("name", &d.zones[k].name)]);
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &pick_name));
            if resp.clicked() {
                pick = Some(k);
            }
            let name_w = (row_w - 18.0 - 24.0 - 3.0 * ui.spacing().item_spacing.x - 10.0).max(40.0);
            let field = egui::TextEdit::singleline(&mut d.zones[k].name).desired_width(name_w).font(egui::FontId::proportional(12.5));
            let r = ui.add(field).accessible_name(tl!("Zone name"));
            if r.gained_focus() {
                pick = Some(k);
            }
            let x = widgets::secondary_button(ui, "×", 22.0);
            let remove_name = crate::i18n::fmt(tl!("Remove {name}"), &[("name", &d.zones[k].name)]);
            if x.on_hover_text(&remove_name).clicked() {
                remove = Some(k);
            }
        });
    }
    if let Some(k) = pick {
        d.zone_sel = Some(k);
        d.overlay_dirty = true;
    }
    if let Some(k) = remove {
        d.remove_zone(k);
    }
    if !d.zones.is_empty() {
        ui.horizontal(|ui| {
            let bw = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            if ui.add_enabled_ui(d.zone_sel.is_some(), |ui| widgets::secondary_button(ui, tl!("Invert"), bw)).inner.clicked()
                && let Some(k) = d.zone_sel
            {
                d.invert_zone(k);
            }
            if widgets::secondary_button(ui, tl!("Clear"), bw).on_hover_text(tl!("Remove every zone")).clicked() {
                d.clear_zones();
            }
        });
        ui.label(egui::RichText::new(tl!("Each visual stops at the zone under its centre.")).size(11.0).color(GREEN));
    } else if has_photo {
        ui.label(egui::RichText::new(tl!("Optional: the visuals are cut at the garment's edge (sleeves, back, arms).")).size(11.0).color(t.text_faint));
    }
}

/// A section of the right column: a card whose title row opens or closes it.
fn section(ui: &mut egui::Ui, d: &mut MockupDialog, k: usize, title: &str, t: &Tokens, body: impl FnOnce(&mut egui::Ui, &mut MockupDialog)) {
    card_frame(t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let open = !d.closed[k];
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, title, crate::theme::semibold(13.0), t.text);
        let icon = if open { "chevron-down" } else { "chevron-right" };
        crate::icons::image(icon, 14.0, t.text_dim).paint_at(ui, ERect::from_center_size(r.right_center() - vec2(8.0, 0.0), vec2(14.0, 14.0)));
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::DisclosureTriangle, true, title));
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            d.closed[k] = open;
        }
        if open {
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            body(ui, d);
        }
    });
    ui.add_space(8.0);
}

fn right_column(ui: &mut egui::Ui, d: &mut MockupDialog, rect: ERect, t: &Tokens, act: &mut Actions) {
    ui.painter().rect_filled(rect, 0.0, t.dock);
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(12.0, 12.0))));
    egui::ScrollArea::vertical().id_salt("mockup-right").auto_shrink([false, false]).show(&mut panel, |ui| {
        section(ui, d, SECTION_VISUAL, tl!("Selected Visual"), t, |ui, d| visual_section(ui, d, t));
        section(ui, d, SECTION_TRANSFORM, tl!("Transform"), t, |ui, d| transform_section(ui, d, t));
        section(ui, d, SECTION_RENDER, tl!("Rendering"), t, |ui, d| render_section(ui, d, t));
        section(ui, d, SECTION_EXPORT, tl!("Export"), t, |ui, d| export_section(ui, d, t, act));
    });
}

fn visual_section(ui: &mut egui::Ui, d: &mut MockupDialog, t: &Tokens) {
    let Some(i) = d.selected.filter(|i| *i < d.items.len()) else {
        ui.label(egui::RichText::new(tl!("Click a visual on the photo or in the list.")).size(11.5).color(t.text_faint));
        return;
    };
    let k = d.ppcm().unwrap_or(1.0);
    let zone_names: Vec<String> = d.zones.iter().map(|z| z.name.clone()).collect();
    let auto_zone = {
        let saved = d.items.get(i).map(|it| it.zone);
        if let Some(it) = d.items.get_mut(i) {
            it.zone = ZoneChoice::Auto;
        }
        let z = d.zone_of(i);
        if let (Some(it), Some(s)) = (d.items.get_mut(i), saved) {
            it.zone = s;
        }
        z
    };
    let mut center = false;
    let mut delete = false;
    let mut changed = false;
    if let Some(it) = d.items.get_mut(i) {
        ui.label(egui::RichText::new(&it.visual.name).font(crate::theme::semibold(12.5)).color(t.text));
        let (wp, hp) = it.size_px();
        let (w0, h0) = ((wp / k) as f32, (hp / k) as f32);
        let (mut w, mut h) = (w0, h0);
        if number_row(ui, tl!("Width:"), &mut w, 0.1..=500.0, "cm", 2) && (w - w0).abs() > 1e-4 {
            let sx = f64::from(w / w0.max(1e-6));
            it.scale(sx, if it.lock { sx } else { 1.0 });
            changed = true;
        }
        if number_row(ui, tl!("Height:"), &mut h, 0.1..=500.0, "cm", 2) && (h - h0).abs() > 1e-4 {
            let sy = f64::from(h / h0.max(1e-6));
            it.scale(if it.lock { sy } else { 1.0 }, sy);
            changed = true;
        }
        switch_row(ui, tl!("Keep Proportions"), &mut it.lock);
        if !zone_names.is_empty() {
            let mut choice = it.zone;
            let auto = match auto_zone {
                Some(k) => crate::i18n::fmt(tl!("Auto ({zone})"), &[("zone", &zone_names[k])]),
                None => tl!("Auto (none)").to_string(),
            };
            let mut options: Vec<(ZoneChoice, &str)> = vec![(ZoneChoice::Auto, auto.as_str()), (ZoneChoice::Off, tl!("Not Masked"))];
            options.extend(zone_names.iter().enumerate().map(|(k, n)| (ZoneChoice::Zone(k), n.as_str())));
            ui.horizontal(|ui| {
                crate::gang_sheet_ui::fixed_label(ui, egui::RichText::new(tl!("Zone:")).color(t.text_dim), 110.0, 24.0);
                let w = ui.available_width();
                if widgets::dropdown_literal(ui, "mockup-visual-zone", tl!("Zone"), &mut choice, &options, w) {
                    it.zone = choice;
                    changed = true;
                }
            });
        }
        let r0 = it.rotation() as f32;
        let mut r = r0;
        if number_row(ui, tl!("Rotation:"), &mut r, -180.0..=180.0, "°", 0) && (r - r0).abs() > 1e-4 {
            it.rotate(f64::from(r - r0));
            changed = true;
        }
        let (nw, nh) = cmd::natural_size_cm(&it.visual);
        if (f64::from(w) - nw).abs() > 0.05 || (f64::from(h) - nh).abs() > 0.05 {
            let text =
                crate::i18n::fmt(tl!("Its file prints at {w} × {h} cm: this size is the mockup's only."), &[("w", &num_text(nw, 1)), ("h", &num_text(nh, 1))]);
            ui.label(egui::RichText::new(text).size(11.0).color(t.warning));
        }
        let bw = ui.available_width();
        if widgets::secondary_button(ui, tl!("Center Horizontally"), bw).on_hover_text(tl!("On the middle of the reference line")).clicked() {
            center = true;
        }
        if widgets::secondary_button(ui, tl!("Delete"), bw).clicked() {
            delete = true;
        }
    }
    if changed {
        d.dirty = true;
    }
    if center {
        d.center(i);
    }
    if delete {
        d.remove(i);
    }
}

/// The transform tool in use, Warp's style, bend and direction, and Reset.
fn transform_section(ui: &mut egui::Ui, d: &mut MockupDialog, t: &Tokens) {
    let Some(i) = d.selected.filter(|i| *i < d.items.len()) else {
        ui.label(egui::RichText::new(tl!("Click a visual on the photo or in the list.")).size(11.5).color(t.text_faint));
        return;
    };
    let mut tool = d.tool;
    let mut options: Vec<(Option<Tool>, &str)> = vec![(None, tl!("Move"))];
    options.extend(Tool::ALL.iter().map(|t| (Some(*t), t.label())));
    ui.horizontal(|ui| {
        crate::gang_sheet_ui::fixed_label(ui, egui::RichText::new(tl!("Tool:")).color(t.text_dim), 110.0, 24.0);
        let w = ui.available_width();
        if widgets::dropdown_literal(ui, "mockup-tool", tl!("Tool"), &mut tool, &options, w) {
            d.set_tool(tool);
        }
    });
    let mut reset = false;
    let mut changed = false;
    if let Some(it) = d.items.get_mut(i) {
        let b = &mut it.placement.bend;
        let mut style = b.style;
        let mut styles: Vec<(WarpStyle, &str)> = vec![(WarpStyle::None, style_label(WarpStyle::None))];
        styles.extend(cmd::STYLES.iter().map(|s| (*s, style_label(*s))));
        if style == WarpStyle::Custom {
            styles.push((WarpStyle::Custom, style_label(WarpStyle::Custom)));
        }
        ui.horizontal(|ui| {
            crate::gang_sheet_ui::fixed_label(ui, egui::RichText::new(tl!("Warp Style:")).color(t.text_dim), 110.0, 24.0);
            let w = ui.available_width();
            if widgets::dropdown_literal(ui, "mockup-warp-style", tl!("Warp Style"), &mut style, &styles, w) && style != b.style {
                if style.is_preset() && b.bend == 0.0 {
                    b.bend = DEFAULT_BEND;
                }
                b.style = style;
                b.points = None;
                changed = true;
            }
        });
        if b.style.is_preset() {
            let mut bend = b.bend as f32;
            if widgets::slider_row(ui, tl!("Bend"), &mut bend, -100.0..=100.0, "%", None).changed() {
                b.bend = f64::from(bend);
                changed = true;
            }
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 8.0) / 2.0;
                if toggle_button(ui, tl!("Horizontal"), !b.vertical, w).clicked() && b.vertical {
                    b.vertical = false;
                    changed = true;
                }
                if toggle_button(ui, tl!("Vertical"), b.vertical, w).clicked() && !b.vertical {
                    b.vertical = true;
                    changed = true;
                }
            });
        } else if b.style == WarpStyle::Custom {
            ui.label(egui::RichText::new(tl!("Drag the grid's points on the photo.")).size(11.0).color(t.text_faint));
        }
        let bw = ui.available_width();
        if widgets::secondary_button(ui, tl!("Reset the Transform"), bw).on_hover_text(tl!("Its own print size, upright, unwarped")).clicked() {
            reset = true;
        }
    }
    ui.label(egui::RichText::new(tl!("Enter or Esc: back to moving.")).size(11.0).color(t.text_faint));
    if changed {
        d.dirty = true;
    }
    if reset {
        d.reset(i);
    }
}

fn render_section(ui: &mut egui::Ui, d: &mut MockupDialog, t: &Tokens) {
    let mut changed = false;
    changed |= widgets::slider_row(ui, tl!("Fabric Shadows"), &mut d.shadows, 0.0..=100.0, "%", None).changed();
    changed |= widgets::slider_row(ui, tl!("Folds (Distortion)"), &mut d.folds, 0.0..=100.0, "%", None).changed();
    changed |= widgets::slider_row(ui, tl!("Ink Opacity"), &mut d.opacity, 0.0..=100.0, "%", None).changed();
    if changed {
        d.dirty = true;
    }
    ui.label(egui::RichText::new(tl!("Only a preview: what prints is your file.")).size(11.0).color(t.text_faint));
}

fn export_section(ui: &mut egui::Ui, d: &mut MockupDialog, t: &Tokens, act: &mut Actions) {
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 8.0) / 2.0;
        if toggle_button(ui, "PNG", !d.jpg, w).clicked() {
            d.jpg = false;
        }
        if toggle_button(ui, "JPG", d.jpg, w).clicked() {
            d.jpg = true;
        }
    });
    ui.label(egui::RichText::new(tl!("At the photo's resolution, then opened as a new document.")).size(11.0).color(t.text_faint));
    let ok = ready(d);
    let w = ui.available_width();
    if ui.add_enabled_ui(ok, |ui| widgets::primary_button(ui, tl!("Export…"), w)).inner.clicked() {
        act.export = true;
    }
}

fn status_bar(ui: &mut egui::Ui, d: &MockupDialog, rect: ERect, t: &Tokens) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Color32::WHITE);
    let text = match (d.ppcm(), &d.photo) {
        (Some(k), Some(ph)) => crate::i18n::fmt(
            tl!("Scale: 1 cm = {px} px · Photo {w} × {h} cm · {n} visuals"),
            &[
                ("px", &num_text(k, 1)),
                ("w", &num_text(f64::from(ph.width) / k, 1)),
                ("h", &num_text(f64::from(ph.height) / k, 1)),
                ("n", &d.items.len().to_string()),
            ],
        ),
        (None, Some(_)) => tl!("No scale yet").to_string(),
        _ => tl!("No photo").to_string(),
    };
    p.text(pos2(rect.left() + 14.0, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(12.0), t.text_dim);
    if !d.status.is_empty() {
        p.text(pos2(rect.right() - 14.0, rect.center().y), Align2::RIGHT_CENTER, &d.status, FontId::proportional(12.0), t.text_dim);
    }
}

/// The ruler along `edge` (top or left of the photo, `horizontal`), a tick each cm.
fn ruler(p: &egui::Painter, photo: ERect, inner: ERect, horizontal: bool, cm_px: f32, t: &Tokens) {
    // Along the visible part of the photo, at its edge or the canvas's when it is zoomed past it.
    let band = if horizontal {
        let top = photo.top().max(inner.top());
        ERect::from_min_max(pos2(photo.left().max(inner.left()), top - RULER), pos2(photo.right().min(inner.right()), top - 2.0))
    } else {
        let left = photo.left().max(inner.left());
        ERect::from_min_max(pos2(left - RULER, photo.top().max(inner.top())), pos2(left - 2.0, photo.bottom().min(inner.bottom())))
    };
    if band.width() <= 0.0 || band.height() <= 0.0 {
        return;
    }
    p.rect_filled(band, 3.0, Color32::from_rgba_unmultiplied(255, 255, 255, 200));
    // Thin ticks when a cm is too narrow: every 5.
    let step = if cm_px < 4.0 { 5 } else { 1 };
    let label_every = if cm_px * 10.0 < 34.0 {
        20
    } else if cm_px * 5.0 >= 34.0 {
        5
    } else {
        10
    };
    let (origin, from, to) = if horizontal { (photo.left(), band.left(), band.right()) } else { (photo.top(), band.top(), band.bottom()) };
    let first = (((from - origin) / cm_px).ceil().max(0.0) as u64).div_ceil(step as u64) as i64 * step;
    let mut k = first;
    while origin + k as f32 * cm_px <= to + 0.5 {
        let at = origin + k as f32 * cm_px;
        let tick = if k % 10 == 0 {
            9.0
        } else if k % 5 == 0 {
            6.0
        } else {
            3.5
        };
        let stroke = Stroke::new(1.0, t.text_faint);
        if horizontal {
            p.line_segment([pos2(at, band.bottom()), pos2(at, band.bottom() - tick)], stroke);
            if k % label_every == 0 && k > 0 && at + 20.0 < band.right() {
                p.text(pos2(at + 2.0, band.top() + 1.0), Align2::LEFT_TOP, k.to_string(), FontId::proportional(9.5), t.text_dim);
            }
        } else {
            p.line_segment([pos2(band.right(), at), pos2(band.right() - tick, at)], stroke);
            if k % label_every == 0 && k > 0 && at + 14.0 < band.bottom() {
                p.text(pos2(band.left() + 1.0, at + 2.0), Align2::LEFT_TOP, k.to_string(), FontId::proportional(9.5), t.text_dim);
            }
        }
        k += step;
    }
}

/// A pill with `text` centred at `at`.
fn pill(p: &egui::Painter, at: Pos2, text: &str, fill: Color32, color: Color32) {
    let g = p.layout_no_wrap(text.to_string(), FontId::proportional(11.0), color);
    let r = ERect::from_center_size(at, g.size() + vec2(14.0, 6.0));
    p.rect_filled(r, r.height() / 2.0, fill);
    p.galley(r.center() - g.size() / 2.0, g, color);
}

/// Where the photo lies in the canvas: its rect on screen and the screen points per photo pixel.
fn photo_rect(canvas: ERect, pw: f32, ph: f32) -> (ERect, f32) {
    view_rect(canvas, pw, ph, None)
}

/// Where the photo's frame is drawn in the canvas: right of the palette, inside the rulers.
fn inner_rect(canvas: ERect) -> ERect {
    ERect::from_min_max(canvas.min + vec2(PALETTE_W + 24.0 + RULER, RULER + 16.0), canvas.max - vec2(16.0, 16.0))
}

/// The photo fitted in the canvas's frame: screen points per photo pixel.
fn fit_zoom(canvas: ERect, pw: f32, ph: f32) -> f32 {
    let inner = inner_rect(canvas);
    (inner.width() / pw).min(inner.height() / ph).max(1e-4)
}

/// The photo's rect on screen and its zoom: fitted, or `view` (zoom, the photo point at the frame's
/// centre).
fn view_rect(canvas: ERect, pw: f32, ph: f32, view: Option<View>) -> (ERect, f32) {
    let inner = inner_rect(canvas);
    let fit = fit_zoom(canvas, pw, ph);
    match view {
        Some(v) if v.zoom > fit * 1.0001 => {
            let z = v.zoom.min(MAX_ZOOM);
            let min = inner.center() - vec2(v.center[0] as f32 * z, v.center[1] as f32 * z);
            (ERect::from_min_size(min, vec2(pw * z, ph * z)), z)
        }
        _ => (ERect::from_center_size(inner.center(), vec2(pw * fit, ph * fit)), fit),
    }
}

/// The zoomed view of the photo: screen points per photo pixel and the photo point at the frame's
/// centre.
#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    zoom: f32,
    center: [f64; 2],
}

impl MockupDialog {
    /// The photo's size and the canvas it was last drawn in.
    fn frame(&self) -> Option<(ERect, f32, f32)> {
        let ph = self.photo.as_ref()?;
        Some((self.canvas_rect?, ph.width as f32, ph.height as f32))
    }

    /// The zoom as it is (fitted or not).
    fn zoom_now(&self) -> Option<f32> {
        let (c, pw, ph) = self.frame()?;
        Some(view_rect(c, pw, ph, self.view).1)
    }

    /// Zoomed to `zoom` keeping the photo point under `at` (screen) where it is; at the fitted
    /// zoom or below, fitted.
    fn zoom_to(&mut self, zoom: f32, at: Pos2) {
        let Some((c, pw, ph)) = self.frame() else { return };
        let fit = fit_zoom(c, pw, ph);
        let (rect, z) = view_rect(c, pw, ph, self.view);
        let zoom = zoom.clamp(fit, MAX_ZOOM);
        if zoom <= fit * 1.0001 {
            self.view = None;
            return;
        }
        let q = [f64::from((at.x - rect.left()) / z), f64::from((at.y - rect.top()) / z)];
        let mid = inner_rect(c).center();
        let center = [q[0] - f64::from((at.x - mid.x) / zoom), q[1] - f64::from((at.y - mid.y) / zoom)];
        self.view = Some(View { zoom, center: [center[0].clamp(0.0, f64::from(pw)), center[1].clamp(0.0, f64::from(ph))] });
    }

    /// One rung of the zoom ladder in (1) or out (-1), about `at` (screen; the frame's centre).
    fn zoom_step(&mut self, dir: i32, at: Option<Pos2>) {
        let Some((c, pw, ph)) = self.frame() else { return };
        let z = self.zoom_now().unwrap_or(1.0);
        let next = crate::zoom_levels::step_in_range(z, dir, fit_zoom(c, pw, ph), MAX_ZOOM);
        self.zoom_to(next, at.unwrap_or(inner_rect(c).center()));
    }

    /// The zoomed photo moved by `delta` screen points (no move while fitted).
    fn pan(&mut self, delta: egui::Vec2) {
        let Some((_, pw, ph)) = self.frame() else { return };
        if let Some(v) = self.view.as_mut() {
            v.center =
                [(v.center[0] - f64::from(delta.x / v.zoom)).clamp(0.0, f64::from(pw)), (v.center[1] - f64::from(delta.y / v.zoom)).clamp(0.0, f64::from(ph))];
        }
    }
}

/// The Transform palette over the canvas's top left: Edit › Transform's six tools.
fn palette(ui: &mut egui::Ui, d: &mut MockupDialog, canvas: ERect, t: &Tokens) {
    let rect = ERect::from_min_size(canvas.min + vec2(12.0, 12.0), vec2(PALETTE_W, 22.0 + 30.0 * Tool::ALL.len() as f32 + 24.0));
    let p = ui.painter().clone();
    p.rect(rect, 10.0, Color32::WHITE, Stroke::new(1.0, t.card_border), StrokeKind::Inside);
    p.text(pos2(rect.center().x, rect.top() + 12.0), Align2::CENTER_CENTER, tl!("TRANSFORM"), crate::theme::semibold(9.5), MAGENTA);
    let enabled = !d.items.is_empty();
    for (k, tool) in Tool::ALL.iter().enumerate() {
        let r = ERect::from_min_size(pos2(rect.left() + 6.0, rect.top() + 22.0 + 30.0 * k as f32), vec2(rect.width() - 12.0, 28.0));
        let resp = ui.interact(r, ui.scope_id().with(("mockup-tool", k)), if enabled { Sense::click() } else { Sense::hover() });
        let on = d.tool == Some(*tool);
        if on {
            p.rect(r, 6.0, Color32::from_rgb(251, 234, 246), Stroke::new(1.5, MAGENTA), StrokeKind::Inside);
        } else if resp.hovered() && enabled {
            p.rect_filled(r, 6.0, t.hover);
        }
        let color = if !enabled {
            t.text_faint
        } else if on {
            Color32::from_rgb(156, 31, 138)
        } else {
            t.text
        };
        crate::icons::image(tool.icon(), 14.0, color).paint_at(ui, ERect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(14.0, 14.0)));
        p.text(
            pos2(r.left() + 28.0, r.center().y),
            Align2::LEFT_CENTER,
            tool.label(),
            if on { crate::theme::semibold(12.5) } else { crate::theme::medium(12.5) },
            color,
        );
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, tool.label()));
        if resp.clicked() {
            d.set_tool(if on { None } else { Some(*tool) });
        }
    }
    p.text(pos2(rect.center().x, rect.bottom() - 12.0), Align2::CENTER_CENTER, tl!("Enter or Esc: done"), FontId::proportional(10.0), t.text_faint);
}

/// The photo, the visuals' outlines, the reference line and the rulers; the line drawn, its ends,
/// the visuals moved and transformed with the mouse.
fn canvas(ui: &mut egui::Ui, d: &mut MockupDialog, rect: ERect, t: &Tokens) {
    let bg = ui.painter_at(rect);
    bg.rect_filled(rect, 0.0, t.canvas);
    d.canvas_rect = Some(rect);
    let Some((pw, ph)) = d.photo.as_ref().map(|ph| (ph.width as f32, ph.height as f32)) else {
        bg.text(rect.center(), Align2::CENTER_CENTER, tl!("Start with the product's photo (step 1)."), FontId::proportional(14.0), t.text_faint);
        return;
    };
    // The mouse wheel and the trackpad as on DTF Craft's canvas: scroll, or zoom about the pointer.
    let inner = inner_rect(rect);
    let hovered = ui.rect_contains_pointer(inner);
    let [zoom_with_wheel, pinch, animated, touch] = d.nav;
    let wheel = crate::wheel_nav::read_with(ui.ctx(), zoom_with_wheel, pinch, animated, ui.scope_id().with("mockup-wheel"), hovered, false, touch);
    if hovered {
        ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
        let at = ui.input(|i| i.pointer.hover_pos()).unwrap_or(inner.center());
        match wheel {
            Some(crate::wheel_nav::Wheel::Zoom(f)) => {
                let z = d.zoom_now().unwrap_or(1.0);
                d.zoom_to(z * f, at);
            }
            Some(crate::wheel_nav::Wheel::SnapZoom(n)) => {
                for _ in 0..n.unsigned_abs() {
                    d.zoom_step(n.signum(), Some(at));
                }
            }
            Some(crate::wheel_nav::Wheel::Pan(delta)) => d.pan(delta),
            None => {}
        }
    }
    let (photo, z) = view_rect(rect, pw, ph, d.view);
    // The photo and all on it stay inside the frame; the rulers go round it.
    let p = ui.painter_at(inner);
    let to_screen = |q: [f64; 2]| pos2(photo.left() + q[0] as f32 * z, photo.top() + q[1] as f32 * z);
    let to_photo = |s: Pos2| [f64::from((s.x - photo.left()) / z), f64::from((s.y - photo.top()) / z)];
    if let Some(tex) = &d.shown {
        p.image(tex.id(), photo, ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    if let Some(tex) = &d.overlay {
        p.image(tex.id(), photo, ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    p.rect_stroke(photo, 0.0, Stroke::new(1.0, t.card_border), StrokeKind::Outside);
    let ppcm = d.ppcm();
    if let Some(k) = ppcm {
        let cm_px = k as f32 * z;
        ruler(&bg, photo, inner, true, cm_px, t);
        ruler(&bg, photo, inner, false, cm_px, t);
    }
    // The visuals' outlines, the selected one in magenta with its size and its tool's handles.
    let k = ppcm.unwrap_or(1.0);
    for (i, it) in d.items.iter().enumerate() {
        let outline: Vec<Pos2> = it.outline().into_iter().map(to_screen).collect();
        if outline.len() < 2 {
            continue;
        }
        let on = d.selected == Some(i);
        if !on {
            p.extend(Shape::dashed_line(&outline, Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 110, 140, 120)), 4.0, 3.0));
            continue;
        }
        p.add(Shape::line(outline.clone(), Stroke::new(1.5, MAGENTA)));
        let tool = d.tool;
        if let Some(tool) = tool {
            let hs: Vec<Pos2> = it.handles(tool).into_iter().map(to_screen).collect();
            if tool == Tool::Warp {
                // The warped thirds, as Edit › Transform › Warp shows them, then the control points.
                let grid = it.grid();
                let stroke = Stroke::new(1.0, MAGENTA.gamma_multiply(0.55));
                for third in [1.0 / 3.0, 2.0 / 3.0] {
                    let across: Vec<Pos2> = (0..=16).filter_map(|n| it.placement.map(grid.as_ref(), n as f64 / 16.0, third)).map(to_screen).collect();
                    let down: Vec<Pos2> = (0..=16).filter_map(|n| it.placement.map(grid.as_ref(), third, n as f64 / 16.0)).map(to_screen).collect();
                    p.add(Shape::line(across, stroke));
                    p.add(Shape::line(down, stroke));
                }
                for q in &hs {
                    p.circle(*q, 4.5, Color32::WHITE, Stroke::new(1.5, MAGENTA));
                }
            } else {
                let shown: &[usize] = match tool {
                    Tool::Perspective => &[0, 1, 2, 3],
                    Tool::Skew => &[0, 1, 2, 3, 4, 5, 6, 7],
                    _ => &[0, 1, 2, 3, 4, 5, 6, 7],
                };
                for &h in shown {
                    p.rect(ERect::from_center_size(hs[h], vec2(8.0, 8.0)), 1.0, Color32::WHITE, Stroke::new(1.5, MAGENTA), StrokeKind::Inside);
                }
                if tool == Tool::Rotate {
                    p.circle_stroke(to_screen(it.center()), 5.0, Stroke::new(1.5, MAGENTA));
                }
            }
        }
        let bottom = outline.iter().map(|q| q.y).fold(f32::MIN, f32::max);
        let midx = outline.iter().map(|q| q.x).sum::<f32>() / outline.len() as f32;
        let (w, h) = it.size_px();
        let mut size = format!("{} × {} cm", num_text(w / k, 1), num_text(h / k, 1));
        if let Some(tool) = tool {
            size = format!("{size} · {}", tool.label());
        }
        pill(&p, pos2(midx, bottom + 14.0), &size, Color32::WHITE, t.text_dim);
    }
    // The reference line.
    if let Some([a, b]) = d.line {
        let (sa, sb) = (to_screen(a), to_screen(b));
        p.extend(Shape::dashed_line(&[sa, sb], Stroke::new(2.0, BLUE), 7.0, 4.0));
        for q in [sa, sb] {
            p.circle(q, 6.0, BLUE, Stroke::new(2.0, Color32::WHITE));
        }
        let label = crate::i18n::fmt(tl!("Reference: {cm} cm"), &[("cm", &num_text(f64::from(d.length_cm), 1))]);
        // Over its left end, clear of the visuals in the middle.
        let left = if sa.x <= sb.x { sa } else { sb };
        pill(&p, left + vec2(0.0, -18.0), &label, BLUE, Color32::WHITE);
    }
    if d.drawing {
        pill(
            &p,
            pos2(photo.center().x.clamp(inner.left() + 160.0, inner.right() - 160.0), photo.top().max(inner.top()) + 18.0),
            tl!("Drag along a length you know (the chest width, say)"),
            Color32::from_rgb(255, 248, 214),
            Color32::from_rgb(107, 86, 0),
        );
    }
    // The mouse.
    let resp = ui.interact(rect, ui.scope_id().with("mockup-photo"), Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, tl!("Product Photo")));
    // The hand: Space held, or the middle button, moves the zoomed photo; whatever tool is on
    // (the lasso keeps its points) waits.
    let space = !ui.ctx().text_edit_focused() && ui.input(|i| i.key_down(egui::Key::Space));
    let middle = resp.dragged_by(egui::PointerButton::Middle);
    if space || middle {
        if resp.dragged() {
            d.pan(resp.drag_delta());
        }
        let _ = resp.clone().on_hover_cursor(crate::tool_cursor::hand(ui.ctx(), resp.dragged()));
        if d.lassoing {
            lasso_outline(d, &p, None, photo, z);
        }
        d.drag = None;
        zoom_bar(ui, d, inner, t);
        palette(ui, d, rect, t);
        return;
    }
    if d.lassoing {
        lasso(d, &p, &resp, photo, z);
        zoom_bar(ui, d, inner, t);
        palette(ui, d, rect, t);
        return;
    }
    let resp = if d.drawing { resp.on_hover_cursor(egui::CursorIcon::Crosshair) } else { resp };
    let ends = d.line.map(|[a, b]| [to_screen(a), to_screen(b)]);
    let handle_at = |d: &MockupDialog, o: Pos2| -> Option<(usize, Tool, usize)> {
        let tool = d.tool?;
        let i = d.selected?;
        let it = d.items.get(i)?;
        let hs = it.handles(tool);
        let usable = |h: usize| tool == Tool::Warp || !(tool == Tool::Perspective && h >= 4);
        hs.iter()
            .enumerate()
            .filter(|(h, _)| usable(*h))
            .map(|(h, q)| (h, to_screen(*q).distance(o)))
            .filter(|(_, dd)| *dd <= GRAB)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| (i, tool, h))
    };
    let press = ui.input(|i| i.pointer.press_origin());
    if resp.drag_started()
        && let Some(o) = press
    {
        let q = to_photo(o);
        d.drag = if d.drawing {
            d.line = Some([q, q]);
            Some(Drag::Line)
        } else if let Some((index, tool, handle)) = handle_at(d, o) {
            Some(Drag::Handle(HandleDrag { index, tool, handle, from: q, start: d.items[index].placement.clone() }))
        } else if let Some(k) = ends.and_then(|e| e.iter().position(|s| s.distance(o) <= GRAB)) {
            Some(Drag::End(k))
        } else if let Some(i) = d.hit(q) {
            if d.selected != Some(i) {
                d.tool = None;
            }
            d.selected = Some(i);
            d.closed[SECTION_VISUAL] = false;
            d.items.get(i).map(|it| {
                let c = it.center();
                Drag::Move { index: i, grab: [q[0] - c[0], q[1] - c[1]] }
            })
        } else if d.tool == Some(Tool::Rotate)
            && let Some(i) = d.selected
        {
            // Rotate: a drag anywhere turns the selected visual.
            Some(Drag::Handle(HandleDrag { index: i, tool: Tool::Rotate, handle: 0, from: q, start: d.items[i].placement.clone() }))
        } else {
            None
        };
    }
    if resp.dragged()
        && let (Some(drag), Some(pos)) = (d.drag.clone(), resp.interact_pointer_pos())
    {
        let q = to_photo(pos);
        let mods = ui.input(|i| i.modifiers);
        match drag {
            Drag::Line => {
                let q = [q[0].clamp(0.0, f64::from(pw)), q[1].clamp(0.0, f64::from(ph))];
                if let Some(l) = d.line.as_mut() {
                    l[1] = q;
                }
            }
            Drag::End(k) => {
                let q = [q[0].clamp(0.0, f64::from(pw)), q[1].clamp(0.0, f64::from(ph))];
                if let Some(l) = d.line.as_mut() {
                    l[k] = q;
                }
            }
            Drag::Move { index, grab } => {
                if let Some(it) = d.items.get_mut(index) {
                    let c = it.center();
                    it.translate([q[0] - grab[0] - c[0], q[1] - grab[1] - c[1]]);
                }
            }
            Drag::Handle(h) => {
                if let Some(it) = d.items.get_mut(h.index) {
                    it.placement = drag_handle(&h, q, it.lock != mods.shift, mods.alt, mods.shift);
                }
            }
        }
        d.dirty = true;
    }
    if resp.drag_stopped() {
        if d.drag == Some(Drag::Line) {
            if d.ppcm().is_some() {
                d.drawing = false;
                d.status.clear();
            } else if d.line.is_some_and(|[a, b]| (a[0] - b[0]).hypot(a[1] - b[1]) < 2.0) {
                d.line = None;
            }
        }
        d.drag = None;
    }
    if (resp.clicked() || resp.secondary_clicked())
        && !d.drawing
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let hit = d.hit(to_photo(pos));
        if hit != d.selected && handle_at(d, pos).is_none() {
            d.tool = None;
            d.selected = hit;
        }
        if d.selected.is_some() {
            d.closed[SECTION_VISUAL] = false;
        }
    }
    // Right click: the transform tools.
    if d.selected.is_some() {
        resp.context_menu(|ui| {
            for tool in Tool::ALL {
                if ui.selectable_label(d.tool == Some(tool), tool.label()).clicked() {
                    d.set_tool(Some(tool));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(tl!("Reset the Transform")).clicked() {
                if let Some(i) = d.selected {
                    d.reset(i);
                }
                ui.close();
            }
        });
    }
    zoom_bar(ui, d, inner, t);
    palette(ui, d, rect, t);
}

/// The zoom at the frame's bottom right: −, the zoom in %, + and Fit.
fn zoom_bar(ui: &mut egui::Ui, d: &mut MockupDialog, inner: ERect, t: &Tokens) {
    let size = vec2(196.0, 30.0);
    let r = ERect::from_min_size(inner.max - size - vec2(6.0, 6.0), size);
    ui.painter().rect(r, 8.0, Color32::WHITE, Stroke::new(1.0, t.card_border), StrokeKind::Inside);
    let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(4.0, 3.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    bar.spacing_mut().item_spacing.x = 4.0;
    let z = d.zoom_now().unwrap_or(1.0);
    if widgets::secondary_button(&mut bar, "−", 26.0).on_hover_text(crate::dtf_keys::keys(tl!("Zoom Out (Ctrl −)"))).clicked() {
        d.zoom_step(-1, None);
    }
    let pct = format!("{} %", (z * 100.0).round());
    let (lr, _) = bar.allocate_exact_size(vec2(52.0, 22.0), Sense::hover());
    bar.painter().text(lr.center(), Align2::CENTER_CENTER, pct, FontId::proportional(12.0), t.text);
    if widgets::secondary_button(&mut bar, "+", 26.0).on_hover_text(crate::dtf_keys::keys(tl!("Zoom In (Ctrl +)"))).clicked() {
        d.zoom_step(1, None);
    }
    let fit = bar.add_enabled_ui(d.view.is_some(), |ui| widgets::secondary_button(ui, tl!("Fit"), 66.0)).inner;
    if fit.on_hover_text(crate::dtf_keys::keys(tl!("The whole photo (Ctrl 0)"))).clicked() {
        d.view = None;
    }
}

/// The polygonal lasso on the photo: a click adds a point, a click on the first point or a double
/// click closes it (into a zone, as the mode says); the outline so far follows the pointer.
fn lasso(d: &mut MockupDialog, p: &egui::Painter, resp: &egui::Response, photo: ERect, z: f32) {
    let to_screen = |q: [f64; 2]| pos2(photo.left() + q[0] as f32 * z, photo.top() + q[1] as f32 * z);
    let to_photo = |s: Pos2| [f64::from((s.x - photo.left()) / z), f64::from((s.y - photo.top()) / z)];
    let resp = resp.clone().on_hover_cursor(egui::CursorIcon::Crosshair);
    if resp.double_clicked() {
        // The second click of the double click added a point: it closes instead.
        d.lasso.pop();
        d.close_lasso();
    } else if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let first = d.lasso.first().map(|q| to_screen(*q));
        if d.lasso.len() >= 3 && first.is_some_and(|f| f.distance(pos) <= GRAB) {
            d.close_lasso();
        } else {
            let q = to_photo(pos);
            let (w, h) = d.photo.as_ref().map_or((0.0, 0.0), |ph| (f64::from(ph.width), f64::from(ph.height)));
            d.lasso.push([q[0].clamp(0.0, w), q[1].clamp(0.0, h)]);
        }
    }
    lasso_outline(d, p, resp.hover_pos(), photo, z);
}

/// The lasso's points so far, the outline to the pointer, the first point and the mode.
fn lasso_outline(d: &MockupDialog, p: &egui::Painter, hover: Option<Pos2>, photo: ERect, z: f32) {
    let to_screen = |q: [f64; 2]| pos2(photo.left() + q[0] as f32 * z, photo.top() + q[1] as f32 * z);
    let mut pts: Vec<Pos2> = d.lasso.iter().map(|q| to_screen(*q)).collect();
    if let Some(hover) = hover
        && !pts.is_empty()
    {
        pts.push(hover);
    }
    if pts.len() >= 2 {
        p.extend(Shape::dashed_line(&pts, Stroke::new(1.5, Color32::BLACK), 5.0, 4.0));
        p.extend(Shape::dashed_line_with_offset(&pts, Stroke::new(1.5, Color32::WHITE), &[5.0], &[4.0], 4.5));
    }
    if let Some(f) = d.lasso.first() {
        p.circle(to_screen(*f), 5.0, Color32::WHITE, Stroke::new(1.5, MAGENTA));
    }
    let hint = match d.zone_mode {
        ZoneMode::New => tl!("Lasso: a new zone"),
        ZoneMode::Add => tl!("Lasso: added to the chosen zone"),
        ZoneMode::Subtract => tl!("Lasso: taken from the chosen zone"),
    };
    let frame = p.clip_rect();
    pill(
        p,
        pos2(photo.center().x.clamp(frame.left() + 120.0, frame.right() - 120.0), photo.top().max(frame.top()) + 18.0),
        hint,
        Color32::from_rgb(255, 248, 214),
        Color32::from_rgb(107, 86, 0),
    );
}

/// Delete removes the selected visual, the arrows move it (a mm, a cm with Shift), Enter or Esc
/// leaves the transform tool (Esc also stops drawing the line).
fn keys(d: &mut MockupDialog, ctx: &egui::Context) {
    // Ctrl + / Ctrl − / Ctrl 0: zoom in, out, the whole photo.
    let (zin, zout, fit) = ctx.input(|i| {
        let c = i.modifiers.command;
        (c && (i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals)), c && i.key_pressed(egui::Key::Minus), c && i.key_pressed(egui::Key::Num0))
    });
    if zin {
        d.zoom_step(1, None);
    }
    if zout {
        d.zoom_step(-1, None);
    }
    if fit {
        d.view = None;
    }
    let (del, esc, enter, shift, arrows) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::Enter),
            i.modifiers.shift,
            [egui::Key::ArrowLeft, egui::Key::ArrowRight, egui::Key::ArrowUp, egui::Key::ArrowDown].map(|k| i.key_pressed(k)),
        )
    });
    if d.lassoing {
        // Esc drops the points (then the lasso), Backspace the last point, Enter closes.
        if esc {
            if d.lasso.is_empty() {
                d.lassoing = false;
            }
            d.lasso.clear();
        } else if del {
            d.lasso.pop();
        } else if enter {
            d.close_lasso();
        }
        return;
    }
    if esc && d.drawing {
        d.drawing = false;
    }
    if esc || enter {
        d.tool = None;
    }
    let Some(i) = d.selected else { return };
    if del {
        d.remove(i);
        return;
    }
    let step = d.ppcm().unwrap_or(0.0) * if shift { 1.0 } else { 0.1 };
    let (dx, dy) = (f64::from(u8::from(arrows[1])) - f64::from(u8::from(arrows[0])), f64::from(u8::from(arrows[3])) - f64::from(u8::from(arrows[2])));
    if (dx != 0.0 || dy != 0.0)
        && let Some(it) = d.items.get_mut(i)
    {
        it.translate([dx * step, dy * step]);
        d.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, ui: Value) -> Value {
        menu(app, ctx, cmd::MOCKUP, &json!({"ui": ui})).unwrap().unwrap()
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dtf-mockup-ui-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A flat photo (`w × h`, `color`) and a 2.54 cm square visual (300 px at 300 ppi).
    fn files(dir: &std::path::Path, w: u32, h: u32, color: &str) -> (String, String) {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": w, "height": h, "resolution": 72, "background": "white"})).unwrap();
        s.execute("edit.fill", json!({"color": color})).unwrap();
        let photo = dir.join("tee.jpg").to_string_lossy().into_owned();
        s.execute("file.saveACopy", json!({"path": photo})).unwrap();
        s.execute("file.new", json!({"width": 300, "height": 300, "resolution": 300, "background": "transparent"})).unwrap();
        s.execute("select.rect", json!({"x": 0, "y": 50, "width": 300, "height": 200})).unwrap();
        s.execute("edit.fill", json!({"color": "#c8102e"})).unwrap();
        let logo = dir.join("logo.png").to_string_lossy().into_owned();
        s.execute("file.saveACopy", json!({"path": logo})).unwrap();
        (photo, logo)
    }

    /// DTF › Mockup (beta)… opens the window with its beta notice; the visuals wait for the scale;
    /// a visual arrives at its print size; the export renders the photo with it and closes.
    #[test]
    fn the_mockup_waits_for_the_scale_then_exports() {
        let dir = temp_dir("flow");
        let (photo, logo) = files(&dir, 600, 400, "#d8d8d8");
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        let ids: Vec<&str> = crate::menu_catalog::CATALOG.iter().filter(|(path, _, _, _)| path == &["DTF"]).map(|(_, _, _, id)| *id).collect();
        assert!(ids.contains(&cmd::MOCKUP), "in the DTF menu");
        assert!(crate::beta::is_beta(cmd::MOCKUP));
        crate::menus::invoke(&mut app, &ctx, cmd::MOCKUP, json!({})).unwrap();
        assert!(app.mockup.is_some());
        let v = invoke(&mut app, &ctx, json!({"photo": photo}));
        assert_eq!(v["photo"]["pixels"], json!([600, 400]));
        assert_eq!(v["drawing"], true, "the line is asked for at once");
        // No scale: the visual waits.
        let v = invoke(&mut app, &ctx, json!({"addFiles": [logo]}));
        assert_eq!(v["visuals"].as_array().unwrap().len(), 0);
        assert_eq!(v["status"], tl!("Set the scale first"));
        // 200 px = 20 cm: 10 px per cm; the 2.54 × 1.69 cm visual on the line's middle.
        let v = invoke(&mut app, &ctx, json!({"scale": {"from": [200, 150], "to": [400, 150], "cm": 20}, "addFiles": [logo]}));
        assert!((v["pixelsPerCm"].as_f64().unwrap() - 10.0).abs() < 1e-9);
        let it = &v["visuals"][0];
        assert!((it["widthCm"].as_f64().unwrap() - 2.54).abs() < 0.01, "{it}");
        assert!((it["heightCm"].as_f64().unwrap() - 2.54 * 200.0 / 300.0).abs() < 0.02, "cropped to its visible pixels: {it}");
        assert_eq!((it["x"].as_f64().unwrap(), it["y"].as_f64().unwrap()), (300.0, 150.0));
        // Proportions kept: twice as wide, twice as tall.
        let v = invoke(&mut app, &ctx, json!({"visual": {"index": 0, "widthCm": 5.08}}));
        assert!((v["visuals"][0]["heightCm"].as_f64().unwrap() - 3.387).abs() < 0.01);
        // The preview renders.
        app.mockup.as_mut().unwrap().refresh(&ctx);
        assert!(app.mockup.as_ref().unwrap().shown.is_some());
        // Hit test on the visual, not beside it.
        let d = app.mockup.as_ref().unwrap();
        assert_eq!(d.hit([300.0, 150.0]), Some(0));
        assert_eq!(d.hit([340.0, 150.0]), None);
        let out = dir.join("mockup.png").to_string_lossy().into_owned();
        invoke(&mut app, &ctx, json!({"render": {"shadows": 0, "folds": 0, "opacity": 100}, "export": out}));
        assert!(app.mockup.is_none(), "closed once exported");
        assert!(std::path::Path::new(&out).exists());
        let st = app.session.active().unwrap();
        assert_eq!((st.doc.size.width, st.doc.size.height), (600, 400));
        let px = st.doc.layers[0].surface().unwrap().read_region(photocraft_geom::Rect::new(300, 150, 301, 151));
        assert!(px[0] > 0.7 && px[1] < 0.15, "the visual's red: {px:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The reference line drawn with the mouse sets the scale; a drag moves a visual.
    #[test]
    fn the_line_is_drawn_and_the_visual_dragged() {
        let dir = temp_dir("mouse");
        let (photo, logo) = files(&dir, 600, 400, "#d8d8d8");
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        open(&mut app);
        app.beta = Default::default();
        invoke(&mut app, &ctx, json!({"photo": photo}));
        // Drawn once the app's fonts are in.
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1366.0, 900.0)).build_ui_state(
            |ui, s: &mut (PhotocraftApp, bool)| {
                if s.1 {
                    let ctx = ui.ctx().clone();
                    show(&mut s.0, &ctx);
                }
            },
            (app, false),
        );
        PhotocraftApp::setup_context(&h.ctx, Default::default());
        h.state_mut().1 = true;
        h.run_steps(2);
        // The photo's place on screen: fitted in the canvas.
        let canvas = regions(ERect::from_min_size(Pos2::ZERO, vec2(1366.0, 900.0))).canvas;
        let (photo_rect, z) = photo_rect(canvas, 600.0, 400.0);
        let at = |x: f32, y: f32| photo_rect.min + vec2(x * z, y * z);
        let drag = |h: &mut egui_kittest::Harness<'static, (PhotocraftApp, bool)>, a: Pos2, b: Pos2| {
            let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            h.hover_at(a);
            h.run_steps(1);
            h.event(button(a, true));
            h.run_steps(1);
            for k in 1..=6 {
                h.hover_at(a + (b - a) * (k as f32 / 6.0));
                h.run_steps(1);
            }
            h.event(button(b, false));
            h.run_steps(2);
        };
        drag(&mut h, at(100.0, 200.0), at(500.0, 200.0));
        let d = h.state().0.mockup.as_ref().unwrap();
        assert!(!d.drawing);
        let [a, b] = d.line.unwrap();
        assert!((b[0] - a[0] - 400.0).abs() < 2.0, "{a:?} {b:?}");
        // 400 px = 40 cm.
        if let Some(d) = h.state_mut().0.mockup.as_mut() {
            d.length_cm = 40.0;
        }
        add_files(&mut h.state_mut().0, &[logo]);
        h.run_steps(2);
        let c0 = h.state().0.mockup.as_ref().unwrap().items[0].center();
        drag(&mut h, at(c0[0] as f32, c0[1] as f32), at(c0[0] as f32 + 50.0, c0[1] as f32 + 30.0));
        let c1 = h.state().0.mockup.as_ref().unwrap().items[0].center();
        assert!((c1[0] - c0[0] - 50.0).abs() < 2.0 && (c1[1] - c0[1] - 30.0).abs() < 2.0, "{c0:?} → {c1:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Edit › Transform's tools on a 100 × 50 box: Scale from a corner (proportions kept or
    /// not, from the centre with Alt), Rotate, Skew, Distort, Perspective and Warp.
    #[test]
    fn the_transform_tools_move_the_corners() {
        let start = Placement { corners: cmd::rect_corners([150.0, 100.0], 100.0, 50.0, 0.0), bend: Bend::default() };
        let drag = |tool, handle, from: [f64; 2], to: [f64; 2], prop, centered, snap| {
            drag_handle(&HandleDrag { index: 0, tool, handle, from, start: start.clone() }, to, prop, centered, snap).corners
        };
        let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6;
        // Bottom right dragged 50 right: half as wide again; proportions kept: half as tall again too.
        let c = drag(Tool::Scale, 2, [200.0, 125.0], [250.0, 125.0], false, false, false);
        assert!(near(c[0], [100.0, 75.0]) && near(c[2], [250.0, 125.0]), "{c:?}");
        let c = drag(Tool::Scale, 2, [200.0, 125.0], [250.0, 125.0], true, false, false);
        assert!(near(c[2], [250.0, 150.0]), "{c:?}");
        // From the centre (Alt): both sides move.
        let c = drag(Tool::Scale, 5, [200.0, 100.0], [250.0, 100.0], false, true, false);
        assert!(near(c[0], [50.0, 75.0]) && near(c[1], [250.0, 75.0]), "{c:?}");
        // Rotate a quarter turn (snapped from 88°).
        let a = 88f64.to_radians();
        let c = drag(Tool::Rotate, 0, [200.0, 100.0], [150.0 + 50.0 * a.cos(), 100.0 + 50.0 * a.sin()], false, false, true);
        assert!(near(c[0], [175.0, 50.0]), "{c:?}");
        // Skew the top side 20 right: the bottom stays.
        let c = drag(Tool::Skew, 4, [150.0, 75.0], [170.0, 90.0], false, false, false);
        assert!(near(c[0], [120.0, 75.0]) && near(c[1], [220.0, 75.0]) && near(c[3], [100.0, 125.0]), "{c:?}");
        // Distort one corner freely.
        let c = drag(Tool::Distort, 1, [200.0, 75.0], [210.0, 60.0], false, false, false);
        assert!(near(c[1], [210.0, 60.0]) && near(c[0], [100.0, 75.0]), "{c:?}");
        // Perspective: the top left in, the top right in as much.
        let c = drag(Tool::Perspective, 0, [100.0, 75.0], [120.0, 77.0], false, false, false);
        assert!(near(c[0], [120.0, 75.0]) && near(c[1], [180.0, 75.0]), "{c:?}");
        // Warp: the grid's top middle point lifted: a custom warp.
        let w = drag_handle(
            &HandleDrag { index: 0, tool: Tool::Warp, handle: 1, from: [133.333, 75.0], start: start.clone() },
            [133.333, 60.0],
            false,
            false,
            false,
        );
        assert_eq!(w.bend.style, WarpStyle::Custom);
        assert!(w.bend.points.unwrap()[1][1] < -0.2, "{:?}", w.bend.points);
    }

    /// A screenshot pasted from the clipboard is the photo; the tools, a warp and the corners
    /// reach the export.
    #[test]
    fn a_pasted_screenshot_and_the_transforms_reach_the_export() {
        let dir = temp_dir("paste");
        let (_, logo) = files(&dir, 10, 10, "#ffffff");
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        app.services.clipboard_get_image = Some(Box::new(|| Ok(Some((400, 300, [200u8, 200, 200, 255].repeat(400 * 300))))));
        open(&mut app);
        let v = invoke(&mut app, &ctx, json!({"paste": true}));
        assert_eq!(v["photo"]["pixels"], json!([400, 300]));
        assert_eq!(v["photo"]["name"], tl!("Screenshot"));
        let v = invoke(&mut app, &ctx, json!({"scale": {"from": [100, 150], "to": [300, 150], "cm": 20}, "addFiles": [logo], "tool": "warp"}));
        assert_eq!(v["tool"], "warp");
        let v = invoke(&mut app, &ctx, json!({"visual": {"index": 0, "warp": {"style": "arc"}, "widthCm": 10}}));
        assert_eq!(v["visuals"][0]["warp"]["bend"], DEFAULT_BEND, "a style starts bent");
        assert!((v["visuals"][0]["widthCm"].as_f64().unwrap() - 10.0).abs() < 1e-6);
        // A new scale: the visual keeps its size in cm.
        invoke(&mut app, &ctx, json!({"scale": {"cm": 40}}));
        app.mockup.as_mut().unwrap().refresh(&ctx);
        let v = app.mockup.as_ref().unwrap().describe();
        assert!((v["visuals"][0]["widthCm"].as_f64().unwrap() - 10.0).abs() < 1e-6, "{v}");
        let corners = v["visuals"][0]["corners"].clone();
        let params = app.mockup.as_ref().unwrap().params(&app).unwrap();
        assert_eq!(params["visuals"][0]["corners"], corners);
        assert_eq!(params["visuals"][0]["warp"]["style"], "arc");
        let out = dir.join("mockup.png").to_string_lossy().into_owned();
        invoke(&mut app, &ctx, json!({"reset": true}));
        let v = app.mockup.as_ref().unwrap().describe();
        assert_eq!(v["visuals"][0]["warp"]["style"], "none");
        invoke(&mut app, &ctx, json!({"export": out}));
        assert!(app.mockup.is_none() && std::path::Path::new(&out).exists());
        // No image on the clipboard: said, nothing changes.
        app.services.clipboard_get_image = Some(Box::new(|| Ok(None)));
        open(&mut app);
        let v = invoke(&mut app, &ctx, json!({"paste": true}));
        assert!(v["photo"].is_null());
        assert_eq!(v["status"], tl!("The clipboard holds no image"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Lasso zones: a new one, one taken away from; a visual stops at the zone under its centre
    /// (or the one chosen, or none); the zones reach the export.
    #[test]
    fn zones_mask_the_visuals_and_reach_the_export() {
        let dir = temp_dir("zones");
        let (photo, logo) = files(&dir, 400, 300, "#d8d8d8");
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        open(&mut app);
        invoke(&mut app, &ctx, json!({"photo": photo, "scale": {"from": [0, 10], "to": [100, 10], "cm": 10}}));
        // Two zones: left and right halves; a strip taken from the left one.
        let v = invoke(&mut app, &ctx, json!({"zones": {"lasso": [[0, 0], [200, 0], [200, 300], [0, 300]]}}));
        assert_eq!(v["zones"].as_array().unwrap().len(), 1);
        invoke(&mut app, &ctx, json!({"zones": {"mode": "new", "lasso": [[200, 0], [400, 0], [400, 300], [200, 300]]}}));
        let v = invoke(
            &mut app,
            &ctx,
            json!({"zones": {"select": 0, "mode": "subtract", "lasso": [[0, 0], [50, 0], [50, 300], [0, 300]], "rename": {"index": 0, "name": "Face"}}}),
        );
        assert_eq!(v["zones"][0]["name"], "Face");
        // A visual on the left half, across the middle: masked by "Face".
        let v = invoke(&mut app, &ctx, json!({"addFiles": [logo], "visual": {"index": 0, "x": 190, "y": 150, "widthCm": 6}}));
        assert_eq!(v["visuals"][0]["zone"], 0);
        assert_eq!(v["visuals"][0]["zoneChoice"], "auto");
        app.mockup.as_mut().unwrap().refresh(&ctx);
        let params = app.mockup.as_ref().unwrap().params(&app).unwrap();
        assert_eq!(params["visuals"][0]["zone"], 0);
        let out = dir.join("zones.png").to_string_lossy().into_owned();
        invoke(&mut app, &ctx, json!({"render": {"shadows": 0, "folds": 0, "opacity": 100}, "export": out}));
        let st = app.session.active().unwrap();
        let surf = st.doc.layers[0].surface().unwrap();
        let red = |x: i32| surf.read_region(photocraft_geom::Rect::new(x, 150, x + 1, 151));
        assert!(red(180)[0] > 0.7 && red(180)[1] < 0.2, "inside Face: {:?}", red(180));
        assert!(red(210)[0] < 0.9 && red(210)[1] > 0.7, "past Face's edge: the photo, {:?}", red(210));
        // Chosen: not masked.
        open(&mut app);
        invoke(
            &mut app,
            &ctx,
            json!({"photo": photo, "scale": {"from": [0, 10], "to": [100, 10], "cm": 10}, "zones": {"lasso": [[0, 0], [200, 0], [200, 300], [0, 300]]}}),
        );
        let v = invoke(&mut app, &ctx, json!({"addFiles": [logo], "visual": {"index": 0, "x": 190, "y": 150, "zone": "none"}}));
        assert!(v["visuals"][0]["zone"].is_null());
        // A new photo clears the zones.
        let v = invoke(&mut app, &ctx, json!({"photo": photo}));
        assert_eq!(v["zones"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The zoom keeps the photo point under the pointer, goes no further than 800 % nor below the
    /// whole photo; the hand moves the zoomed photo; a closed lasso turns off.
    #[test]
    fn the_zoom_and_the_hand() {
        let dir = temp_dir("zoom");
        let (photo, _) = files(&dir, 600, 400, "#d8d8d8");
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        open(&mut app);
        invoke(&mut app, &ctx, json!({"photo": photo}));
        let canvas = ERect::from_min_size(pos2(320.0, 52.0), vec2(800.0, 800.0));
        let d = app.mockup.as_mut().unwrap();
        d.canvas_rect = Some(canvas);
        let fit = fit_zoom(canvas, 600.0, 400.0);
        let at = pos2(500.0, 300.0);
        let (r0, z0) = view_rect(canvas, 600.0, 400.0, d.view);
        let q0 = ((at.x - r0.left()) / z0, (at.y - r0.top()) / z0);
        d.zoom_to(fit * 3.0, at);
        let (r1, z1) = view_rect(canvas, 600.0, 400.0, d.view);
        assert!((z1 - fit * 3.0).abs() < 1e-4);
        let q1 = ((at.x - r1.left()) / z1, (at.y - r1.top()) / z1);
        assert!((q0.0 - q1.0).abs() < 0.01 && (q0.1 - q1.1).abs() < 0.01, "{q0:?} {q1:?}");
        d.pan(vec2(30.0, 0.0));
        let (r2, _) = view_rect(canvas, 600.0, 400.0, d.view);
        assert!((r2.left() - r1.left() - 30.0).abs() < 0.01, "the hand moves the photo");
        d.zoom_to(100.0, at);
        assert_eq!(d.zoom_now(), Some(MAX_ZOOM));
        d.zoom_step(-1, None);
        assert!(d.zoom_now().unwrap() < MAX_ZOOM);
        d.zoom_to(fit * 0.5, at);
        assert!(d.view.is_none(), "below the whole photo: fitted");
        // The lasso turns off once its zone is closed.
        invoke(&mut app, &ctx, json!({"zones": {"lassoOn": true}}));
        assert!(app.mockup.as_ref().unwrap().lassoing);
        let v = invoke(&mut app, &ctx, json!({"zones": {"lasso": [[10, 10], [100, 10], [100, 100]]}}));
        assert_eq!(v["lasso"]["on"], false);
        assert_eq!(v["zones"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Screenshots for review (`DTF_VISUALS=<folder>`, the photo from `MOCKUP_PHOTO` if set): the
    /// window in French with a photo, its scale and two visuals, the first selected; then empty.
    #[test]
    fn capture_mockup() {
        let Some(out) = std::env::var_os("DTF_VISUALS").map(std::path::PathBuf::from) else { return };
        std::fs::create_dir_all(&out).unwrap();
        let _fr = crate::i18n::language_scope(crate::i18n::Lang::from_pref("fr"));
        let dir = temp_dir("capture");
        let (flat, _) = files(&dir, 1000, 1000, "#e9e7e2");
        let photo = std::env::var("MOCKUP_PHOTO").unwrap_or(flat);
        for (name, mode) in [("mockup", 1), ("mockup_vide", 0), ("mockup_deformer", 2), ("mockup_zones", 3), ("mockup_zoom", 4)] {
            let photo = if mode >= 3 { std::env::var("MOCKUP_PHOTO4").unwrap_or_else(|_| photo.clone()) } else { photo.clone() };
            let photo = photo.clone();
            let mut h = egui_kittest::Harness::builder().with_size(vec2(1440.0, 900.0)).with_max_steps(64).wgpu().build_eframe(move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
                crate::dtf_panel::dtf_craft_startup(&mut app);
                let ctx = cc.egui_ctx.clone();
                open(&mut app);
                if mode > 0 {
                    // A club logo: a red block with a white band, and a small blue square.
                    app.run("file.new", json!({"name": "logo-club", "width": 1200, "height": 900, "resolution": 300, "background": "transparent"})).unwrap();
                    app.run("select.rect", json!({"x": 0, "y": 0, "width": 1200, "height": 900})).unwrap();
                    app.run("edit.fill", json!({"color": "#c8102e"})).unwrap();
                    app.run("select.rect", json!({"x": 0, "y": 380, "width": 1200, "height": 140})).unwrap();
                    app.run("edit.fill", json!({"color": "#ffffff"})).unwrap();
                    app.run("select.deselect", json!({})).unwrap();
                    let w = photo_width(&photo);
                    invoke(
                        &mut app,
                        &ctx,
                        json!({"photo": photo, "scale": {"from": [w * 0.245, w * 0.44], "to": [w * 0.765, w * 0.44], "cm": 52}, "addDocument": true}),
                    );
                    invoke(&mut app, &ctx, json!({"visual": {"index": 0, "widthCm": 24, "x": w * 0.5, "y": w * 0.45}}));
                    app.run("file.new", json!({"name": "logo-coeur", "width": 600, "height": 600, "resolution": 300, "background": "transparent"})).unwrap();
                    app.run("select.rect", json!({"x": 0, "y": 0, "width": 600, "height": 600})).unwrap();
                    app.run("edit.fill", json!({"color": "#1f4fa8"})).unwrap();
                    app.run("select.deselect", json!({})).unwrap();
                    invoke(&mut app, &ctx, json!({"addDocument": true}));
                    invoke(&mut app, &ctx, json!({"visual": {"index": 1, "widthCm": 6, "x": w * 0.64, "y": w * 0.26}, "select": 0}));
                    invoke(&mut app, &ctx, json!({"select": 0}));
                    if mode == 2 {
                        invoke(&mut app, &ctx, json!({"visual": {"index": 0, "warp": {"style": "arc", "bend": 25}}, "tool": "warp"}));
                    }
                    if mode == 3 {
                        // The four views: one zone each; the logo across the front view's sleeve.
                        invoke(&mut app, &ctx, json!({"scale": {"from": [337, 219], "to": [636, 219], "cm": 52}}));
                        app.mockup.as_mut().unwrap().refresh(&ctx);
                        let v = invoke(&mut app, &ctx, json!({"zones": {"auto": true}}));
                        eprintln!("zones found: {}", v["zones"]);
                        invoke(&mut app, &ctx, json!({"visual": {"index": 0, "widthCm": 30, "x": 600, "y": 250}, "zones": {"select": 0}}));
                        invoke(&mut app, &ctx, json!({"visual": {"index": 1, "widthCm": 8, "x": 1100, "y": 200}, "select": 0}));
                    }
                    if mode == 4 {
                        // Zoomed in on the front view's neck, a lasso half drawn (taking the collar
                        // away from the front zone).
                        invoke(&mut app, &ctx, json!({"scale": {"from": [337, 219], "to": [636, 219], "cm": 52}, "zones": {"auto": true}}));
                        invoke(&mut app, &ctx, json!({"zones": {"select": 0, "mode": "subtract", "lassoOn": true}, "select": null}));
                        if let Some(d) = app.mockup.as_mut() {
                            d.view = Some(View { zoom: 2.4, center: [486.0, 160.0] });
                            d.lasso = vec![[420.0, 92.0], [450.0, 118.0], [486.0, 128.0], [522.0, 118.0]];
                        }
                    }
                }
                app.beta = Default::default();
                app
            });
            h.state_mut().session.prefs.edit(|p| p.interface.language = "fr".into());
            h.run_steps(8);
            h.render().unwrap().save(out.join(format!("{name}.png"))).unwrap();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    fn photo_width(path: &str) -> f64 {
        f64::from(cmd::load_photo(path).map_or(1000, |p| p.width))
    }
}
