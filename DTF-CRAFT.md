# DTF Craft

Version de PhotoCraft dédiée à la préparation de fichiers DTF (impression directe sur film),
par Stéphane GLACET (FORMATION DTF) — [www.dtfcraft.fr](https://www.dtfcraft.fr), contact@stephane-glacet.com.

DTF Craft est une version dérivée de [PhotoCraft](https://github.com/storytold/photocraft)
(licence MIT ou Apache-2.0, voir `LICENSE-MIT`, `LICENSE-APACHE` et `NOTICE`). Ce n'est pas un
produit officiel du projet PhotoCraft.

## Menu DTF

| Commande | Rôle |
|---|---|
| Conformité impression… | Calque « origin », hauteur d'impression (cm) à la résolution voulue, 8/16 bits, rognage de la transparence |
| Suppression fond blanc… | Copie du calque « Sans fond » au-dessus de la source (masquée, intacte) ; le fond uni (couleur lue sur les bords, ou choisie) est supprimé depuis les bords, le blanc intérieur au visuel reste. Tolérance (bruit JPEG), contour décontaminé (pas de liseré clair), trous intérieurs en option (désactivés par défaut, taille minimale en px). Aperçu sur fond #231c1c |
| Fond de contrôle… | Calque de remplissage « backfill » sous l'image, couleur du textile, avec aperçu ; remplace un fond de contrôle déjà présent |
| Suppression fond de contrôle | Supprime les calques « backfill » |
| Pixels transparents… | Supprime les pixels semi-transparents (seuil d'opacité, 62 % par défaut) |
| Suppression frange… | Supprime les franges de couleur sur les bords |
| Netteté optimisée… | Netteté optimisée avec aperçu |
| Saturation… | Saturation (+15 par défaut) avec aperçu |
| Halftone DTF… | Deux modes : pixels semi-transparents seulement, ou visuel entier (textile foncé/clair). Crée un dossier « Halftone DTF » au-dessus du calque source (masqué, intact) : calque « Halftone » tramé, sur un « Fond de contrôle » #231c1c (textile foncé) ou blanc (textile clair) ; sans fond pour les semi-transparents. L'aperçu montre ce résultat |
| Gang sheet… | Fenêtre plein écran : visuels placés sur la laize, contrôle de la résolution et des marges, export PNG 300 dpi ouvert comme nouveau document (voir ci-dessous) |

Toutes les commandes sont annulables (Édition › Annuler) et utilisables dans les actions.

## Onglets DTF

Chaque onglet DTF et la fenêtre Halftone DTF ont un bouton « ? » qui ouvre la fiche d'aide de l'outil
(à quoi il sert, quand l'utiliser, réglages conseillés) ; chaque réglage a une infobulle au survol.

Chaque outil DTF a son onglet (Conformité, Détourage, Fond de contrôle, Pixels transparents,
Supprimer franges, Netteté, Saturation). Ils se déplacent, se regroupent, se détachent et
s'ancrent comme les autres panneaux ; Fenêtre › Conformité impression (et les autres outils) les
affiche. Chaque onglet a ses réglages et « Appliquer » ; l'aperçu s'affiche sur l'image dès qu'un
réglage bouge (sauf Conformité), tant que l'onglet de l'outil est visible. L'onglet Fond de
contrôle a deux boutons : « Afficher le fond » (remplace un fond déjà présent) et « Supprimer le fond ».

Espace de travail DTF (Fenêtre › Espace de travail › DTF, ouvert au démarrage) : la colonne de
droite en trois groupes de trois onglets au plus — Fond de contrôle, Netteté, Saturation ;
Pixels transparents, Supprimer franges ; Calques, Conformité, Détourage. Canaux et Tracés n'y
sont pas (menu Fenêtre). L'accueil donne le lien du projet GitHub de DTF Craft.

Conformité impression : l'onglet part de la taille actuelle du visuel (rogné si « Rogner la
transparence » est coché) et de la résolution du document ; changer la largeur met à jour la
hauteur, et inversement ; changer la résolution recalcule la taille en cm sans rééchantillonner les
pixels. Les boutons ×2, ×3, ×4 lancent l'Agrandissement IA à la résolution de l'onglet et
indiquent la taille obtenue (cm et px). La transparence est rognée avant le redimensionnement, en laissant
2 px transparents autour du visuel. Le calque est renommé « origin » (champ Nom du calque).

Retouches (Netteté, Saturation, Pixels transparents, Supprimer franges) : l'aperçu s'affiche dès
que l'onglet est ouvert ou cliqué ; « Afficher l'avant » montre l'image d'origine pour comparer.

Détourage : l'aperçu s'affiche dès l'ouverture de l'onglet, sur la dernière couleur utilisée
(Fond de contrôle ou « Fond d'aperçu ») ; « Ajouter des trous (gomme magique) » active la gomme magique (en mode contigu), avec sa tolérance
réglable dans l'onglet : chaque clic dans une zone fermée du calque « Sans fond » l'efface (une
annulation par clic) ; décocher rend l'outil précédent.

## Gang sheet

DTF › Gang sheet… ouvre une fenêtre plein écran (Échap ou Annuler la referme).

- Visuels (colonne de gauche) : « Ajouter des PNG… » (plusieurs fichiers à la fois, ou glissés sur
  la fenêtre) ou « Ajouter le document ouvert » (aplati). Chaque visuel est rogné à ses pixels
  visibles. Sa largeur d'impression est réglée en cm, la hauteur suit (proportions verrouillées) ;
  au départ, c'est la taille enregistrée dans le fichier. Le nombre de copies place ou retire des
  exemplaires sur la planche.
- Résolution : la résolution d'impression de chaque visuel s'affiche en couleur, dans la liste et
  sur la planche : vert ≥ 300 ppp, jaune ≥ 250, orange ≥ 200, rouge en dessous.
- Planche (colonne de droite) : laize (56 cm par défaut), longueur max (100 cm au plus), marge
  autour des visuels (5 mm par défaut), rotation autorisée ou non pour la construction auto.
  La marge est la distance minimale entre deux visuels et entre un visuel et le bord du film.
  Ces réglages sont repris à l'ouverture suivante.
- Construction auto : tous les exemplaires sont replacés (algorithme MaxRects, les plus grands
  d'abord, la planche la plus courte possible). Ce qui ne tient pas sur la planche est signalé
  « non placé ».
- Placement manuel : glisser un visuel le déplace au millimètre ; il reste sur le film, marge
  comprise. Lâché à moins de la marge d'un autre visuel, il revient à la dernière position
  correcte. Flèches : 1 mm (Maj : 1 cm) ; Suppr retire le visuel ; Pivoter 90°, Dupliquer et
  Supprimer agissent sur le visuel sélectionné. Molette : défilement ; Ctrl + molette : zoom.
- Les visuels trop proches ou qui débordent sont en rouge ; l'export le signale et demande
  confirmation.
- Exporter en PNG… : la planche est rendue à 300 ppp, à la laize et à la longueur utilisée,
  chaque visuel sur son propre calque ; elle est enregistrée en PNG (résolution inscrite dans le
  fichier) et ouverte comme nouveau document.

La commande `dtf.gangSheet` fait le même travail sans fenêtre (actions, ligne de commande) : avec
`sources` seulement, elle construit la planche elle-même ; avec `placements`, elle reprend la mise
en place donnée.

## Agrandissement IA

Lancé depuis l'onglet Conformité (boutons ×2, ×3, ×4) ; il n'est plus dans le menu DTF.

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
