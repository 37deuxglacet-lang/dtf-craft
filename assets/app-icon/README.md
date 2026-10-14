# DTF Craft app icon

The DTF Craft icon (raccoon with a brush and a film roll on a blue rounded tile), supplied by
FORMATION DTF — Stéphane Glacet. `dtfcraft-logo.png` is the original artwork with the
"dtfcraft" wordmark; `dtfcraft-master.png` is the tile alone, enlarged ×4 with DTF Craft's AI
upscale and cut out with transparent rounded corners.

Every render below is generated from `dtfcraft-master.png` (file names kept from PhotoCraft so
the build and packaging scripts are unchanged):

- `photocraft-1024.png`: macOS grid (824/1024 body, transparent margin), also the window icon;
- `photocraft.ico`: Windows executable and window icon, 16–256 px;
- `photocraft.icns`: macOS bundle;
- `hicolor/*/apps/ai.storyteller.photocraft.png` and `hicolor/scalable/…svg`: Linux theme and
  the in-app brand tile.

`photocraft.svg` / `photocraft-small.svg` are PhotoCraft's original vector icon, kept only for a
file-import test; DTF Craft does not show them. `packaging/icons.sh` regenerates PhotoCraft's
icon from that SVG: do not run it for DTF Craft.
