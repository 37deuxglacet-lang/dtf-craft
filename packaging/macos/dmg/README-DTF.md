# DTF Craft DMG window

DTF Craft's macOS DMG (`packaging/macos/dtf-package.sh`) uses its own window, in the same layout
as PhotoCraft's (`README.md` here): 660 × 400 pt, the app at (326, 205), `Applications` at
(574, 205), icon size 128.

| File | What |
|---|---|
| `dtf-background.svg` | Source: the DTF Craft tile (`assets/app-icon/dtfcraft-master.png`, linked) on a navy panel, the title, « Outils DTF · basé sur PhotoCraft » and « Glissez DTF Craft dans Applications ». Text outlined (usvg), no fonts needed. |
| `dtf-background.tiff` | The background at 1x and 2x, full colour (the tile's gradient would band in a 16-colour palette). |
| `dtf-layout.DS_Store` | Finder's view settings for the `DTF Craft` volume. |
| `dtf-generate.py` | Writes both from the SVG, reusing `generate.py`. |

The volume is called `DTF Craft` (no version): the layout finds the background through an alias
holding the volume name.

Regenerate after editing `dtf-background.svg`:

```sh
pip install 'pillow>=12' ds_store==1.3.3 mac_alias==2.2.3
python3 packaging/macos/dmg/dtf-generate.py   # needs resvg on PATH
```
