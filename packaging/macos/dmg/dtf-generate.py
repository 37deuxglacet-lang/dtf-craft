#!/usr/bin/env python3
# DTF Craft (2026, Stéphane Glacet): DTF Craft's own DMG window, from PhotoCraft's generate.py.
"""Write dtf-background.tiff and dtf-layout.DS_Store for DTF Craft's DMG (see README-DTF.md).

Reuses generate.py (PhotoCraft's) for the .DS_Store and the sRGB profile, with DTF Craft's
volume name and app name. The background keeps its full colours (the raccoon tile is a gradient,
which a 16-colour palette would band).

Needs resvg on PATH and:
    pip install 'pillow>=12' ds_store==1.3.3 mac_alias==2.2.3

    python3 packaging/macos/dmg/dtf-generate.py
"""

import os
import shutil
import sys
import tempfile

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import generate as g  # noqa: E402  (PhotoCraft's generator, same folder)

# Keep in sync with dtf-package.sh (volume name and app name).
g.VOLUME = "DTF Craft"
g.ICONS = {"DTF Craft.app": (326, 205), "Applications": (574, 205)}


def write_tiff(svg, out):
    """1x (72 dpi) and 2x (144 dpi) in one TIFF, RGB, Deflate."""
    with tempfile.TemporaryDirectory() as tmp:
        pages = []
        for s in (1, 2):
            png = os.path.join(tmp, f"{s}x.png")
            # The SVG's text is outlined; the tile is a linked PNG next to the app icons.
            g.subprocess.run(["resvg", "--skip-system-fonts", "-w", str(g.WIDTH * s), svg, png], check=True)
            with Image.open(png) as im:
                pages.append(im.convert("RGB"))
    one, two = pages
    two.encoderinfo = {"dpi": (144, 144)}
    one.save(
        out,
        "TIFF",
        save_all=True,
        append_images=[two],
        compression="tiff_adobe_deflate",  # brand-ok: Pillow's name for TIFF compression 8 (Deflate)
        dpi=(72, 72),
        icc_profile=g.srgb_profile(),
    )


def main():
    if not shutil.which("resvg"):
        sys.exit("error: resvg not found (brew install resvg / cargo install resvg --locked)")
    write_tiff(os.path.join(HERE, "dtf-background.svg"), os.path.join(HERE, "dtf-background.tiff"))
    g.write_ds_store(os.path.join(HERE, "dtf-layout.DS_Store"))


if __name__ == "__main__":
    main()
