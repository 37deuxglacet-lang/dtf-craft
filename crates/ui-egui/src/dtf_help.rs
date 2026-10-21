// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! The DTF tools' help: a "?" button that opens the tool's sheet (what it is for, when to use
//! it, the advised settings) at the top of its tab or dialog, and a tooltip on each setting.
//!
//! The texts are in French (DTF Craft's language for now), the same in every UI language.

use egui::RichText;
use serde_json::{Map, Value, json};

use crate::theme::Tokens;

/// A tool's help sheet.
pub struct Sheet {
    pub what: &'static str,
    pub when: &'static str,
    pub advice: &'static str,
}

/// The help sheet of a DTF tool (`None` for other commands).
pub fn sheet(command: &str) -> Option<Sheet> {
    Some(match command {
        "dtf.conformity" => Sheet {
            what: "Mettre le visuel à sa taille d'impression en cm, à la bonne résolution, et l'agrandir par IA s'il est trop petit. Le calque d'arrière-plan devient un calque normal, renommé ; la transparence autour du visuel est rognée (2 px gardés), puis l'image est redimensionnée.",
            when: "En premier, dès l'ouverture du fichier client, avant le détourage et les retouches.",
            advice: "300 ppp, « Rogner la transparence » coché, profondeur 8. L'onglet part de la taille actuelle du visuel et de la résolution du document. Changer la résolution recalcule la taille en cm sans toucher aux pixels. Saisir une dimension redimensionne l'image ; l'autre suit les proportions. Si la taille obtenue à 300 ppp est trop petite, passez d'abord par un bouton ×2, ×3 ou ×4.",
        },
        "filter.dtf.removeWhiteBackground" => Sheet {
            what: "Supprimer un fond uni, souvent blanc, en partant des bords vers l'intérieur. Le travail se fait sur une copie « Sans fond » du calque ; l'original est masqué et intact. Le blanc à l'intérieur du visuel est conservé.",
            when: "Fichier client sur fond blanc ou uni (JPEG, capture, logo exporté sans transparence), à la place de la gomme magique.",
            advice: "Tolérance 10 %, contour 2 px, « Supprimer le liseré » coché. L'aperçu s'affiche dès l'ouverture de l'onglet, sur la dernière couleur utilisée (Fond de contrôle ou Fond d'aperçu). Effacez les zones fermées restantes à la gomme magique (« Ajouter des trous »), en réglant sa tolérance.",
        },
        "dtf.controlBackground" => Sheet {
            what: "Placer sous l'image un calque de couleur unie nommé « backfill », de la couleur du textile, pour juger le rendu : liserés, pixels clairs, contraste.",
            when: "Après le détourage et les retouches, pour contrôler le visuel avant export.",
            advice: "#231c1c pour un textile foncé, la couleur réelle du textile sinon. Supprimez le fond avant l'export, sinon il sera imprimé.",
        },
        "filter.dtf.removeSemiTransparent" => Sheet {
            what: "Rendre chaque pixel soit opaque, soit transparent. Les pixels semi-transparents (ombres douces, dégradés vers le vide, bords flous) s'impriment mal en DTF.",
            when: "Sur un visuel détouré ou un PNG avec ombres ou flous, avant l'export. Pour garder l'effet d'un dégradé, préférez Halftone DTF.",
            advice: "Seuil 62 %. Comparez avec « Afficher l'avant » sur les bords fins.",
        },
        "dtf.defringe" => Sheet {
            what: "Recolorer les pixels du bord du visuel avec la couleur de leurs voisins intérieurs. Le liseré hérité de l'ancien fond, clair ou sombre, disparaît.",
            when: "Après un détourage, quand un fin contour de l'ancien fond reste visible sur le fond de contrôle.",
            advice: "Largeur 1 px, 2 px au plus. Au-delà, les détails fins du contour s'empattent.",
        },
        "dtf.sharpen" => Sheet {
            what: "Accentuer les détails et les contours du visuel (netteté optimisée), pour compenser le léger flou d'un agrandissement ou d'un fichier client moyen.",
            when: "Après la conformité ou l'agrandissement IA, sur le visuel à sa taille finale.",
            advice: "Quantité 100 %, rayon 1 px, réduire le bruit 10 %. Trop de netteté crée des halos clairs autour des contours.",
        },
        "dtf.saturation" => Sheet {
            what: "Renforcer ou atténuer la vivacité des couleurs, sans changer leur teinte ni leur luminosité. Compense des couleurs qui sortent plus ternes sur textile qu'à l'écran.",
            when: "En fin de préparation, après la netteté, avant l'export.",
            advice: "+15. Au-delà de +30, les aplats saturés peuvent perdre leurs détails.",
        },
        "filter.dtf.halftone" => Sheet {
            what: "Remplacer les zones semi-transparentes, ou tout le visuel, par une trame de points pleins. Le dégradé reste visible tout en s'imprimant en pixels opaques. Le résultat est un groupe « Halftone DTF » au-dessus du calque : le calque tramé « Halftone » sur un fond de contrôle ; le calque source est masqué et intact.",
            when: "Ombres douces, dégradés vers le vide, effets vintage ; pour un visuel entier tramé sur textile foncé ou clair.",
            advice: "25 lpi, angle 22,5°, point rond, point minimal 5 %, maximal 95 %. Montez la fréquence pour une trame plus fine, baissez-la pour un effet plus marqué.",
        },
        _ => return None,
    })
}

/// The tooltip of a setting (`key` as in the command's spec), or of a tab's own control
/// (`apply`, `cancel`, `showBefore`, `showBackground`, `deleteBackground`, `previewColor`,
/// `holeEraser`, `eraserTolerance`, `upscale`).
pub fn tip(command: &str, key: &str) -> Option<&'static str> {
    Some(match (command, key) {
        (_, "apply") => "Applique l'outil au calque actif. Annulable avec Édition › Annuler.",
        (_, "cancel") => "Arrête l'aperçu : l'image revient à son état actuel.",
        (_, "showBefore") => "Cochée : l'image d'origine. Décochée : le résultat de l'aperçu.",
        ("dtf.conformity", "width") => "Largeur d'impression du visuel, en cm. La hauteur suit les proportions. Saisir une valeur redimensionne l'image.",
        ("dtf.conformity", "height") => "Hauteur d'impression du visuel, en cm. La largeur suit les proportions. Saisir une valeur redimensionne l'image.",
        ("dtf.conformity", "resolution") => "Points par pouce à l'impression ; 300 ppp pour le DTF. La changer recalcule la largeur et la hauteur en cm : les pixels ne changent pas.",
        ("dtf.conformity", "resample") => "Calcul des pixels quand l'image change de taille. « Bicubique automatique » convient le plus souvent ; « Préserver les détails » pour agrandir un petit visuel.",
        ("dtf.conformity", "depth") => "8 bits par couche suffisent pour l'impression ; 16 bits gardent plus de nuances mais doublent le poids du fichier.",
        ("dtf.conformity", "trim") => "Supprime les bords transparents autour du visuel et laisse 2 px de marge.",
        ("dtf.conformity", "layerName") => "Nom donné au calque du visuel (« origin » par défaut), repère pour les actions. Vide : le nom ne change pas.",
        ("dtf.conformity", "upscale") => "Agrandissement IA du document dans une copie nommée « nom x2 » (x3, x4), à la résolution de l'onglet ; l'original n'est pas touché. Le bouton indique la taille obtenue en cm et en pixels. Calcul hors ligne, de quelques secondes à plusieurs minutes.",
        ("filter.dtf.removeWhiteBackground", "detectColor") => "Lit la couleur du fond sur les bords de l'image. Décochez pour la choisir vous-même.",
        ("filter.dtf.removeWhiteBackground", "color") => "Couleur du fond à supprimer, quand elle n'est pas détectée.",
        ("filter.dtf.removeWhiteBackground", "tolerance") => "Écart de couleur encore pris pour du fond, en %. Plus haut : absorbe le bruit JPEG, mais peut mordre sur les couleurs claires du visuel.",
        ("filter.dtf.removeWhiteBackground", "contour") => "Épaisseur du bord, en px, nettoyée du mélange avec le fond.",
        ("filter.dtf.removeWhiteBackground", "removeHalo") => "Retire le blanc mélangé au contour, pour éviter un liseré clair sur textile foncé. Les couleurs claires restent opaques.",
        ("filter.dtf.removeWhiteBackground", "interiorHoles") => "Supprime aussi le fond enfermé dans le visuel (intérieur d'un O, d'un anneau).",
        ("filter.dtf.removeWhiteBackground", "minHoleSize") => "Les zones fermées plus petites que cette surface, en pixels, sont gardées.",
        ("filter.dtf.removeWhiteBackground", "previewColor") => "Couleur sur laquelle l'aperçu s'affiche : la dernière couleur utilisée, ici ou dans Fond de contrôle. Sert seulement à l'aperçu.",
        ("filter.dtf.removeWhiteBackground", "holeEraser") => "Cochée : la gomme magique est active ; chaque clic dans une zone fermée du calque « Sans fond » l'efface. Un clic = une annulation. Décochée : l'outil précédent revient.",
        ("filter.dtf.removeWhiteBackground", "eraserTolerance") => "Écart de couleur effacé autour du point cliqué (0–255). Plus haut : la gomme mord davantage sur les couleurs proches.",
        ("dtf.controlBackground", "color") => "Couleur du textile. Cliquez sur la pastille pour la choisir.",
        ("dtf.controlBackground", "showBackground") => "Ajoute le fond sous tous les calques ; un fond déjà présent est remplacé.",
        ("dtf.controlBackground", "deleteBackground") => "Retire le calque « backfill ». À faire avant l'export.",
        ("filter.dtf.removeSemiTransparent", "threshold") => "Opacité, en %, à partir de laquelle un pixel devient opaque ; en dessous, il devient transparent. Plus bas : plus de matière gardée.",
        ("dtf.defringe", "width") => "Épaisseur du bord recoloré, en px. 1 à 2 px suffisent le plus souvent.",
        ("dtf.sharpen", "amount") => "Force de l'accentuation, en %.",
        ("dtf.sharpen", "radius") => "Largeur, en px, de la zone accentuée autour de chaque contour. 0,5 à 2 px à 300 ppp.",
        ("dtf.sharpen", "reduceNoise") => "Limite l'accentuation du grain et des artefacts JPEG, en %.",
        ("dtf.saturation", "saturation") => "De −100 (noir et blanc) à +100. Valeur positive : couleurs plus vives ; négative : plus ternes.",
        ("filter.dtf.halftone", "mode") => "Ce qui est tramé : les pixels semi-transparents seulement, ou le visuel entier selon sa luminosité pour un textile foncé ou clair.",
        ("filter.dtf.halftone", "frequency") => "Nombre de lignes de points par pouce (lpi), à la résolution du document. Plus haut : points plus petits et plus serrés.",
        ("filter.dtf.halftone", "angle") => "Inclinaison de la trame, en degrés. 22,5° ou 45° limitent l'effet d'escalier sur les lignes.",
        ("filter.dtf.halftone", "shape") => "Forme des points : rond, ellipse, ligne, carré, losange, croix.",
        ("filter.dtf.halftone", "minDot") => "Taille du plus petit point gardé. En dessous, la zone reste vide : évite les points trop fins qui ne tiennent pas à l'impression.",
        ("filter.dtf.halftone", "maxDot") => "Taille du plus gros point. Au-dessus, la zone devient pleine.",
        ("filter.dtf.halftone", "inBlack") => "Seuil bas de couverture (0–255) : en dessous, aucun point ; le reste de la gamme est étiré.",
        ("filter.dtf.halftone", "inWhite") => "Seuil haut de couverture (0–255) : au-dessus, la zone devient pleine.",
        ("filter.dtf.halftone", "gamma") => "Au-dessus de 1 : points plus petits dans les tons moyens (moins d'encre) ; en dessous de 1 : points plus gros.",
        _ => return None,
    })
}

/// The "?" button and, while it is on, the tool's sheet, at the top of its tab or dialog. The
/// open state lives in the fields (`__help`).
pub fn sheet_ui(ui: &mut egui::Ui, f: &mut Map<String, Value>, command: &str) {
    let Some(sheet) = sheet(command) else { return };
    let t = Tokens::get(ui.ctx());
    let mut open = f.get("__help").and_then(Value::as_bool).unwrap_or(false);
    // One row's height: a right-to-left layout alone would take the whole tab's height.
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 22.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let b = egui::Button::new(RichText::new("?").strong()).selected(open).min_size(egui::vec2(22.0, 20.0));
        if ui.add(b).on_hover_text(if open { "Masquer l'aide" } else { "Aide de l'outil" }).clicked() {
            open = !open;
        }
    });
    f.insert("__help".into(), json!(open));
    if !open {
        return;
    }
    egui::Frame::group(ui.style()).fill(t.card).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for (title, text) in [("À quoi ça sert", sheet.what), ("Quand l'utiliser", sheet.when), ("Réglages conseillés", sheet.advice)] {
            ui.label(RichText::new(title).strong());
            ui.label(RichText::new(text).color(t.text_dim));
            ui.add_space(4.0);
        }
    });
    ui.add_space(4.0);
}

/// A setting's tooltip over the rows drawn since `top` (from `ui.cursor().top()` before them):
/// shown while the pointer is over them, without taking their clicks or drags.
pub fn tip_over(ui: &mut egui::Ui, command: &str, key: &str, top: f32) {
    let Some(text) = tip(command, key) else { return };
    let bottom = ui.cursor().top();
    if bottom <= top {
        return;
    }
    let rect = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=bottom);
    let id = ui.id().with(("dtf-help-tip", command, key));
    ui.interact(rect, id, egui::Sense::hover()).on_hover_text(text);
}
