#!/usr/bin/env python3
"""
Redraws the installer pictures from packaging/art/installer-art.html into apps/desktop/src-tauri/installer/:

- dmg-background.tiff: the disk image window's background, 660 x 440 points at 1x and 2x in one file.
- nsis-sidebar.bmp (164 x 314) and nsis-header.bmp (150 x 57): the setup .exe's welcome page and page header.
- wix-dialog.bmp (493 x 312) and wix-banner.bmp (493 x 58): the .msi's welcome page and page banner.

It needs macOS (for tiffutil), Google Chrome and Pillow (python3 -m pip install pillow). Run it after changing the
HTML: python3 scripts/installer-art.py
"""

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "packaging" / "art" / "installer-art.html"
OUT = ROOT / "apps" / "desktop" / "src-tauri" / "installer"
CHROME = os.environ.get("CHROME", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")

# Each Windows picture: its size in pixels. They're drawn at twice the size and scaled down, for smooth edges.
BITMAPS = {
    "nsis-sidebar": (164, 314),
    "nsis-header": (150, 57),
    "wix-dialog": (493, 312),
    "wix-banner": (493, 58),
}
DMG = (660, 440)


def render(piece: str, size: tuple[int, int], scale: int, out: Path) -> Image.Image:
    """Screenshots one section of the HTML in a throwaway Chrome profile, at `scale` pixels per point."""
    profile = tempfile.mkdtemp(prefix="smx-art-")
    try:
        subprocess.run(
            [
                CHROME,
                "--headless=new",
                "--hide-scrollbars",
                "--no-first-run",
                "--no-default-browser-check",
                f"--user-data-dir={profile}",
                f"--force-device-scale-factor={scale}",
                f"--window-size={size[0]},{size[1]}",
                f"--screenshot={out}",
                f"{ART.as_uri()}#{piece}",
            ],
            check=True,
            capture_output=True,
            timeout=90,
        )
    finally:
        shutil.rmtree(profile, ignore_errors=True)
    image = Image.open(out).convert("RGB")
    expected = (size[0] * scale, size[1] * scale)
    if image.size != expected:
        sys.exit(f"{piece} came out {image.size[0]} x {image.size[1]}, not {expected[0]} x {expected[1]}")
    return image


def main() -> None:
    if not Path(CHROME).exists():
        sys.exit(f"Chrome isn't at {CHROME}; set CHROME to its path")
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="smx-art-") as tmp:
        work = Path(tmp)
        # The disk image: one TIFF holding both resolutions, so Finder picks the sharp one on a Retina display.
        one = work / "dmg.png"
        two = work / "dmg@2x.png"
        render("dmg", DMG, 1, one).save(one)
        render("dmg", DMG, 2, two).save(two, dpi=(144, 144))
        subprocess.run(
            ["tiffutil", "-cathidpicheck", str(one), str(two), "-out", str(OUT / "dmg-background.tiff")],
            check=True,
            capture_output=True,
        )
        print("dmg-background.tiff")
        for piece, size in BITMAPS.items():
            big = render(piece, size, 2, work / f"{piece}.png")
            # 24-bit BMP, the kind both NSIS and WiX read.
            big.resize(size, Image.LANCZOS).save(OUT / f"{piece}.bmp", format="BMP")
            print(f"{piece}.bmp")


if __name__ == "__main__":
    main()
