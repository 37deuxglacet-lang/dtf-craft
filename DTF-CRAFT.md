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
| Halftone DTF… | Deux onglets. Automatique : cinq préréglages repris des actions Photoshop de Stéphane Glacet — T-shirt noir (négatif, Niveaux 50–240 γ 0,5 deux fois, 25 lpi, 22°, rond, fond noir), T-shirt noir doux (Niveaux une fois), T-shirt blanc (sans négatif, Niveaux deux fois, fond blanc), Noir et blanc (55 lpi, points blancs, fond noir), Uniforme (trame régulière à 92 %, sans fond) ; option « Calques de retouche » : groupe masqué « RasterFx » (Luminosité/Contraste +32, Teinte/Saturation +16) au-dessus du calque tramé. Manuel (réglages verrouillés à l'ouverture, cadenas et Réinitialiser) : pixels semi-transparents seulement, ou visuel entier (textile foncé/clair). Crée un dossier « Halftone DTF » au-dessus du calque source (masqué, intact) : calque « Halftone » tramé, sur un « Fond de contrôle » #231c1c (textile foncé) ou blanc (textile clair) ; sans fond pour les semi-transparents. L'aperçu montre ce résultat |
| Gang sheet… | Fenêtre plein écran claire, aux couleurs du logo DTF Craft : import, réglages, plusieurs planches en onglets, construction auto, contrôle de la résolution, des chevauchements et des fonds, export PNG 300 dpi ouvert comme nouveau document (voir ci-dessous) |
| Vectorisation… | Module plein écran (comme Camera Raw) : le visuel est réduit à une palette (8 couleurs par défaut, 2 à 16) puis nettoyé (les liserés trop fins d'une couleur intermédiaire entre deux autres, typiques des bords flous ou JPEG, reviennent à la plus proche des deux ; les contours sont lissés à la couleur majoritaire du voisinage, sans dents ni marches, à une échelle fixée par le niveau de détail) et tracé en formes vectorielles découpées, une couleur unie par forme, sans superposition. Réglages : type de visuel (logo · aplats / illustration · nuances), niveau de détail, lissage (courbes / angles vifs), suppression du fond lu au bord du visuel, joints fermés entre couleurs (chaque forme déborde d'un demi-pixel sur ses voisines ; à décocher pour un traceur de découpe). Aperçu original / vectorisé / côte à côte, contours, zoom jusqu'à 800 % (Ctrl+molette, boutons − / +, double-clic ; glisser ou molette pour se déplacer ; les deux images suivent en côte à côte ; la partie visible du vectorisé est redessinée nette à la résolution de l'écran), palette où décocher une couleur la retire du SVG. Nuancier de référence partagé avec l'Analyseur couleur : référence la plus proche et ΔE 2000 par couleur détectée, « Changer… », case ⇄ (auto jusqu'à ΔE 5) qui remplace la couleur par la référence dans l'aperçu et le SVG, formes nommées d'après la référence (id SVG, calques à l'ouverture). Fiche PDF A4 : original et vectorisé côte à côte, couleurs du SVG et leurs références. Deux moteurs au choix : « DTF Craft » (ci-dessus) et « VectorCraft » (Image Trace de VectorCraft, copié sans modification) avec ses 15 préréglages et tous ses réglages : mode (noir et blanc avec seuil, niveaux de gris, couleur, logo aplats), palette (limitée, tons continus, automatique, ou le nuancier de référence), couleurs, détail couleur, tracés, angles, bruit, méthode (juxtaposée / superposée), remplissages et traits en lignes centrales avec leur épaisseur, ignorer une couleur, accrocher les courbes aux lignes, formes exactes, dégradés linéaires ; la table des couleurs, le nuancier, la fiche PDF et l'export sont les mêmes avec les deux moteurs. Export SVG à la taille d'impression du document, ouvert en option dans un nouvel onglet en calques de forme ; le document n'est pas modifié. Traceurs : visioncortex (MIT OR Apache-2.0) et VectorCraft Image Trace (ArtCraft Team et contributeurs VectorCraft, MIT OR Apache-2.0) |

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

DTF › Gang sheet… ouvre une fenêtre plein écran, toujours claire (quel que soit le thème), aux
couleurs du logo DTF Craft : panneaux blancs et gris mauve, magenta pour Exporter, la sélection,
les cases et la fin de planche, bleu pour Importer et Construction auto. Échap ou Annuler la
referme.

- **En-tête** : logo, « Gang sheet », un onglet par planche (nom et nombre d'exemplaires à
  imprimer, par exemple « Gang sheet 1 · ×1 » ; clic : afficher la planche ; double-clic : la
  renommer) et « + » (nouvelle planche), puis Annuler et Exporter le PNG… (ou Exporter les
  planches quand il y en a plusieurs).
- **Colonne de gauche** : zone d'import (« Importer des images », PNG ou SVG, ou fichiers glissés
  sur la fenêtre ; « Ajouter le document ouvert », aplati), puis **Construction auto**, puis la
  liste des visuels : une ligne par visuel avec vignette, nom, taille d'impression et résolution,
  pastille de résolution et quantité (− champ +). Chaque visuel est rogné à ses pixels visibles.
  Clic : afficher ses réglages (ligne encadrée en magenta) ; double-clic : un exemplaire de plus ;
  × (au survol) : retirer le visuel.
- **Palette d'outils** (flottante, à gauche de la planche) : annuler / rétablir, rotation 90° dans
  les deux sens, retournement horizontal et vertical, étirement à la laize ou à la longueur pour un
  visuel seul, centrage horizontal, vertical ou sur la planche, pousser à gauche, à droite, en haut,
  en bas, grouper, dégrouper, supprimer.
- **Colonne de droite**, en sections repliables :
  - **Planche** : nom (crayon pour renommer), taille réelle (par exemple « 42 × 62 cm (max 100) »)
    et nombre d'images, exemplaires à imprimer, Dupliquer, Supprimer la planche, Recommencer.
  - **Film** : laize (30, 40, 42, 56, 57 ou 60 cm), subdivision (sans, ou 2, 3, 4, 5, 6, 8 colonnes,
    avec la taille des cases, par exemple « 4 × 10 cm »), organisation automatique à l'import,
    aimantation aux cases, marge du plan de travail (le long des bords, 1 cm par défaut), marge de
    l'image (entre les visuels, 1 cm par défaut, sans objet avec les cases), largeur imprimable
    (laize moins les deux marges : 40 cm), laize personnalisée, longueur max (100 cm au plus),
    rotation autorisée pour la construction auto.
  - **Visuel sélectionné** : les réglages du dernier visuel sélectionné, importé ou modifié
    (ils restent affichés tant qu'il y a des visuels, même quand les planches sont reconstruites) :
    largeur et hauteur en cm, proportions verrouillables, résolution, avertissements, quantité
    d'images (en mode cases : la quantité totale, toutes planches confondues ; les planches sont
    reconstruites et générées autant qu'il en faut, 124 logos de 10 cm donnent 36 + 36 + 36 + 16 ;
    sans cases : la quantité sur la planche, l'excédent part sur de nouvelles planches), marge propre
    (interrupteur), Remplissage automatique (remplit la place restante), Dupliquer l'image. En mode
    cases, un exemplaire supprimé ou dupliqué à la main compte dans la quantité à la réorganisation
    suivante. Plusieurs visuels sélectionnés (ou un groupe) : leur nombre, la taille de leur cadre,
    Aligner (bords gauches, centres horizontaux, bords droits, bords hauts, centres verticaux, bords
    bas, sur le cadre de la sélection ; 2 blocs au moins), Répartir (horizontalement ou
    verticalement, à espacement égal, le premier et le dernier restant en place ; 3 blocs au moins),
    Grouper, Dégrouper, Dupliquer, Supprimer. Un groupe compte comme un seul bloc ; un alignement ou
    une répartition qui rapprocherait deux visuels à moins de la marge est refusé (rien ne bouge).
  - **Affichage** (replié à l'ouverture) : lignes de résolution, lignes en chevauchement,
    alignement automatique pendant le déplacement, avertissement sur l'arrière-plan (coins opaques :
    fond non détouré), avertissement de transparence (plus de 0,5 % de pixels semi-transparents),
    grille 1 cm, fond de la planche (damier, gris, noir, blanc, couleur au choix).

  Ces réglages sont repris à l'ouverture suivante.
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
