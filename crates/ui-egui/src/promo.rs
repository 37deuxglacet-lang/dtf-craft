// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! The start screen's "services" strip: Stéphane Glacet's sites, each as a card with a visual (a
//! coloured banner and its glyph), its name, a catch line and a button that opens the site in the
//! browser ([`crate::links::open`]).

use egui::{Color32, Mesh, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::PhotocraftApp;

/// One site: name, domain, catch line, glyph and the banner's two colours.
pub struct Site {
    pub name: &'static str,
    pub url: &'static str,
    pub domain: &'static str,
    pub pitch: &'static str,
    pub icon: &'static str,
    pub from: Color32,
    pub to: Color32,
}

pub const SITES: [Site; 5] = [
    Site {
        name: "Pixel-Net",
        url: "https://www.pixel-net.fr",
        domain: "pixel-net.fr",
        pitch: "Visuel trop complexe pour le logiciel ? Nous préparons vos fichiers DTF.",
        icon: "wand-sparkles",
        from: Color32::from_rgb(124, 58, 237),
        to: Color32::from_rgb(219, 39, 119),
    },
    Site {
        name: "Réparation DTF",
        url: "https://www.reparation-dtf.fr",
        domain: "reparation-dtf.fr",
        pitch: "Réparation et maintenance de vos imprimantes DTF.",
        icon: "wrench",
        from: Color32::from_rgb(234, 88, 12),
        to: Color32::from_rgb(245, 158, 11),
    },
    Site {
        name: "Formation DTF",
        url: "https://www.formation-dtf.fr",
        domain: "formation-dtf.fr",
        pitch: "Formez-vous : maintenance DTF, préparation des fichiers, RIP d'impression.",
        icon: "graduation-cap",
        from: Color32::from_rgb(37, 99, 235),
        to: Color32::from_rgb(6, 182, 212),
    },
    Site {
        name: "Chromilab",
        url: "https://www.chromilab.fr",
        domain: "chromilab.fr",
        pitch: "Toutes vos problématiques liées à la colorimétrie, de l'écran à l'impression.",
        icon: "palette",
        from: Color32::from_rgb(5, 150, 105),
        to: Color32::from_rgb(132, 204, 22),
    },
    Site {
        name: "Stéphane Glacet",
        url: "https://www.stephane-glacet.com",
        domain: "stephane-glacet.com",
        pitch: "Toutes vos formations PAO, Web et IA.",
        icon: "sparkles",
        from: Color32::from_rgb(51, 65, 85),
        to: Color32::from_rgb(99, 102, 241),
    },
];

/// The strip's height.
pub const HEIGHT: f32 = 262.0;
const GAP: f32 = 14.0;
const BANNER_H: f32 = 86.0;

/// The narrowest room the five cards take.
pub fn min_width() -> f32 {
    SITES.len() as f32 * 150.0 + (SITES.len() - 1) as f32 * GAP
}

/// A left-to-right gradient over `r`, its top corners rounded by `radius` (a convex fan, each
/// vertex coloured by where it lies across).
fn gradient(painter: &egui::Painter, r: Rect, radius: f32, from: Color32, to: Color32) {
    let colour = |x: f32| {
        let k = ((x - r.left()) / r.width().max(1.0)).clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * k).round() as u8;
        Color32::from_rgb(mix(from.r(), to.r()), mix(from.g(), to.g()), mix(from.b(), to.b()))
    };
    let mut outline = vec![r.left_bottom()];
    let arc = |c: Pos2, a0: f32, a1: f32, out: &mut Vec<Pos2>| {
        for k in 0..=8 {
            let a = a0 + (a1 - a0) * k as f32 / 8.0;
            out.push(pos2(c.x + radius * a.cos(), c.y + radius * a.sin()));
        }
    };
    let pi = std::f32::consts::PI;
    arc(pos2(r.left() + radius, r.top() + radius), pi, 1.5 * pi, &mut outline);
    arc(pos2(r.right() - radius, r.top() + radius), 1.5 * pi, 2.0 * pi, &mut outline);
    outline.push(r.right_bottom());
    let mut m = Mesh::default();
    m.colored_vertex(r.center(), colour(r.center().x));
    for p in &outline {
        m.colored_vertex(*p, colour(p.x));
    }
    let n = outline.len() as u32;
    for i in 1..n {
        m.add_triangle(0, i, i + 1);
    }
    m.add_triangle(0, n, 1);
    painter.add(egui::Shape::mesh(m));
}

/// The strip in `rect`: a title, then the cards side by side.
pub fn strip(app: &mut PhotocraftApp, ui: &mut egui::Ui, rect: Rect) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let painter = ui.painter_at(rect);
    // Title with a hairline either side.
    let title = "Les services de Stéphane Glacet";
    let g = painter.layout_no_wrap(title.into(), crate::theme::semibold(13.0), t.text_dim);
    let ty = rect.top() + 10.0;
    let tx = rect.center().x - g.size().x / 2.0;
    painter.line_segment([pos2(rect.left(), ty), pos2(tx - 14.0, ty)], Stroke::new(1.0, t.separator));
    painter.line_segment([pos2(tx + g.size().x + 14.0, ty), pos2(rect.right(), ty)], Stroke::new(1.0, t.separator));
    painter.galley(pos2(tx, ty - g.size().y / 2.0), g, t.text_dim);
    let n = SITES.len() as f32;
    let w = ((rect.width() - (n - 1.0) * GAP) / n).min(220.0);
    let total = n * w + (n - 1.0) * GAP;
    let mut x = rect.center().x - total / 2.0;
    let top = rect.top() + 30.0;
    let mut open = None;
    for (i, s) in SITES.iter().enumerate() {
        let card = Rect::from_min_size(pos2(x, top), vec2(w, rect.bottom() - top));
        if card_ui(ui, i, s, card, &t) {
            open = Some(s.url);
        }
        x += w + GAP;
    }
    if let Some(url) = open {
        crate::links::open(app, ui.ctx(), url);
    }
}

/// One card; true when its button was clicked.
fn card_ui(ui: &mut egui::Ui, i: usize, s: &Site, card: Rect, t: &crate::theme::Tokens) -> bool {
    let hover = ui.interact(card, ui.id().with(("promo-card", i)), Sense::hover()).hovered();
    let painter = ui.painter_at(card.expand(2.0));
    painter.rect_filled(card, 10.0, t.card);
    // The visual: the banner, two soft circles and the glyph.
    let banner = Rect::from_min_size(card.min, vec2(card.width(), BANNER_H));
    let clipped = ui.painter_at(banner);
    gradient(&clipped, banner, 10.0, s.from, s.to);
    // Kept inside the rounded corner.
    clipped.circle_filled(pos2(banner.right() - 30.0, banner.top() + 18.0), 30.0, Color32::from_white_alpha(28));
    clipped.circle_filled(pos2(banner.left() + 26.0, banner.bottom() + 6.0), 30.0, Color32::from_white_alpha(20));
    let glyph = Rect::from_center_size(banner.center(), vec2(40.0, 40.0));
    crate::icons::image(s.icon, 40.0, Color32::WHITE).paint_at(ui, glyph);
    // The card's corners stay round over the banner.
    painter.rect_stroke(card, 10.0, Stroke::new(if hover { 1.5 } else { 1.0 }, if hover { s.to } else { t.card_border }), StrokeKind::Inside);
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(card.left() + 12.0, banner.bottom() + 10.0), card.max - vec2(12.0, 10.0))));
    body.spacing_mut().item_spacing.y = 3.0;
    body.label(egui::RichText::new(s.name).font(crate::theme::semibold(14.0)).color(t.text));
    body.label(egui::RichText::new(s.domain).size(11.0).color(t.text_faint));
    body.add_space(2.0);
    body.add(egui::Label::new(egui::RichText::new(s.pitch).size(12.0).color(t.text_dim)).wrap());
    // The button sits at the bottom of every card.
    let b = Rect::from_min_max(pos2(card.left() + 12.0, card.bottom() - 40.0), pos2(card.right() - 12.0, card.bottom() - 12.0));
    let resp = ui.interact(b, ui.id().with(("promo-open", i)), Sense::click());
    let fill = if resp.hovered() { s.to } else { s.from };
    ui.painter().rect_filled(b, 6.0, fill);
    let label = ui.painter().layout_no_wrap("Ouvrir le site".into(), crate::theme::semibold(12.5), Color32::WHITE);
    let lw = label.size().x + 6.0 + 13.0;
    let start: Pos2 = pos2(b.center().x - lw / 2.0, b.center().y - label.size().y / 2.0);
    ui.painter().galley(start, label.clone(), Color32::WHITE);
    crate::icons::image("external-link", 13.0, Color32::WHITE).paint_at(ui, Rect::from_min_size(pos2(start.x + label.size().x + 6.0, b.center().y - 6.5), vec2(13.0, 13.0)));
    resp.on_hover_text(s.url).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}
