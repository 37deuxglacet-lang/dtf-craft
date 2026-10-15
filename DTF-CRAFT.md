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
| Fond de contrôle… | Calque de remplissage « backfill » sous l'image, couleur du textile, avec aperçu |
| Suppression fond de contrôle | Supprime les calques « backfill » |
| Pixels transparents… | Supprime les pixels semi-transparents (seuil d'opacité, 62 % par défaut) |
| Suppression frange… | Supprime les franges de couleur sur les bords |
| Netteté optimisée… | Netteté optimisée avec aperçu |
| Saturation… | Saturation (+15 par défaut) avec aperçu |
| Halftone DTF… | Deux modes : pixels semi-transparents seulement, ou visuel entier (textile foncé/clair). Crée un dossier « Halftone DTF » (calque « Halftone » tramé + « Fond de contrôle » #231c1c) au-dessus du calque source, qui est masqué et conservé intact |
| Agrandissement IA… | ×2, ×3 ou ×4 par le modèle Real-ESRGAN (hors ligne, sur le processeur), sur une copie nommée « nom x2 » : l'original n'est pas modifié |

Toutes les commandes sont annulables (Édition › Annuler) et utilisables dans les actions.

## Onglets DTF

Chaque outil DTF a son onglet (Conformité, Fond de contrôle, Sans fond, Transparence, Franges,
Netteté, Saturation), regroupés au-dessus de Calques. Ils se déplacent, se regroupent, se
détachent et s'ancrent comme les autres panneaux ; Fenêtre › Conformité impression (et les autres
outils) les affiche. Chaque onglet a ses réglages et « Appliquer » ; l'aperçu s'affiche sur l'image
dès qu'un réglage bouge (sauf Conformité et Sans fond). Au démarrage, la colonne ne montre que les
onglets DTF et Calques ; les autres panneaux sont dans le menu Fenêtre.

Conformité impression : largeur et/ou hauteur en cm ; avec une seule valeur l'autre suit les
proportions, avec les deux l'image tient dans le cadre largeur × hauteur. La transparence est
rognée avant le redimensionnement.

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
