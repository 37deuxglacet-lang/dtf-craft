// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! Gang sheet layout (DTF Craft): visuals laid out on a film of fixed width (the laize) and
//! limited length, kept a margin away from the film edges and spaced from one another.
//!
//! Everything is measured in tenths of a millimetre ([`UNITS_PER_CM`]), whatever the print
//! resolution: the layout is the same at 300 ppi or 600 ppi and only the render converts to
//! pixels ([`to_px`]).
//!
//! * [`Sheet::pack`]: MaxRects placement (largest first, bottom-left rule, optional 90°
//!   rotation) around the visuals already on the sheet.
//! * [`Sheet::conflicts`] and [`Sheet::overlaps`]: which visuals are too close to another visual
//!   or to an edge, or on top of another one, for the warnings drawn on the sheet.
//! * [`Sheet::slide`] and [`Sheet::snap`]: alignment for manual placement.
//! * [`effective_dpi`] and [`DpiLevel`]: the resolution a visual prints at and its rating.

use serde::{Deserialize, Serialize};

/// Layout units per centimetre (one unit = 0.1 mm).
pub const UNITS_PER_CM: i64 = 100;
/// Layout units per inch.
pub const UNITS_PER_INCH: f64 = 254.0;
/// Largest sheet dimension or margin taken (about 110 km): keeps every sum far from overflow.
const LIMIT: i64 = 1 << 40;

/// A visual on the sheet: top-left corner and size as printed (rotation already applied).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl Item {
    pub fn new(x: i64, y: i64, w: i64, h: i64) -> Self {
        Item { x, y, w, h }
    }

    /// Whether `self` and `o` are closer than `gap` (touching at exactly `gap` is fine).
    pub fn too_close(&self, o: &Item, gap: i64) -> bool {
        let gap = gap.max(0);
        self.x < o.x.saturating_add(o.w).saturating_add(gap)
            && o.x < self.x.saturating_add(self.w).saturating_add(gap)
            && self.y < o.y.saturating_add(o.h).saturating_add(gap)
            && o.y < self.y.saturating_add(self.h).saturating_add(gap)
    }
}

/// Where [`Sheet::pack`] put one visual: top-left corner and whether it is turned 90° clockwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spot {
    pub x: i64,
    pub y: i64,
    pub rotated: bool,
}

/// The film: its width and longest length, the margin kept along its edges (`edge`) and the
/// spacing kept between two visuals (`gap`, the default; a visual may ask for more, see the
/// `gaps` arguments: two visuals stay apart by the larger of their two spacings).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sheet {
    pub width: i64,
    pub max_length: i64,
    pub edge: i64,
    pub gap: i64,
}

/// A direction to push a visual in ([`Sheet::slide`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// The spacing between visuals `i` and `j`: the larger of theirs (`gaps` may be shorter than the
/// list; a missing entry counts as `fallback`).
fn pair_gap(gaps: &[i64], fallback: i64, i: usize, j: usize) -> i64 {
    gaps.get(i).copied().unwrap_or(fallback).max(gaps.get(j).copied().unwrap_or(fallback)).max(0)
}

impl Sheet {
    pub fn new(width: i64, max_length: i64, edge: i64, gap: i64) -> Self {
        Sheet { width: width.clamp(0, LIMIT), max_length: max_length.clamp(0, LIMIT), edge: edge.clamp(0, LIMIT), gap: gap.clamp(0, LIMIT) }
    }

    /// This sheet with another default spacing.
    pub fn with_gap(self, gap: i64) -> Self {
        Sheet { gap: gap.clamp(0, LIMIT), ..self }
    }

    /// Width and length left for visuals once the edge margin is kept on every side.
    pub fn inner(&self) -> (i64, i64) {
        (self.width - 2 * self.edge, self.max_length - 2 * self.edge)
    }

    /// Whether a `w × h` visual fits on the sheet, as it is.
    pub fn fits(&self, w: i64, h: i64) -> bool {
        let (iw, il) = self.inner();
        w > 0 && h > 0 && w <= iw && h <= il
    }

    /// Whether `r` lies on the sheet with the edge margin kept.
    pub fn inside(&self, r: &Item) -> bool {
        let m = self.edge;
        r.w > 0
            && r.h > 0
            && r.x >= m
            && r.y >= m
            && r.x.saturating_add(r.w) <= self.width - m
            && r.y.saturating_add(r.h) <= self.max_length - m
    }

    /// `r` moved (not resized) the least so the edge margin is kept. A visual larger than the
    /// sheet stays against the top-left margin.
    pub fn clamp(&self, r: Item) -> Item {
        let m = self.edge;
        let x = r.x.min((self.width - m).saturating_sub(r.w)).max(m);
        let y = r.y.min((self.max_length - m).saturating_sub(r.h)).max(m);
        Item { x, y, ..r }
    }

    /// Whether `r`, standing for visual `skip` (`usize::MAX`: a new one) with spacing `gap`, comes
    /// closer to another visual of `items` than their spacing allows.
    pub fn collides(&self, items: &[Item], gaps: &[i64], skip: usize, r: &Item, gap: i64) -> bool {
        items.iter().enumerate().any(|(i, o)| i != skip && r.too_close(o, gap.max(gaps.get(i).copied().unwrap_or(self.gap))))
    }

    /// For each visual: true when it leaves the edge margin or comes closer to another visual
    /// than their spacing allows (overlaps included).
    pub fn conflicts(&self, items: &[Item], gaps: &[i64]) -> Vec<bool> {
        let mut out: Vec<bool> = items.iter().map(|r| !self.inside(r)).collect();
        for (i, a) in items.iter().enumerate() {
            for (j, b) in items.iter().enumerate().skip(i + 1) {
                if a.too_close(b, pair_gap(gaps, self.gap, i, j)) {
                    if let Some(c) = out.get_mut(i) {
                        *c = true;
                    }
                    if let Some(c) = out.get_mut(j) {
                        *c = true;
                    }
                }
            }
        }
        out
    }

    /// For each visual: true when it overlaps another one (pixels on top of each other, not just
    /// too close).
    pub fn overlaps(&self, items: &[Item]) -> Vec<bool> {
        let mut out = vec![false; items.len()];
        for (i, a) in items.iter().enumerate() {
            for (j, b) in items.iter().enumerate().skip(i + 1) {
                if a.too_close(b, 0) {
                    if let Some(c) = out.get_mut(i) {
                        *c = true;
                    }
                    if let Some(c) = out.get_mut(j) {
                        *c = true;
                    }
                }
            }
        }
        out
    }

    /// The sheet length the visuals need: the bottom of the lowest one plus the edge margin (0
    /// when there are none), never more than the longest sheet.
    pub fn used_length(&self, items: &[Item]) -> i64 {
        items.iter().map(|r| r.y.saturating_add(r.h)).max().map_or(0, |b| b.saturating_add(self.edge).clamp(0, self.max_length))
    }

    /// Visual `i` pushed towards `side` as far as the edge margin and the other visuals (with
    /// their spacing) let it go, without jumping over any of them.
    pub fn slide(&self, items: &[Item], gaps: &[i64], i: usize, side: Side) -> Item {
        let Some(r) = items.get(i).copied() else { return Item::default() };
        let m = self.edge;
        let horizontal = matches!(side, Side::Left | Side::Right);
        // Visuals in the band the move sweeps (its rows for a horizontal move, its columns for a
        // vertical one), widened by the spacing.
        let blockers = items.iter().enumerate().filter(|(j, o)| {
            let g = pair_gap(gaps, self.gap, i, *j);
            *j != i
                && if horizontal {
                    o.y < r.y.saturating_add(r.h).saturating_add(g) && r.y < o.y.saturating_add(o.h).saturating_add(g)
                } else {
                    o.x < r.x.saturating_add(r.w).saturating_add(g) && r.x < o.x.saturating_add(o.w).saturating_add(g)
                }
        });
        let mut out = r;
        match side {
            Side::Left => {
                let stop = blockers.filter(|(_, o)| o.x.saturating_add(o.w) <= r.x).map(|(j, o)| o.x + o.w + pair_gap(gaps, self.gap, i, j)).fold(m, i64::max);
                out.x = stop.min(r.x);
            }
            Side::Right => {
                let stop = blockers.filter(|(_, o)| o.x >= r.x.saturating_add(r.w)).map(|(j, o)| o.x - pair_gap(gaps, self.gap, i, j)).fold(self.width - m, i64::min);
                out.x = (stop - r.w).max(r.x);
            }
            Side::Top => {
                let stop = blockers.filter(|(_, o)| o.y.saturating_add(o.h) <= r.y).map(|(j, o)| o.y + o.h + pair_gap(gaps, self.gap, i, j)).fold(m, i64::max);
                out.y = stop.min(r.y);
            }
            Side::Bottom => {
                let stop = blockers.filter(|(_, o)| o.y >= r.y.saturating_add(r.h)).map(|(j, o)| o.y - pair_gap(gaps, self.gap, i, j)).fold(self.max_length - m, i64::min);
                out.y = (stop - r.h).max(r.y);
            }
        }
        out
    }

    /// `r` (visual `skip`, spacing `gap`) moved onto the nearest guide within `tolerance` on each
    /// axis: the edge margins, the sheet's centre, and for every other visual its edges (aligned)
    /// or its spacing (side by side).
    pub fn snap(&self, items: &[Item], gaps: &[i64], skip: usize, r: Item, gap: i64, tolerance: i64) -> Item {
        let m = self.edge;
        let mut xs = vec![m, self.width - m - r.w, (self.width - r.w) / 2];
        let mut ys = vec![m, self.max_length - m - r.h];
        for (j, o) in items.iter().enumerate().filter(|(j, _)| *j != skip) {
            let g = gap.max(gaps.get(j).copied().unwrap_or(self.gap));
            xs.extend([o.x, o.x + o.w - r.w, o.x + o.w + g, o.x - g - r.w]);
            ys.extend([o.y, o.y + o.h - r.h, o.y + o.h + g, o.y - g - r.h]);
        }
        let near = |v: i64, guides: &[i64]| guides.iter().copied().filter(|g| (g - v).abs() <= tolerance).min_by_key(|g| (g - v).abs()).unwrap_or(v);
        Item { x: near(r.x, &xs), y: near(r.y, &ys), ..r }
    }

    /// Places `sizes` (`(w, h)` as given, unrotated) in the room the `occupied` visuals leave,
    /// largest first, each where it keeps the sheet shortest (bottom-left rule), turned 90° when
    /// `allow_rotate` and that is better. Visuals are kept `gap` apart (pass the largest spacing
    /// involved, see [`Sheet::with_gap`]). `None` for a visual that finds no room.
    pub fn pack(&self, sizes: &[(i64, i64)], occupied: &[Item], allow_rotate: bool) -> Vec<Option<Spot>> {
        let (e, g) = (self.edge, self.gap);
        let (iw, il) = self.inner();
        let mut out = vec![None; sizes.len()];
        if iw <= 0 || il <= 0 {
            return out;
        }
        // Packing space: the inner area, one spacing wider and longer; every visual reserves its
        // size plus the spacing to its right and below, so neighbours stay `gap` apart and the
        // last column and row end exactly at the inner edge.
        let bin = FreeRect { x: 0, y: 0, w: iw + g, h: il + g };
        let mut free = vec![bin];
        for o in occupied {
            // A visual already there reserves what a packed one would: its size plus the spacing
            // to its right and below, in packing coordinates (the sheet's, less the edge margin).
            let used = FreeRect { x: o.x.saturating_sub(e), y: o.y.saturating_sub(e), w: o.w.saturating_add(g), h: o.h.saturating_add(g) };
            split(&mut free, &used.clip(&bin));
        }
        let mut order: Vec<usize> = (0..sizes.len()).collect();
        let key = |i: usize| {
            let (w, h) = sizes.get(i).copied().unwrap_or((0, 0));
            (w.max(h), w.saturating_mul(h))
        };
        order.sort_by(|a, b| key(*b).cmp(&key(*a)).then(a.cmp(b)));
        for i in order {
            let Some(&(w, h)) = sizes.get(i) else { continue };
            if w <= 0 || h <= 0 {
                continue;
            }
            if let Some(p) = place(&mut free, w.saturating_add(g), h.saturating_add(g), allow_rotate)
                && let Some(slot) = out.get_mut(i)
            {
                *slot = Some(Spot { x: p.0 + e, y: p.1 + e, rotated: p.2 });
            }
        }
        out
    }
}

/// The size, in layout units, of a `px_w × px_h` pixel visual printed `width` units wide (its
/// proportions kept).
pub fn size_for_width(px_w: u32, px_h: u32, width: i64) -> (i64, i64) {
    if px_w == 0 || px_h == 0 || width <= 0 {
        return (0, 0);
    }
    let h = (width as f64 * f64::from(px_h) / f64::from(px_w)).round() as i64;
    (width, h.max(1))
}

/// Layout units to pixels at `dpi`.
pub fn to_px(units: i64, dpi: f64) -> i64 {
    (units as f64 / UNITS_PER_INCH * dpi).round() as i64
}

/// Pixels to layout units at `dpi` (0 when `dpi` is not a positive number).
pub fn from_px(px: f64, dpi: f64) -> i64 {
    if !(dpi.is_finite() && dpi > 0.0 && px.is_finite()) {
        return 0;
    }
    (px / dpi * UNITS_PER_INCH).round() as i64
}

/// The resolution a visual `px` pixels wide prints at when it is `units` wide.
pub fn effective_dpi(px: u32, units: i64) -> f64 {
    if units <= 0 {
        return 0.0;
    }
    f64::from(px) / (units as f64 / UNITS_PER_INCH)
}

/// The rating of a visual's print resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DpiLevel {
    /// 300 ppi or more.
    Optimal,
    /// 250 ppi or more.
    Good,
    /// 200 ppi or more.
    Low,
    /// Under 200 ppi.
    Poor,
}

impl DpiLevel {
    pub fn of(dpi: f64) -> Self {
        // A hair under the threshold (rounding of the size in tenths of a millimetre) still rates.
        match dpi + 0.5 {
            d if d >= 300.0 => DpiLevel::Optimal,
            d if d >= 250.0 => DpiLevel::Good,
            d if d >= 200.0 => DpiLevel::Low,
            _ => DpiLevel::Poor,
        }
    }
}

// ---------------------------------------------------------------- MaxRects

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FreeRect {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

impl FreeRect {
    fn contains(&self, o: &FreeRect) -> bool {
        o.x >= self.x && o.y >= self.y && o.x + o.w <= self.x + self.w && o.y + o.h <= self.y + self.h
    }

    fn clip(&self, b: &FreeRect) -> FreeRect {
        let x0 = self.x.max(b.x);
        let y0 = self.y.max(b.y);
        let x1 = self.x.saturating_add(self.w).min(b.x + b.w);
        let y1 = self.y.saturating_add(self.h).min(b.y + b.h);
        FreeRect { x: x0, y: y0, w: (x1 - x0).max(0), h: (y1 - y0).max(0) }
    }
}

/// Places one `w × h` area (margin included) in the free list, bottom-left rule: the lowest
/// bottom edge, then the leftmost. Returns its corner and whether it was turned.
fn place(free: &mut Vec<FreeRect>, w: i64, h: i64, allow_rotate: bool) -> Option<(i64, i64, bool)> {
    let mut best: Option<((i64, i64), FreeRect, bool)> = None;
    for fr in free.iter() {
        let mut try_one = |rw: i64, rh: i64, rotated: bool| {
            if rw <= fr.w && rh <= fr.h {
                let score = (fr.y + rh, fr.x);
                if best.is_none_or(|(b, _, _)| score < b) {
                    best = Some((score, FreeRect { x: fr.x, y: fr.y, w: rw, h: rh }, rotated));
                }
            }
        };
        try_one(w, h, false);
        if allow_rotate && w != h {
            try_one(h, w, true);
        }
    }
    let (_, used, rotated) = best?;
    split(free, &used);
    Some((used.x, used.y, rotated))
}

/// MaxRects split: every free rectangle overlapping `used` is replaced by its up to four maximal
/// parts outside it; rectangles contained in another are pruned.
fn split(free: &mut Vec<FreeRect>, used: &FreeRect) {
    if used.w <= 0 || used.h <= 0 {
        return;
    }
    let mut out: Vec<FreeRect> = Vec::with_capacity(free.len() + 4);
    for fr in free.iter() {
        let overlaps = used.x < fr.x + fr.w && used.x + used.w > fr.x && used.y < fr.y + fr.h && used.y + used.h > fr.y;
        if !overlaps {
            out.push(*fr);
            continue;
        }
        if used.x > fr.x {
            out.push(FreeRect { x: fr.x, y: fr.y, w: used.x - fr.x, h: fr.h });
        }
        if used.x + used.w < fr.x + fr.w {
            out.push(FreeRect { x: used.x + used.w, y: fr.y, w: fr.x + fr.w - (used.x + used.w), h: fr.h });
        }
        if used.y > fr.y {
            out.push(FreeRect { x: fr.x, y: fr.y, w: fr.w, h: used.y - fr.y });
        }
        if used.y + used.h < fr.y + fr.h {
            out.push(FreeRect { x: fr.x, y: used.y + used.h, w: fr.w, h: fr.y + fr.h - (used.y + used.h) });
        }
    }
    out.retain(|r| r.w > 0 && r.h > 0);
    let mut keep = vec![true; out.len()];
    for i in 0..out.len() {
        if !keep.get(i).copied().unwrap_or(false) {
            continue;
        }
        let Some(a) = out.get(i).copied() else { continue };
        for (j, b) in out.iter().enumerate() {
            if i != j && keep.get(j).copied().unwrap_or(false) && b.contains(&a) && (*b != a || j < i) {
                if let Some(k) = keep.get_mut(i) {
                    *k = false;
                }
                break;
            }
        }
    }
    *free = out.into_iter().zip(keep).filter_map(|(r, k)| k.then_some(r)).collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 56 cm film, 100 cm sheets, 5 mm edge and spacing.
    fn film() -> Sheet {
        Sheet::new(5600, 10_000, 50, 50)
    }

    fn items(sizes: &[(i64, i64)], spots: &[Option<Spot>]) -> Vec<Item> {
        sizes
            .iter()
            .zip(spots)
            .filter_map(|(&(w, h), s)| s.map(|s| if s.rotated { Item::new(s.x, s.y, h, w) } else { Item::new(s.x, s.y, w, h) }))
            .collect()
    }

    #[test]
    fn packed_visuals_keep_the_margin_and_stay_short() {
        let sheet = film();
        let sizes: Vec<(i64, i64)> = (0..40).map(|i| (300 + (i * 370) % 1200, 250 + (i * 530) % 900)).collect();
        let spots = sheet.pack(&sizes, &[], true);
        let placed = items(&sizes, &spots);
        assert!(!placed.is_empty());
        assert!(sheet.conflicts(&placed, &[]).iter().all(|c| !c), "no visual closer than 5 mm to another or to an edge");
        let area: i64 = placed.iter().map(|r| (r.w + 50) * (r.h + 50)).sum();
        let used = sheet.used_length(&placed);
        assert!(used <= area / 5600 * 14 / 10 + 1500, "reasonably dense: {used} for {area}");
    }

    #[test]
    fn a_full_row_ends_exactly_at_the_inner_edge() {
        // 5600 − 2×50 = 5500: five 1060-wide visuals with four 50 gaps fill it exactly.
        let sheet = film();
        let sizes = vec![(1060, 500); 5];
        let spots = sheet.pack(&sizes, &[], false);
        let placed = items(&sizes, &spots);
        assert_eq!(placed.len(), 5);
        assert!(placed.iter().all(|r| r.y == 50), "{placed:?}");
        assert_eq!(placed.iter().map(|r| r.x + r.w).max(), Some(5550));
        assert_eq!(sheet.used_length(&placed), 600);
    }

    #[test]
    fn edge_and_spacing_are_separate() {
        // 1 cm edge, 4 mm spacing: inner width 5400 holds four 1320 visuals (4×1320 + 3×40).
        let sheet = Sheet::new(5600, 10_000, 100, 40);
        let sizes = vec![(1320, 400); 4];
        let placed = items(&sizes, &sheet.pack(&sizes, &[], false));
        assert_eq!(placed.len(), 4);
        assert_eq!(placed.iter().map(|r| r.x).min(), Some(100));
        assert_eq!(placed.iter().map(|r| r.x + r.w).max(), Some(5500));
        assert_eq!(sheet.used_length(&placed), 600);
        assert!(sheet.conflicts(&placed, &[]).iter().all(|c| !c));
        // One visual asking for 5 mm: its neighbour 4 mm away is now too close.
        let pair = [Item::new(100, 100, 400, 400), Item::new(540, 100, 400, 400)];
        assert_eq!(sheet.conflicts(&pair, &[40, 40]), vec![false, false]);
        assert_eq!(sheet.conflicts(&pair, &[50, 40]), vec![true, true]);
    }

    #[test]
    fn packing_goes_around_visuals_already_placed() {
        let sheet = film();
        let occupied = vec![Item::new(50, 50, 2000, 2000), Item::new(3000, 400, 1000, 3000)];
        let sizes = vec![(800, 800); 12];
        let spots = sheet.pack(&sizes, &occupied, false);
        let mut all = occupied.clone();
        all.extend(items(&sizes, &spots));
        assert_eq!(all.len(), 14);
        assert!(sheet.conflicts(&all, &[]).iter().all(|c| !c), "{all:?}");
    }

    #[test]
    fn what_does_not_fit_is_left_out() {
        let sheet = Sheet::new(1000, 1000, 50, 50);
        // Inner area 900 × 900: four 400-squares with their margin, not a fifth.
        let spots = sheet.pack(&[(400, 400); 5], &[], false);
        assert_eq!(spots.iter().filter(|s| s.is_some()).count(), 4);
        // Too wide either way, empty, or turned to fit.
        let spots = sheet.pack(&[(950, 10), (0, 10), (100, 880)], &[], true);
        assert_eq!(spots[0], None);
        assert_eq!(spots[1], None);
        assert!(spots[2].is_some());
        let tall = Sheet::new(1000, 400, 50, 50);
        let spots = tall.pack(&[(100, 800)], &[], true);
        assert!(spots[0].is_some_and(|s| s.rotated));
        assert_eq!(tall.pack(&[(100, 800)], &[], false), vec![None]);
    }

    #[test]
    fn conflicts_flag_overlaps_closeness_and_edges() {
        let sheet = film();
        let a = Item::new(50, 50, 500, 500);
        let touching = Item::new(600, 50, 500, 500); // exactly 5 mm away
        let close = Item::new(1140, 50, 500, 500); // 4 mm from `touching`
        let edge = Item::new(5200, 50, 400, 400); // over the right margin
        assert_eq!(sheet.conflicts(&[a, touching], &[]), vec![false, false]);
        assert_eq!(sheet.conflicts(&[a, touching, close], &[]), vec![false, true, true]);
        assert_eq!(sheet.overlaps(&[a, touching, close]), vec![false, false, false]);
        assert_eq!(sheet.overlaps(&[a, Item::new(400, 400, 300, 300)]), vec![true, true]);
        assert_eq!(sheet.conflicts(&[edge], &[]), vec![true]);
        assert!(sheet.collides(&[a, touching], &[], 1, &Item::new(560, 50, 500, 500), 50));
        assert!(!sheet.collides(&[a, touching], &[], 1, &Item::new(600, 50, 500, 500), 50));
        assert_eq!(sheet.clamp(Item::new(-300, 99_999, 500, 500)), Item::new(50, 9450, 500, 500));
    }

    #[test]
    fn slide_stops_at_neighbours_and_edges() {
        let sheet = film();
        let it = [Item::new(1000, 1000, 400, 400), Item::new(100, 1100, 300, 300), Item::new(2000, 3000, 300, 300)];
        // Left: stops 5 mm after the visual on its rows.
        assert_eq!(sheet.slide(&it, &[], 0, Side::Left).x, 450);
        // Right: nothing on its rows, up to the edge.
        assert_eq!(sheet.slide(&it, &[], 0, Side::Right).x, 5550 - 400);
        // Top: nothing above in its columns.
        assert_eq!(sheet.slide(&it, &[], 0, Side::Top).y, 50);
        // Visual 2 going up is not stopped by visual 0 (other columns).
        assert_eq!(sheet.slide(&it, &[], 2, Side::Top).y, 50);
        assert_eq!(sheet.slide(&it, &[], 9, Side::Top), Item::default());
    }

    #[test]
    fn snap_pulls_onto_edges_and_neighbours() {
        let sheet = film();
        let it = [Item::new(1000, 1000, 400, 400)];
        // 2 mm from sitting 5 mm right of the other: snapped there; y aligned with its top.
        let r = sheet.snap(&it, &[], usize::MAX, Item::new(1470, 1015, 300, 300), 50, 30);
        assert_eq!((r.x, r.y), (1450, 1000));
        // Near the left edge margin.
        assert_eq!(sheet.snap(&[], &[], usize::MAX, Item::new(70, 4000, 300, 300), 50, 30).x, 50);
        // Out of reach: unchanged.
        assert_eq!(sheet.snap(&it, &[], usize::MAX, Item::new(3000, 5000, 300, 300), 50, 30), Item::new(3000, 5000, 300, 300));
    }

    #[test]
    fn sizes_resolutions_and_ratings() {
        assert_eq!(size_for_width(3000, 1500, 2540), (2540, 1270));
        assert_eq!(size_for_width(0, 10, 100), (0, 0));
        assert_eq!(to_px(5600, 300.0), 6614);
        assert_eq!(from_px(3000.0, 300.0), 2540);
        assert_eq!(from_px(3000.0, 0.0), 0);
        assert!((effective_dpi(3000, 2540) - 300.0).abs() < 1e-9);
        assert_eq!(effective_dpi(3000, 0), 0.0);
        assert_eq!(DpiLevel::of(300.0), DpiLevel::Optimal);
        assert_eq!(DpiLevel::of(299.7), DpiLevel::Optimal);
        assert_eq!(DpiLevel::of(260.0), DpiLevel::Good);
        assert_eq!(DpiLevel::of(210.0), DpiLevel::Low);
        assert_eq!(DpiLevel::of(150.0), DpiLevel::Poor);
        assert_eq!(DpiLevel::of(f64::NAN), DpiLevel::Poor);
    }

    #[test]
    fn hostile_values_do_not_panic() {
        let sheet = Sheet::new(-5, i64::MAX / 4, -1, -7);
        let _ = sheet.pack(&[(i64::MAX / 8, 3), (-1, -1)], &[Item::new(i64::MIN / 8, 0, 5, 5)], true);
        let tiny = Sheet::new(10, 10, 50, 50);
        assert_eq!(tiny.pack(&[(1, 1)], &[], true), vec![None]);
        let far = [Item::new(i64::MAX / 8, i64::MIN / 8, 3, 3), Item::new(0, 0, 1, 1)];
        let _ = tiny.conflicts(&far, &[i64::MAX / 8]);
        let _ = tiny.overlaps(&far);
        let _ = tiny.slide(&far, &[], 1, Side::Bottom);
        let _ = tiny.snap(&far, &[], 0, Item::new(5, 5, 1, 1), 2, 3);
        let _ = tiny.used_length(&[Item::new(0, i64::MAX / 2, 1, i64::MAX / 2)]);
    }
}
