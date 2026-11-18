# DTF Craft

Version de PhotoCraft dédiée à la préparation de fichiers DTF (impression directe sur film),
par Stéphane GLACET (FORMATION DTF) — [www.dtfcraft.fr](https://www.dtfcraft.fr), contact@stephane-glacet.com.

DTF Craft est une version dérivée de [PhotoCraft](https://github.com/storytold/photocraft)
(licence MIT ou Apache-2.0, voir `LICENSE-MIT`, `LICENSE-APACHE` et `NOTICE`). Ce n'est pas un
produit officiel du projet PhotoCraft.

## Menu DTF

| Commande | Rôle |
|---|---|
| Conformité… | Calque « origin », hauteur d'impression (cm) à la résolution voulue, 8/16 bits, rognage de la transparence |
| Détourage… | Copie du calque « Sans fond » au-dessus de la source (masquée, intacte) ; le fond uni (couleur lue sur les bords, ou choisie) est supprimé depuis les bords, le blanc intérieur au visuel reste. Tolérance (bruit JPEG), contour décontaminé (pas de liseré clair), trous intérieurs en option (désactivés par défaut, taille minimale en px). Aperçu sur fond #231c1c |
| Fond de contrôle… | Calque de remplissage « backfill » sous l'image, couleur du textile, avec aperçu ; remplace un fond de contrôle déjà présent |
| Suppression fond de contrôle | Supprime les calques « backfill » |
| Pixels transparents… | Supprime les pixels semi-transparents (seuil d'opacité, 62 % par défaut) |
| Supprimer franges… | Supprime les franges de couleur sur les bords |
| Netteté… | Netteté optimisée avec aperçu |
| Saturation… | Saturation (+15 par défaut) avec aperçu |
| Aplat couleur… | Nettoie le liseré sombre de 1 à 3 px qu'un Agrandissement IA laisse sur les bords des formes en aplat, sans toucher aux contours voulus. Fenêtre propre avec aperçu en direct (pleine résolution) et barre de progression ; Appliquer agit sur le calque actif (Édition › Annuler revient en arrière). Les couleurs sont regroupées en une palette (k-means en L*a*b*), découpée en formes ; chaque forme reprend sa couleur dominante (médiane de ses pixels à plus de 4 px du bord, sinon sa couleur de palette) ; seuls les pixels de la bande de bord plus sombres que la forme et de sa teinte sont corrigés ; un pixel ambigu prend la couleur de la majorité de ses voisins ; un liseré assez épais pour former sa propre couleur est comparé à la forme voisine. Réglages : nombre de couleurs auto (case) ou 2 à 32, largeur de bande 2 px (1 à 5), seuil d'assombrissement ΔL* 8 (0 à 40), tolérance de teinte ΔE 15 (0 à 50), bords nets (défaut) ou anticrénelage 1 px, couleurs protégées prises à la pipette sur l'image (jamais remplacées ; clic sur une pastille pour la retirer, Effacer), « Afficher l'avant », « Surligner les modifications » (pixels corrigés en magenta dans l'aperçu seulement). La transparence ne change pas. Aide « ? » et infobulles |
| Plage de couleurs… | L'outil Plage de couleurs complet de PhotoCraft, dans sa propre fenêtre, sans verrou, remis à zéro à chaque ouverture : Sélection (couleurs échantillonnées, familles de teintes, tons clairs / moyens / foncés, tons chair, couleurs non imprimables), clusters de couleurs localisés et étendue, tolérance, plages de tons, pipettes échantillon / ajouter / soustraire sur l'image (Maj+clic, Alt+clic), Inverser, vignette Sélection / Image (blanc = effacé). OK efface la plage sur une copie « Sans fond » du calque, progressivement selon la tolérance ; l'original est masqué et intact ; une seule étape d'historique. Sans échantillon, OK est refusé et rien n'est effacé. Une sélection active limite la zone traitée et reste en place. À utiliser avant le Halftone DTF |
| Halftone DTF… | Deux modes : pixels semi-transparents seulement, ou visuel entier (textile foncé/clair). Crée un dossier « Halftone DTF » au-dessus du calque source (masqué, intact) : calque « Halftone » tramé, sur un « Fond de contrôle » #231c1c (textile foncé) ou blanc (textile clair) ; sans fond pour les semi-transparents. L'aperçu montre ce résultat, avec une barre de progression sous les réglages pendant son calcul. La trame est calculée par bandes sur la copie : peu de mémoire de travail, même sur un gros fichier. Réglages toujours modifiables (pas de verrou) |
| Gang sheet (bêta)… | Module en bêta. Fenêtre plein écran claire, aux couleurs du logo DTF Craft : import, réglages, plusieurs planches en onglets, construction auto, contrôle de la résolution, des chevauchements et des fonds, export PNG 300 dpi ouvert comme nouveau document (voir ci-dessous) |
| Mockup (bêta)… | Module en bêta. Les visuels du client sur la photo du vêtement, à leur taille d'impression réelle : photo PNG/JPG, échelle par un repère tracé sur la photo et sa longueur en cm, visuels PNG/SVG ou document ouvert, ombres et plis du tissu, export PNG/JPG ouvert comme nouveau document (voir ci-dessous) |
| Vectorisation… | Module plein écran (comme Camera Raw) : le visuel est réduit à une palette (8 couleurs par défaut, 2 à 16) puis nettoyé (les liserés trop fins d'une couleur intermédiaire entre deux autres, typiques des bords flous ou JPEG, reviennent à la plus proche des deux ; les contours sont lissés à la couleur majoritaire du voisinage, sans dents ni marches, à une échelle fixée par le niveau de détail) et tracé en formes vectorielles découpées, une couleur unie par forme, sans superposition. Réglages : type de visuel (logo · aplats / illustration · nuances), niveau de détail, lissage (courbes / angles vifs), suppression du fond lu au bord du visuel, joints fermés entre couleurs (chaque forme déborde d'un demi-pixel sur ses voisines ; à décocher pour un traceur de découpe). Aperçu original / vectorisé / côte à côte, contours, zoom jusqu'à 800 % (Ctrl+molette, boutons − / +, double-clic ; glisser ou molette pour se déplacer ; les deux images suivent en côte à côte ; la partie visible du vectorisé est redessinée nette à la résolution de l'écran), palette où décocher une couleur la retire du SVG. Nuancier de référence partagé avec l'Analyseur couleur : référence la plus proche et ΔE 2000 par couleur détectée, « Changer… », case ↔ (auto jusqu'à ΔE 5) qui remplace la couleur par la référence dans l'aperçu et le SVG, formes nommées d'après la référence (id SVG, calques à l'ouverture). Fiche PDF A4 : original et vectorisé côte à côte, couleurs du SVG et leurs références. Deux moteurs au choix : « DTF Craft » (ci-dessus) et « VectorCraft » (Image Trace de VectorCraft, copié sans modification) avec ses 15 préréglages et tous ses réglages : mode (noir et blanc avec seuil, niveaux de gris, couleur, logo aplats), palette (limitée, tons continus, automatique, ou le nuancier de référence), couleurs, détail couleur, tracés, angles, bruit, méthode (juxtaposée / superposée), remplissages et traits en lignes centrales avec leur épaisseur, ignorer une couleur, accrocher les courbes aux lignes, formes exactes, dégradés linéaires ; la table des couleurs, le nuancier, la fiche PDF et l'export sont les mêmes avec les deux moteurs. Export SVG à la taille d'impression du document, ouvert en option dans un nouvel onglet en calques de forme ; le document n'est pas modifié. Traceurs : visioncortex (MIT OR Apache-2.0) et VectorCraft Image Trace (ArtCraft Team et contributeurs VectorCraft, MIT OR Apache-2.0) |

Toutes les commandes sont annulables (Édition › Annuler) et utilisables dans les actions.

## Fenêtres DTF

Chaque outil DTF s'ouvre depuis le menu DTF dans sa fenêtre, comme Halftone DTF (Conformité,
Détourage, Fond de contrôle, Pixels transparents, Supprimer franges, Netteté, Saturation : le
menu porte le titre de la fenêtre) ; une fenêtre à la fois. Chaque fenêtre a un bouton « ? »
qui ouvre la fiche d'aide de l'outil (à quoi il sert, quand l'utiliser, réglages conseillés) ;
chaque réglage a une infobulle au survol. « Appliquer » applique et ferme ; « Annuler », la croix
ou Échap ferment sans appliquer. L'aperçu s'affiche sur l'image dès qu'un réglage bouge (sauf
Conformité), tant que la fenêtre est ouverte. La fenêtre Fond de contrôle a le bouton
« Afficher le fond » (remplace un fond déjà présent) ; DTF › Suppression fond de contrôle le retire.

Sauf Conformité, chaque fenêtre s'ouvre réglages verrouillés (curseurs grisés) : le cadenas les
déverrouille ou les reverrouille, « Réinitialiser » (fenêtre déverrouillée) remet les valeurs
d'usine de l'outil, enregistrées seulement par « Appliquer ». « ? », « Afficher l'avant »,
« Appliquer », « Annuler », « Afficher le fond » et, dans Détourage, « Ajouter des trous » et
« Supprimer le dernier trou » restent utilisables verrouillés ; la pipette du fond, non.

Les fenêtres sont modales (l'image et les autres menus attendent), sauf Détourage :
l'image reste cliquable pour ajouter des trous et prendre la couleur du fond à la pipette.

Espace de travail DTF (Fenêtre › Espace de travail › DTF, ouvert au démarrage) : Calques seul
dans la colonne de droite. Canaux et Tracés n'y sont pas (menu Fenêtre). L'accueil donne le lien du
projet GitHub de DTF Craft.

Conformité impression : la fenêtre part de la taille actuelle du visuel (rogné si « Rogner la
transparence » est coché) et de la résolution du document ; changer la largeur met à jour la
hauteur, et inversement ; changer la résolution recalcule la taille en cm sans rééchantillonner les
pixels. Les boutons ×2, ×3, ×4 lancent l'Agrandissement IA à la résolution de la fenêtre et
indiquent la taille obtenue (cm et px). La transparence est rognée avant le redimensionnement, en laissant
2 px transparents autour du visuel. Le calque est renommé « origin » (champ Nom du calque).

« Optimiser pour la vectorisation » (sous les boutons ×2, ×3, ×4) prépare le visuel et ouvre
directement le module Vectorisation, sans copie et sans modifier le document : pixels visibles
posés sur fond blanc ; côté long sous 1 500 px : agrandissement IA (×2, ×3 ou ×4, le plus proche de
2 500 px, la taille d'impression conservée ; le module travaille de toute façon sur 2 500 px au
plus) ; source JPEG : filtre médian de 1 px (bords répétés). La préparation tourne à côté de la
fenêtre avec une barre de progression (Annuler), puis le module s'ouvre ; son export trace les
mêmes pixels (PNG temporaire, paramètre `image` de `dtf.vectorize`). Au-delà de 64 couleurs
dominantes, une notice avertit que le visuel convient mal à la vectorisation (photo, dégradé). Le
fond blanc est à retirer dans le module. Commande `dtf.optimizeForVectorization` (`path` : PNG
préparé).

Module Vectorisation : l'analyse des couleurs et le tracé tournent à côté de la fenêtre, avec une
barre de progression sur l'aperçu (part des couleurs tracées) ; les exports attendent la fin.

Retouches (Netteté, Saturation, Pixels transparents, Supprimer franges) : l'aperçu s'affiche dès
l'ouverture de la fenêtre ; « Afficher l'avant » montre l'image d'origine pour comparer.

Détourage : l'aperçu s'affiche dès l'ouverture de la fenêtre, sur la dernière
couleur utilisée (Fond de contrôle ou « Fond d'aperçu »). La couleur du fond est détectée sur les
bords si elle est unie, ou prise à la pipette. « Ajouter des trous » et « Couleur du fond à la
pipette » sont deux boutons à bascule exclusifs ; sans bouton enfoncé, un clic sur l'image ne fait
rien. Chaque trou cliqué s'ajoute à l'aperçu et est percé avec le fond à l'application ;
Ctrl+Alt+Z annule puis rétablit le dernier changement avant « Appliquer ».

## Gang sheet

Module en **bêta** : « (bêta) » après son entrée de menu, pastille BÊTA dans son en-tête, fenêtre
d'avertissement à l'ouverture (voir « Modules bêta et signalement des bugs »).

DTF › Gang sheet (bêta)… ouvre une fenêtre plein écran, toujours claire (quel que soit le thème), aux
couleurs du logo DTF Craft : panneaux blancs et gris mauve, magenta pour Exporter, la sélection,
les cases et la fin de planche, bleu pour Importer et Construction auto. Échap ou Annuler la
referme.

- **En-tête** : logo, « Gang sheet », pastille BÊTA, un onglet par planche (nom et nombre d'exemplaires à
  imprimer, par exemple « Gang sheet 1 · ×1 » ; clic : afficher la planche ; double-clic : la
  renommer ; croix : fermer la planche, ses visuels restant dans la liste, sauf quand il n'en reste
  qu'une) et « + » (nouvelle planche), puis Signaler un bug, Annuler et Exporter le PNG… (ou
  Exporter les planches quand il y en a plusieurs).
- **Colonne de gauche** : zone d'import (« Importer des images », PNG ou SVG, ou fichiers glissés
  sur la fenêtre ; « Ajouter le document ouvert », aplati), puis **Construction auto**, puis la
  liste des visuels : une ligne par visuel avec vignette, nom, taille d'impression et résolution,
  pastille de résolution et quantité (− champ +). Chaque visuel est rogné à ses pixels visibles.
  Clic : afficher ses réglages (ligne encadrée en magenta) ; double-clic : un exemplaire de plus ;
  × (au survol) : retirer le visuel. S'il était sur des planches, tous les visuels restants sont
  replacés ensemble (exemplaires et groupes conservés) sur le moins de planches possible : les
  planches qui le portaient disparaissent. Ctrl+Z annule.
  En bas de la colonne, la section repliable **Visuel sélectionné** : les réglages du dernier
  visuel sélectionné, importé ou modifié (ils restent affichés tant qu'il y a des visuels, même
  quand les planches sont reconstruites) : largeur et hauteur en cm, proportions verrouillables,
  résolution, avertissements, quantité d'images (en mode cases : la quantité totale, toutes
  planches confondues ; les planches sont reconstruites et générées autant qu'il en faut, 124 logos
  de 10 cm donnent 36 + 36 + 36 + 16 ; sans cases : la quantité sur la planche, l'excédent part sur
  de nouvelles planches), marge propre (interrupteur), Remplissage automatique (remplit la place
  restante), Dupliquer l'image. En mode cases, un exemplaire supprimé ou dupliqué à la main compte
  dans la quantité à la réorganisation suivante. Plusieurs visuels sélectionnés (ou un groupe) :
  leur nombre, la taille de leur cadre, Grouper, Dégrouper, Dupliquer, Supprimer.
- **Palette d'outils** (flottante, à gauche de la planche, deux boutons par rangée) : annuler /
  rétablir, rotation 90° dans les deux sens, retournement horizontal et vertical, étirement à la
  laize ou à la longueur pour un visuel seul, centrage horizontal, vertical ou sur la planche,
  pousser à gauche, à droite, en haut, en bas, Aligner (bords gauches, centres horizontaux, bords
  droits, bords hauts, centres verticaux, bords bas, sur le cadre de la sélection ; 2 blocs au
  moins), Répartir (horizontalement ou verticalement, à espacement égal, le premier et le dernier
  restant en place ; 3 blocs au moins), grouper, dégrouper, supprimer. Un groupe compte comme un
  seul bloc ; un alignement ou une répartition qui rapprocherait deux visuels à moins de la marge
  est refusé (rien ne bouge).
- **Colonne de droite**, en sections repliables :
  - **Planche** (ouverte) : nom (crayon pour renommer), taille réelle (par exemple « 60 × 62 cm
    (max 100) ») et nombre d'images, exemplaires à imprimer, Dupliquer, Supprimer la planche,
    Recommencer.
  - **Film** (replié) : laize (30, 40, 42, 56, 57 ou 60 cm), subdivision (sans, ou 2, 3, 4, 5, 6, 8
    colonnes, avec la taille des cases, par exemple « 4 × 10 cm »), organisation automatique à
    l'import, aimantation aux cases, marge du plan de travail (le long des bords, 1 cm par défaut),
    marge de l'image (entre les visuels, 1 cm par défaut, sans objet avec les cases), largeur
    imprimable (laize moins les deux marges), laize personnalisée, longueur max (100 cm au plus),
    rotation autorisée pour la construction auto. Réglages de ce gang sheet seulement.
  - **Affichage** (replié) : lignes de résolution, lignes en chevauchement, alignement automatique
    pendant le déplacement, avertissement sur l'arrière-plan (coins opaques : fond non détouré),
    avertissement de transparence (plus de 0,5 % de pixels semi-transparents), grille 1 cm, fond de
    la planche (damier, gris, noir, blanc, couleur au choix). Réglages de ce gang sheet seulement.
  - **Préférences** (replié) : les valeurs de départ de chaque nouveau gang sheet, gardées d'une
    session à l'autre avec les préférences de DTF Craft : tous les réglages Film et Affichage, plus
    le nombre de copies d'un visuel importé. Valeurs d'usine : laize 60 cm, sans subdivision, marges
    de 1 cm, longueur max 100 cm, 1 copie. Modifier une préférence ne change pas le gang sheet en
    cours ; « Valeurs d'usine » les remet.
- **Barre d'état** (en bas) : légende des résolutions (≥ 300 ppp, ≥ 250, ≥ 200, < 200) et des
  chevauchements ; pour le visuel survolé ou sélectionné : position gauche / haut, taille,
  résolution et avertissements (ou le nombre et le cadre des visuels sélectionnés) ; le dernier
  message ; à droite, la longueur utilisée et le total des planches et des visuels.
- **Champs numériques** : ils se saisissent au clavier ; glisser la souris dessus ne change rien.
- **Cases** : la largeur imprimable est découpée en colonnes de cases carrées (40 cm en 4
  colonnes : cases de 10 × 10 cm, 9 rangées entières dans les 98 cm entre les marges d'une planche
  de 100 cm). Chaque visuel occupe les cases entières qu'il lui faut (0,5 mm de tolérance) et y est
  centré en largeur et en hauteur ; les cases se touchent, sans espacement. Chaque import replace
  tous les visuels (les plus grands d'abord, rotation si elle raccourcit la planche) et ouvre une
  nouvelle planche quand une est pleine. Un visuel déplacé à la main se pose dans les cases les
  plus proches. Les cases sont tracées en pointillés magenta (non imprimés).
- **SVG** : chaque fichier SVG déposé ou importé ouvre une fenêtre « Taille d'impression du SVG »
  (aperçu, largeur et hauteur liées, boutons « Largeur en cases » 1, 2, 3, 4…), une fenêtre par
  fichier ; Placer ou Passer. Taille proposée : celle du fichier quand elle est en cm, mm, in, pt
  ou pc, sinon une case (10 cm). Le dessin est rogné à sa partie visible et rendu directement à
  sa taille d'impression à l'export (300 ppp), même agrandi après coup : jamais pixellisé. Les
  textes utilisent les polices installées ; une image incluse garde sa propre résolution.
- **Planche** : la longueur utile (bas du dernier visuel plus la marge) est marquée en magenta ;
  au delà, jusqu'à la longueur max, la zone est hachurée (« Réserve jusqu'à 100 cm · non
  exportée »). Règles en cm. Contours : vert ≥ 300 ppp, jaune ≥ 250, rouge ≥ 200, noir en dessous ;
  bleu pour un visuel en chevauchement ou trop proche ; la sélection est encadrée en magenta. Molette : défilement ;
  Ctrl + molette : zoom ; boutons de zoom en bas à droite.
- **Sélection** : clic sur un visuel ; Maj+clic (ou Ctrl+clic) ajoute ou retire un visuel ; un
  rectangle tracé depuis une zone vide du film prend tous les visuels qu'il touche (Maj : ajoutés à
  la sélection) ; Ctrl+A : tous ; Échap : plus rien.
- **Groupes** : Ctrl+G groupe la sélection, Ctrl+Maj+G dégroupe (aussi dans la palette d'outils,
  le clic droit et la section Visuel sélectionné). Un clic sur un visuel du groupe prend tout le groupe ; un
  groupe est entouré d'un cadre pointillé et garde les écarts entre ses visuels. Annuler revient
  sur un groupage.
- **Déplacement** : glisser au millimètre ; la sélection se déplace d'un bloc et reste sur le
  film, marge du plan de travail comprise. Lâchée à moins de la marge d'un autre visuel (la plus
  grande des deux marges), elle revient à sa place. Rotation : le bloc tourne autour de son centre,
  chaque visuel tourne avec lui ; retournement : le bloc est retourné. Flèches : 1 mm (Maj : 1 cm) ;
  Suppr, Ctrl+C / Ctrl+X / Ctrl+V, Ctrl+D, Ctrl+Z / Ctrl+Y ; clic droit : copier, coller,
  supprimer, dupliquer, grouper, dégrouper, tout sélectionner, remplissage automatique, rotation.
- **Construction auto** : tous les visuels avec largeur, hauteur, verrou des proportions,
  résolution et nombre d'exemplaires (dupliquer : même image, autre taille) ; marges et rotation ;
  Appliquer replace tout, les plus grands d'abord, et ouvre une nouvelle planche quand une est
  pleine. Chaque groupe est conservé tel quel, en un seul bloc placé en premier ; ses visuels
  comptent dans le nombre d'exemplaires de leur image.
- **Export** : chaque planche avec des visuels est rendue à 300 ppp (laize × longueur utilisée,
  un calque par visuel), enregistrée en PNG (résolution inscrite) et ouverte comme nouveau
  document. Plusieurs planches : « nom 1.png », « nom 2.png »… ; une quantité supérieure à 1 est
  ajoutée au nom (« x3 »). Les visuels trop proches ou hors du film sont signalés avant l'export.

La commande `dtf.gangSheet` fait le même travail sans fenêtre (actions, ligne de commande) : avec
`sources` seulement, elle construit la planche elle-même ; avec `placements` (position, rotation,
retournements), elle reprend la mise en place donnée. Marges : `edgeMargin` (bords) et `spacing`
(entre visuels), en mm ; `marginMm` par visuel. `columns` (construction par la commande) range
les visuels dans les cases, sans espacement ; laize par défaut 42 cm. Un `path` SVG est rendu à
sa taille d'impression.

## Mockup

Module en **bêta** (pastille BÊTA, fenêtre d'avertissement, Signaler un bug). DTF › Mockup (bêta)…
ouvre une fenêtre plein écran claire, comme le Gang sheet, pour montrer au client ses visuels sur
le vêtement, à leur vraie taille.

- **Colonne de gauche**, en trois étapes numérotées :
  1. **Photo du produit** : un PNG ou un JPG du vêtement, de face, à plat ou porté, ou une
     copie d'écran collée (« Coller une copie d'écran » ou Ctrl+V ; gardée en PNG dans le
     dossier temporaire pour l'export).
  2. **Échelle** : « Tracer un repère sur la photo » (glisser le long d'une longueur connue, la
     largeur de poitrine par exemple ; les deux extrémités restent déplaçables), puis la
     longueur réelle du repère en cm. La fenêtre en déduit les pixels par cm de la photo et
     affiche des règles en cm le long de la photo. Le tracé est demandé dès qu'une photo est
     chargée.
  3. **Zones du produit** (facultatif) : les vues du vêtement sur la photo (face, dos, côtés),
     une zone par vue, chacune avec son nom et sa couleur. « Sélectionner le produit (auto) »
     retire le fond uni depuis les bords de la photo (sur un fond chargé : Sélectionner le sujet
     de PhotoCraft) et crée une zone par objet trouvé, dans l'ordre de lecture. « Lasso
     polygonal » : un clic par point, double-clic ou premier point pour fermer, Échap annule,
     Retour arrière retire le dernier point ; mode Nouvelle, Ajouter ou Soustraire (à la zone
     choisie). « Reprendre la sélection du document ouvert » (document de la taille de la
     photo) : une zone par objet. Intervertir, Effacer, renommer, retirer. Chaque visuel est
     masqué par la zone sous son centre (recalculée quand il bouge) ; « Zone » dans « Visuel
     sélectionné » la force ou choisit « Non masqué ». Changer de photo efface les zones.
  4. **Visuels** : PNG ou SVG importés, ou le document ouvert (aplati, rogné à ses pixels
     visibles). Bloqués tant que l'échelle n'est pas donnée (« Indiquez d'abord l'échelle »).
     Chaque visuel arrive à sa taille d'impression (ses pixels à sa résolution, celle que
     l'outil Conformité impose à 300 ppp), au milieu du repère ; plusieurs visuels possibles.
- **Photo** : le rendu tel qu'imprimé ; clic pour choisir un visuel, glisser pour le déplacer
  (flèches : 1 mm, Maj : 1 cm ; Suppr le retire) ; sa taille en cm est écrite dessous.
- **Colonne de droite** : **Visuel sélectionné** (largeur et hauteur en cm, proportions
  conservées ou non, rotation, centrer horizontalement sur le milieu du repère, supprimer ;
  un avertissement quand la taille diffère de celle du fichier : elle ne vaut que pour le
  mockup), **Rendu** (ombres du tissu 60 %, plis 35 %, opacité de l'encre 95 %) et **Export**
  (PNG ou JPG, replié).
- **Transformation** (les outils d'Édition › Transformation de PhotoCraft) : palette sur la photo,
  clic droit sur le visuel ou bloc « Transformation » à droite. Modification en direct ; Entrée
  ou Échap revient au simple déplacement ; « Réinitialiser la transformation » rend au visuel sa
  taille d'impression, droit et sans déformation.
  - Mise à l'échelle : 8 poignées (Maj inverse « Conserver les proportions », Alt depuis le centre).
  - Rotation : glisser autour du visuel (Maj : pas de 15°).
  - Inclinaison : les côtés glissent le long d'eux-mêmes.
  - Déformation : chaque coin librement.
  - Perspective : un coin entraîne son symétrique.
  - Déformer : grille 4 × 4 de points de Bézier modifiables, ou un style (Arc, Arche,
    Renflement, Drapeau, Gonflement) avec sa courbure, horizontal ou vertical.
  Changer l'échelle (le repère) garde la taille en cm des visuels.
- **Barre d'état** : échelle, taille de la photo en cm, nombre de visuels, dernier message.

Rendu : les plis déplacent l'encre selon la pente de la lumière floutée de la photo (jusqu'à
0,4 cm à 100 %) ; les ombres reportent la lumière de la photo sur l'encre (plus sombre que la
médiane de la zone : l'encre fonce d'autant ; plus clair : elle s'éclaircit peu) ; l'opacité
laisse voir le tissu. L'aperçu est calculé sur la photo réduite à 1 400 px, l'export à la
résolution de la photo, avec le même calcul. Les fichiers des visuels ne sont jamais modifiés.
En bêta, pas d'enregistrement de projet : l'export est le résultat.

La commande `dtf.mockup` fait le même travail sans fenêtre : `photo`, `scale` (`from`, `to` en
pixels de la photo, `cm`) ou `pixelsPerCm`, `visuals` (`path` ou `document`, `x`, `y` du centre,
`widthCm`, `heightCm`, `rotation`, ou `corners` : ses 4 coins sur la photo, et `warp` : `style`,
`bend`, `vertical`, `points` pour une grille personnalisée, et `zone` : l'indice de sa zone),
`zones` (masques PNG de la taille de la photo, blanc dedans), `shadows`, `folds`, `opacity`,
`output` (.png ou .jpg), `open`.

## Modules bêta et signalement des bugs

Un module en bêta (aujourd'hui le Gang sheet et le Mockup) porte « (bêta) » après son entrée de menu et une
pastille BÊTA dans sa fenêtre. À sa première ouverture dans une version, une fenêtre « Module en
version bêta » explique qu'il est en cours de développement, peut comporter des bugs et évoluer
d'une version à l'autre (vérifier ses fichiers avant impression) ; boutons Signaler un bug et
Continuer, case « Ne plus afficher pour cette version » (gardée dans les préférences, la fenêtre
revient à la version suivante).

**Signaler un bug** (fenêtre bêta, en-tête du module bêta, Aide › Signaler un bug…) ouvre un
rapport à compléter dans la fenêtre : ce que j'ai fait, ce qui s'est passé, ce que j'attendais,
puis la version de DTF Craft (et son commit), le système, le module, le document (taille en px,
résolution) et le dernier message affiché. **Copier le rapport** le met dans le presse-papiers
avec son objet, à coller dans un e-mail à contact@stephane-glacet.com (capture d'écran à joindre
soi-même) ; **Ouvrir dans la messagerie** l'ouvre dans le logiciel de messagerie, s'il y en a un.
Rien n'est envoyé sans l'utilisateur.

## Agrandissement IA

Lancé depuis la fenêtre Conformité impression (boutons ×2, ×3, ×4) ; il n'est plus dans le menu DTF.

Le modèle `realesr-general-x4v3` de Real-ESRGAN (© 2021 Xintao Wang, licence BSD 3-Clause, voir
`LICENSE-Real-ESRGAN.txt`) est intégré à l'application. Le calcul se fait sur le processeur,
sans connexion internet : compter environ une à deux minutes pour une image de 1000 × 1000 px
sur un processeur 8 cœurs.

## Accueil

Page d'accueil sobre : le bloc DTF Craft (version, Nouveau document / Ouvrir, dépôt de fichier),
puis en liens texte les sites de Stéphane Glacet (dtfcraft.fr, pixel-net.fr, reparation-dtf.fr,
formation-dtf.fr, chromilab.fr, stephane-glacet.com, dev-ia.fr) et, plus petits, GitHub et le site
ArtCraft / PhotoCraft. Ni fichiers récents (menu Fichier › Ouvrir récent), ni encarts : la
formation (75 €) et le devis (régie 375 € HT / jour) restent dans les menus DTF et Aide et les
fiches d'aide.

En fond, une illustration Pixabay (recherche « pop art », illustrations, format paysage, tout
public), une nouvelle à chaque ouverture, sous un voile qui garde l'accueil lisible ; la source est
affichée en bas à droite (« Illustration : auteur sur Pixabay », lien vers sa page), avec « Masquer
l'image » / « Afficher une illustration ». La recherche est gardée 24 heures (règle de Pixabay) et
l'image est téléchargée. La clé API vient du secret GitHub `PIXABAY_KEY` à la compilation (ou de la
préférence `dtfCraft.homeImage.key`) ; sans clé ni connexion, l'accueil garde son fond habituel.

## Réglages

DTF Craft garde ses réglages à part de PhotoCraft : `%APPDATA%\DTF Craft` sous Windows.

## Compilation

GitHub Actions compile la version Windows (`.github/workflows/dtf-craft-windows.yml`) à chaque
envoi sur la branche `main` ; le zip portable est dans les artefacts de l'exécution (conservés
un jour). Aucun binaire n'est joint aux releases publiques ; sur une étiquette ou un lancement manuel
(`upload_name`), le zip est envoyé sur l'hébergement Nuxit quand les secrets `NUXIT_FTP_*` sont définis.

## Mise à niveau de PhotoCraft

L'historique Git de DTF Craft n'est pas relié à celui de PhotoCraft (import en un commit).
`PHOTOCRAFT-BASE.toml` enregistre le commit PhotoCraft de départ ; `scripts/dtf-sync-photocraft.sh`
s'en sert comme ancêtre commun pour fusionner une nouvelle version :

```sh
scripts/dtf-sync-photocraft.sh start v0.23.1   # branche sync/… (à ne pas pousser), conflits listés
scripts/dtf-sync-photocraft.sh finish          # branche maj/photocraft-v0.23.1, un seul commit
scripts/dtf-sync-photocraft.sh abort           # abandon
```

Les workflows GitHub de PhotoCraft restent retirés à chaque fusion. Pour limiter les conflits, le
code DTF reste dans ses propres fichiers (liste dans `MODIFICATIONS.md`) ; les traductions DTF sont
dans `crates/ui-egui/src/i18n/dtf/<langue>.tsv`, ajoutées au catalogue PhotoCraft à la compilation.

Depuis PhotoCraft 0.20, les traductions de PhotoCraft viennent du dépôt craft-i18n, à la révision
de `craft-i18n.rev` : la compilation lit `CRAFT_I18N_DIR` (une copie de craft-i18n). Sans elle,
l'interface est en anglais. Le workflow Windows la récupère avant de compiler.

Depuis PhotoCraft 0.20, les traductions de PhotoCraft viennent du dépôt craft-i18n, à la révision
de `craft-i18n.rev` : la compilation lit `CRAFT_I18N_DIR` (une copie de craft-i18n). Sans elle,
l'interface est en anglais. Le workflow Windows la récupère avant de compiler.
