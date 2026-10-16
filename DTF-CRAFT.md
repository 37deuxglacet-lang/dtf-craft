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
| Agrandissement IA… | ×2, ×3 ou ×4 par le modèle Real-ESRGAN (hors ligne, sur le processeur), sur une copie nommée « nom x2 » : l'original n'est pas modifié |
| Analyseur couleurs… | Fenêtre plein écran : couleurs principales du document ouvert (RVB) et planche de variantes autour de chacune, PNG sRGB 300 ppp ouvert comme nouveau document (voir ci-dessous) |

Toutes les commandes sont annulables (Édition › Annuler) et utilisables dans les actions.

## Onglets DTF

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

Conformité impression : largeur et/ou hauteur en cm ; avec une seule valeur l'autre suit les
proportions, avec les deux l'image tient dans le cadre largeur × hauteur. La transparence est
rognée avant le redimensionnement.

## Analyseur couleurs

DTF › Analyseur couleurs… ouvre une fenêtre plein écran sur le document ouvert (Échap ou Annuler
la referme). Document RVB uniquement ; il est aplati et converti en sRGB pour l'analyse, de sorte
que les codes affichés sont des codes sRGB.

- Couleurs (colonne de gauche) : les couleurs principales du visuel, 6 par défaut, la plus présente
  d'abord, avec leur part de la surface. Les pixels transparents (opacité sous 50 %) sont ignorés ;
  l'anticrénelage et les teintes proches sont fusionnés. Pipette : un clic sur le visuel ajoute la
  couleur sous le pointeur ; un code hexadécimal peut aussi être saisi. Le code de chaque couleur
  est modifiable ; « Restaurer » revient à la couleur d'origine ; la case à cocher la met ou non
  sur la planche.
- Analyse (colonne de droite) : nombre de couleurs (1 à 32) et fusion des teintes proches ; un
  changement relance l'analyse (les codes modifiés sont alors perdus).
- Variantes : deux axes au choix parmi Luminosité, Saturation (en points de %), Teinte (en
  degrés), Rouge, Vert, Bleu (en % de 255) ; Luminosité à l'horizontale et Saturation à la
  verticale par défaut. Pas de 5 % par défaut ; blocs de 5 × 5 cases (3 × 3 ou 7 × 7 au choix).
  L'axe horizontal croît vers la droite, l'axe vertical vers le haut ; la couleur est au centre.
- Planche : 56 cm de large par défaut, cases de 15 mm au pas de 16,5 mm, un bloc par couleur
  (six blocs par rangée sur 56 cm). En-tête : nom du document, date, profil, axes et pas, champs
  Machine / Film / Encre à remplir, et le visuel en réduction. Sous chaque case : son numéro, son
  code et son écart (ex. « L+10 S−5 ») ; la case d'origine est encadrée de noir. Sous chaque bloc,
  une ligne pour reporter la case retenue.
- Après impression : un clic sur une case de la planche la sélectionne (flèches pour se déplacer
  dans le bloc). « Prendre comme centre » en fait le nouveau centre de sa couleur, pour une
  seconde planche plus fine (réduire le pas) ; « Copier le code » et « Copier les codes » mettent
  les codes dans le presse-papiers (« Couleur 1 : #C03020 → #B04436 »).
- Exporter en PNG… : la planche est rendue à 300 ppp, en sRGB (profil inscrit), sur fond
  transparent, en trois calques (« Cases », « Visuel », « Textes » en noir, sans pixels
  semi-transparents) ; elle est enregistrée en PNG et ouverte comme nouveau document. À imprimer
  comme un fichier de production (même RIP, même profil, même film).

Les réglages sont repris à l'ouverture suivante. La commande `dtf.colorAnalyzer` fait le même
travail sans fenêtre (actions, ligne de commande) : sans `colors`, elle extrait elle-même les
couleurs ; avec `colors`, elle reprend les codes donnés.

## Agrandissement IA

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
