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
            advice: "300 ppp, « Rogner la transparence » coché, profondeur 8. La fenêtre part de la taille actuelle du visuel et de la résolution du document. Changer la résolution recalcule la taille en cm sans toucher aux pixels. Saisir une dimension redimensionne l'image ; l'autre suit les proportions. Si la taille obtenue à 300 ppp est trop petite, passez d'abord par un bouton ×2, ×3 ou ×4.",
        },
        "filter.dtf.removeWhiteBackground" => Sheet {
            what: "Supprimer un fond uni, souvent blanc, en partant des bords vers l'intérieur. Le travail se fait sur une copie « Sans fond » du calque ; l'original est masqué et intact. Le blanc à l'intérieur du visuel est conservé.",
            when: "Fichier client sur fond blanc ou uni (JPEG, capture, logo exporté sans transparence), à la place de la gomme magique.",
            advice: "Tolérance 10 %, contour 2 px, « Supprimer le liseré » coché. L'aperçu s'affiche dès l'ouverture de la fenêtre, sur la dernière couleur utilisée (Fond de contrôle ou Fond d'aperçu). Sur un fichier déjà transparent, la détection ne trouve pas de fond : prenez la couleur à la pipette si besoin. Ajoutez les zones fermées restantes avec « Ajouter des trous » : un clic par zone dans l'aperçu, percées avec le fond à l'application. Avant d'appliquer, Ctrl+Alt+Z annule le dernier changement de la fenêtre (un trou, un réglage) et un nouvel appui le rétablit. Sans bouton enfoncé, un clic sur l'image ne fait rien pendant l'aperçu.",
        },
        "dtf.controlBackground" => Sheet {
            what: "Placer sous l'image un calque de couleur unie nommé « backfill », de la couleur du textile, pour juger le rendu : liserés, pixels clairs, contraste.",
            when: "Après le détourage et les retouches, pour contrôler le visuel avant export.",
            advice: "#231c1c pour un textile foncé, la couleur réelle du textile sinon. Supprimez le fond avant l'export (DTF › Suppression fond de contrôle), sinon il sera imprimé.",
        },
        "filter.dtf.removeSemiTransparent" => Sheet {
            what: "Rendre opaques ou transparents les pixels semi-transparents de la frange du visuel : le bord extérieur et le bord des trous, sur la largeur choisie. Ces pixels s'impriment mal en DTF. L'intérieur du visuel (ombres, dégradés, lissage entre deux couleurs) n'est pas touché.",
            when: "Sur un visuel détouré ou un PNG avec ombres ou flous, avant l'export. Pour garder l'effet d'un dégradé, préférez Halftone DTF.",
            advice: "Seuil 62 %, largeur 2 px. Comparez avec « Afficher l'avant » sur les bords fins.",
        },
        "dtf.defringe" => Sheet {
            what: "Recolorer les pixels du bord du visuel, et du bord des trous, avec la couleur de leurs voisins intérieurs. Le liseré hérité de l'ancien fond, clair ou sombre, disparaît.",
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
        "dtf.colorAnalyzer" => Sheet {
            what: "Relever les couleurs principales du visuel (RVB, profil sRGB, transparence ignorée) et trouver pour chacune la couleur la plus proche d'un nuancier que vous chargez (.ase, .aco, .csv). L'écart est donné en ΔE 2000. Dans « Couleurs remplacées », des patchs avec les références s'affichent à côté du visuel ; une fiche PDF A4 les reprend. L'export PNG remplace les couleurs cochées par celles du nuancier dans une copie, ouverte dans un nouvel onglet : le document n'est jamais modifié.",
            when: "Logo ou visuel en aplats, quand le client impose des couleurs de référence, ou pour documenter un travail avant impression.",
            advice: "8 couleurs, regroupement ΔE 5, couleurs sous 0,5 % ignorées. Pour remplacer une seule zone ou une couleur oubliée, « Plage de couleurs » : clic sur la couleur (Maj+clic pour en ajouter, Alt+clic pour en retirer), tolérance 20 par défaut, « Pixels contigus » pour ne prendre que la zone cliquée ; choisissez la couleur du nuancier puis « Appliquer ». Les plages passent après les couleurs cochées, dans l'export PNG et sur la fiche PDF ; décochez une plage appliquée pour la mettre de côté sans la supprimer. ΔE jusqu'à 2 : écart imperceptible ; de 2 à 5 : visible de près ; au-delà : autre teinte, choisissez une autre référence avec « Changer… ». Une couleur à plus de ΔE 5 de sa référence (souvent un contour lissé) n'est pas cochée d'office. Le remplacement garde les contours lissés et les dégradés ; vérifiez-le avec l'aperçu « Couleurs remplacées ».",
        },
        "dtf.vectorize" => Sheet {
            what: "Redessiner le visuel en formes vectorielles nettes, une couleur unie par forme : les couleurs sont réduites à une palette (8 par défaut), puis chaque zone est tracée en courbes. Les formes sont découpées, sans superposition. Le fond lu au bord du visuel peut être supprimé ; la même couleur à l'intérieur du visuel reste. L'export SVG est à la taille d'impression du document et peut s'ouvrir dans un nouvel onglet en calques de forme : le document n'est jamais modifié.",
            when: "Logo ou visuel en aplats de basse définition, flou ou pixelisé, à remettre au propre avant l'impression ou à fournir en vectoriel (découpe, broderie, sérigraphie).",
            advice: "Logo · aplats, 8 couleurs, détail moyen, courbes. Réduisez le nombre de couleurs si des contours lissés forment des couleurs en trop (les liserés fins d'une couleur intermédiaire entre deux autres sont déjà rendus aux couleurs voisines et les bords lissés) ; « Élevé » garde les petits détails, « Faible » simplifie. « Angles vifs » pour les textes et formes géométriques. Décochez une couleur de la palette pour la retirer du SVG. « Fermer les joints entre couleurs » (coché) évite tout liseré transparent entre deux couleurs à l'impression ; décochez-le pour un traceur de découpe. Vérifiez avec « Afficher les contours » et la vue côte à côte ; zoomez (Ctrl+molette, jusqu'à 800 %) pour juger les contours de près. Chargez le nuancier de référence (partagé avec l'Analyseur couleur) : chaque couleur détectée reçoit la référence la plus proche et son ΔE ; cochée dans ⇄ (automatique jusqu'à ΔE 5 ou si vous la choisissez avec « Changer… »), la couleur est remplacée par celle de la référence et les formes portent son nom dans le SVG. « Fiche PDF… » : l'original et le vectorisé côte à côte, les couleurs du SVG et leurs références. Moteur « VectorCraft » : partez d'un préréglage (Logo aplats, 6 couleurs, Noir et blanc…), puis ajustez Tracés (fidélité), Angles, Bruit et Méthode ; « Superposée » évite tout interstice entre couleurs ; la palette « Nuancier » trace directement avec les couleurs du nuancier de référence. Sur un JPEG flou, le moteur « DTF Craft » donne souvent des bords plus propres.",
        },
        _ => return None,
    })
}

/// The tooltip of a setting (`key` as in the command's spec), or of a tab's own control
/// (`apply`, `cancel`, `showBefore`, `showBackground`, `previewColor`,
/// `holeEraser`, `removeLastHole`, `backgroundPicker`, `upscale`).
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
        ("dtf.conformity", "upscale") => "Agrandissement IA du document dans une copie nommée « nom x2 » (x3, x4), à la résolution de la fenêtre ; l'original n'est pas touché. Le bouton indique la taille obtenue en cm et en pixels. Calcul hors ligne, de quelques secondes à plusieurs minutes.",
        ("filter.dtf.removeWhiteBackground", "detectColor") => "Lit la couleur du fond sur les bords de l'image, s'ils sont d'une couleur unie. Sur un fichier déjà transparent, rien n'est détecté ni supprimé. Décochez pour choisir la couleur vous-même, ou prenez-la à la pipette.",
        ("filter.dtf.removeWhiteBackground", "color") => "Couleur du fond à supprimer, quand elle n'est pas détectée.",
        ("filter.dtf.removeWhiteBackground", "tolerance") => "Écart de couleur encore pris pour du fond, en %. Plus haut : absorbe le bruit JPEG, mais peut mordre sur les couleurs claires du visuel.",
        ("filter.dtf.removeWhiteBackground", "contour") => "Épaisseur du bord, en px, nettoyée du mélange avec le fond.",
        ("filter.dtf.removeWhiteBackground", "removeHalo") => "Retire le blanc mélangé au contour, pour éviter un liseré clair sur textile foncé. Les couleurs claires restent opaques.",
        ("filter.dtf.removeWhiteBackground", "interiorHoles") => "Supprime aussi le fond enfermé dans le visuel (intérieur d'un O, d'un anneau).",
        ("filter.dtf.removeWhiteBackground", "minHoleSize") => "Les zones fermées plus petites que cette surface, en pixels, sont gardées.",
        ("filter.dtf.removeWhiteBackground", "previewColor") => "Couleur sur laquelle l'aperçu s'affiche : la dernière couleur utilisée, ici ou dans Fond de contrôle. Sert seulement à l'aperçu.",
        ("filter.dtf.removeWhiteBackground", "holeEraser") => "Bouton enfoncé : chaque clic dans une zone fermée ajoute un trou à l'aperçu, sur le fond d'aperçu. Les trous sont percés avec le fond en cliquant sur Appliquer, avec le même traitement que le contour extérieur (Tolérance, Contour, Supprimer le liseré) : pas de liseré autour. Un second clic sur le bouton le relâche. Il ne peut pas être enfoncé en même temps que la pipette. Ctrl+Alt+Z retire le dernier trou, puis le remet.",
        ("filter.dtf.removeWhiteBackground", "removeLastHole") => "Retire de l'aperçu le dernier trou ajouté. Annuler retire tous les trous.",
        ("filter.dtf.removeWhiteBackground", "backgroundPicker") => "Bouton enfoncé : cliquez sur le fond dans l'image pour choisir la couleur à supprimer. « Détecter la couleur du fond » se décoche et l'aperçu se met à jour.",
        ("filter.dtf.removeSemiTransparent", "width") => "Largeur de la frange traitée, en px, autour de la transparence (bord du visuel et trous). Les pixels semi-transparents plus à l'intérieur (ombres, dégradés, lissage entre deux couleurs) ne changent pas.",
        ("dtf.controlBackground", "color") => "Couleur du textile. Cliquez sur la pastille pour la choisir.",
        ("dtf.controlBackground", "showBackground") => "Ajoute le fond sous tous les calques ; un fond déjà présent est remplacé.",
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
    let mut open = f.get("__help").and_then(Value::as_bool).unwrap_or(false);
    sheet_toggle(ui, &mut open, command);
    f.insert("__help".into(), json!(open));
}

/// The "?" button and, while `open`, the tool's sheet.
pub fn sheet_toggle(ui: &mut egui::Ui, open: &mut bool, command: &str) {
    let Some(sheet) = sheet(command) else { return };
    let t = Tokens::get(ui.ctx());
    // One row's height: a right-to-left layout alone would take the whole tab's height.
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 22.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let b = egui::Button::new(RichText::new("?").strong()).selected(*open).min_size(egui::vec2(22.0, 20.0));
        if ui.add(b).on_hover_text(if *open { "Masquer l'aide" } else { "Aide de l'outil" }).clicked() {
            *open = !*open;
        }
    });
    if !*open {
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
