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
| Halftone DTF… | Deux modes : pixels semi-transparents seulement, ou visuel entier (textile foncé/clair). Crée un dossier « Halftone DTF » au-dessus du calque source (masqué, intact) : calque « Halftone » tramé, sur un « Fond de contrôle » #231c1c (textile foncé) ou blanc (textile clair) ; sans fond pour les semi-transparents. L'aperçu montre ce résultat |
| Gang sheet… | Fenêtre plein écran claire : import, réglages, plusieurs planches, construction auto, contrôle de la résolution, des chevauchements et des fonds, export PNG 300 dpi ouvert comme nouveau document (voir ci-dessous) |
| Vectorisation… | Module plein écran (comme Camera Raw) : le visuel est réduit à une palette (8 couleurs par défaut, 2 à 16) puis tracé en formes vectorielles découpées, une couleur unie par forme, sans superposition. Réglages : type de visuel (logo · aplats / illustration · nuances), niveau de détail, lissage (courbes / angles vifs), suppression du fond lu au bord du visuel, joints fermés entre couleurs (chaque forme déborde d'un demi-pixel sur ses voisines ; à décocher pour un traceur de découpe). Aperçu original / vectorisé / côte à côte, contours, palette où décocher une couleur la retire du SVG. Export SVG à la taille d'impression du document, ouvert en option dans un nouvel onglet en calques de forme ; le document n'est pas modifié. Traceur : visioncortex (MIT OR Apache-2.0) |

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

Retouches (Netteté, Saturation, Pixels transparents, Supprimer franges) : l'aperçu s'affiche dès
l'ouverture de la fenêtre ; « Afficher l'avant » montre l'image d'origine pour comparer.

Détourage : l'aperçu s'affiche dès l'ouverture de la fenêtre, sur la dernière
couleur utilisée (Fond de contrôle ou « Fond d'aperçu »). La couleur du fond est détectée sur les
bords si elle est unie, ou prise à la pipette. « Ajouter des trous » et « Couleur du fond à la
pipette » sont deux boutons à bascule exclusifs ; sans bouton enfoncé, un clic sur l'image ne fait
rien. Chaque trou cliqué s'ajoute à l'aperçu et est percé avec le fond à l'application ;
Ctrl+Alt+Z annule puis rétablit le dernier changement avant « Appliquer ».

## Gang sheet

DTF › Gang sheet… ouvre une fenêtre plein écran, toujours claire (quel que soit le thème), sur le
modèle des constructeurs de planches en ligne. Échap ou Annuler la referme.

- **En-tête** : Annuler, Exporter le PNG… (ou Exporter les planches quand il y en a plusieurs).
- **Barre d'outils** : format de la planche (laize × longueur), annuler / rétablir, grille, marge
  de l'image (masquée sur un écran étroit, toujours dans Paramètres), outils de la sélection
  (rotation 90° dans les deux sens, retournement horizontal et vertical, étirement à la laize ou à
  la longueur pour un visuel seul, centrage horizontal, vertical ou sur la planche, pousser à
  gauche, à droite, en haut, en bas, grouper, dégrouper, supprimer) et Construction auto.
- **Accueil** (colonne de gauche) : zone d'import (« Importer des images », PNG uniquement, ou
  fichiers glissés sur la fenêtre ; « Ajouter le document ouvert », aplati). Chaque visuel est
  rogné à ses pixels visibles et apparaît en vignette avec son nombre d'exemplaires sur la planche
  (clic : un exemplaire de plus ; × : retirer le visuel). Le visuel sélectionné se règle :
  largeur et hauteur en cm, proportions verrouillables, résolution, avertissements, quantité
  d'images sur la planche, marge propre (interrupteur), Remplissage automatique (remplit la place
  restante), Dupliquer l'image. Plusieurs visuels sélectionnés (ou un groupe) : leur nombre, la
  taille de leur cadre, Grouper, Dégrouper, Dupliquer, Supprimer.
- **Champs numériques** : ils se saisissent au clavier ; glisser la souris dessus ne change rien.
- **Paramètres** : lignes de résolution, lignes en chevauchement, alignement automatique pendant
  le déplacement, avertissement sur l'arrière-plan (coins opaques : fond non détouré),
  avertissement de transparence (plus de 0,5 % de pixels semi-transparents), grille 1 cm, fond de
  la planche (damier, gris, noir, blanc, couleur au choix), marge du plan de travail (le long des
  bords, 1 cm par défaut), marge de l'image (entre les visuels, 1 cm par défaut), laize (56 cm par
  défaut), longueur max (100 cm au plus), rotation autorisée pour la construction auto. Ces
  réglages sont repris à l'ouverture suivante.
- **Planche** : règles en cm, carte en haut à gauche (afficher les lignes en chevauchement et de
  résolution, légende, et pour le visuel survolé ou sélectionné : position gauche / haut, largeur,
  hauteur, résolution, avertissements). Contours : vert ≥ 300 ppp, jaune ≥ 250, rouge ≥ 200, noir
  en dessous ; bleu pour un visuel en chevauchement ou trop proche. Molette : défilement ;
  Ctrl + molette : zoom ; boutons de zoom en bas à droite.
- **Sélection** : clic sur un visuel ; Maj+clic (ou Ctrl+clic) ajoute ou retire un visuel ; un
  rectangle tracé depuis une zone vide du film prend tous les visuels qu'il touche (Maj : ajoutés à
  la sélection) ; Ctrl+A : tous ; Échap : plus rien.
- **Groupes** : Ctrl+G groupe la sélection, Ctrl+Maj+G dégroupe (aussi dans la barre d'outils, le
  clic droit et la colonne de gauche). Un clic sur un visuel du groupe prend tout le groupe ; un
  groupe est entouré d'un cadre pointillé et garde les écarts entre ses visuels. Annuler revient
  sur un groupage.
- **Déplacement** : glisser au millimètre ; la sélection se déplace d'un bloc et reste sur le
  film, marge du plan de travail comprise. Lâchée à moins de la marge d'un autre visuel (la plus
  grande des deux marges), elle revient à sa place. Rotation : le bloc tourne autour de son centre,
  chaque visuel tourne avec lui ; retournement : le bloc est retourné. Flèches : 1 mm (Maj : 1 cm) ;
  Suppr, Ctrl+C / Ctrl+X / Ctrl+V, Ctrl+D, Ctrl+Z / Ctrl+Y ; clic droit : copier, coller,
  supprimer, dupliquer, grouper, dégrouper, tout sélectionner, remplissage automatique, rotation.
- **Gang sheets actives** (colonne de droite) : une carte par planche (format, nom modifiable,
  nombre d'images, quantité à imprimer, dupliquer, supprimer) ; Ajouter une nouvelle planche,
  Construction auto, Recommencer ; longueur utilisée.
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
(entre visuels), en mm ; `marginMm` par visuel.

## Agrandissement IA

Lancé depuis la fenêtre Conformité impression (boutons ×2, ×3, ×4) ; il n'est plus dans le menu DTF.

Le modèle `realesr-general-x4v3` de Real-ESRGAN (© 2021 Xintao Wang, licence BSD 3-Clause, voir
`LICENSE-Real-ESRGAN.txt`) est intégré à l'application. Le calcul se fait sur le processeur,
sans connexion internet : compter environ une à deux minutes pour une image de 1000 × 1000 px
sur un processeur 8 cœurs.

## Réglages

DTF Craft garde ses réglages à part de PhotoCraft : `%APPDATA%\DTF Craft` sous Windows.

## Compilation

GitHub Actions compile la version Windows (`.github/workflows/dtf-craft-windows.yml`) à chaque
envoi sur la branche `main` ; le zip portable est dans les artefacts de l'exécution (conservés
un jour). Aucun binaire n'est joint aux releases publiques ; sur une étiquette ou un lancement manuel
(`upload_name`), le zip est envoyé sur l'hébergement Nuxit quand les secrets `NUXIT_FTP_*` sont définis.
