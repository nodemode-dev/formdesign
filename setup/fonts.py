#!/usr/bin/env python3
import json
import shutil
import sys
from pathlib import Path

from fontTools.ttLib import TTFont

HEAVY = {"playfair-display", "fraunces", "montserrat", "archivo", "poppins", "work-sans", "inter-tight", "raleway",
         "merriweather", "barlow", "barlow-condensed", "figtree", "outfit", "syne", "manrope", "dm-sans", "space-grotesk",
         "source-sans-3", "source-serif-4", "newsreader", "eb-garamond", "lora", "cormorant-garamond"}
SEMI = {"dm-sans", "manrope", "figtree", "outfit", "inter-tight", "space-grotesk", "work-sans", "poppins", "montserrat",
        "ibm-plex-sans", "barlow", "source-sans-3"}
SKIP = {"inter", "jetbrains-mono"}


def family_name(font: TTFont) -> tuple[str, str]:
    names = font["name"]
    fam = names.getDebugName(16) or names.getDebugName(1) or "Unknown"
    sub = names.getDebugName(17) or names.getDebugName(2) or "Regular"
    return fam, sub


WEIGHT_NAMES = {600: "SemiBold", 900: "Black"}


def convert(src: Path, dest_dir: Path, base: str | None = None, weight: int = 400, italic: bool = False) -> tuple[str, str] | None:
    font = TTFont(str(src))
    font.flavor = None
    fam, sub = family_name(font)
    if base and weight in WEIGHT_NAMES:
        fam, sub = base, WEIGHT_NAMES[weight] + (" Italic" if italic else "")
        for rec in list(font["name"].names):
            if rec.nameID in (16, 17):
                font["name"].removeNames(nameID=rec.nameID)
        font["name"].setName(fam, 16, 3, 1, 0x409)
        font["name"].setName(sub, 17, 3, 1, 0x409)
    out = dest_dir / f"{fam.replace(' ', '')}-{sub.replace(' ', '')}.ttf"
    font.save(str(out))
    return fam, sub


def main(fontsource: Path, dest: Path) -> int:
    if not fontsource.is_dir():
        print(f"[formdesign] no @fontsource packages at {fontsource}: run npm install in frontend/", file=sys.stderr)
        return 1
    dest.mkdir(parents=True, exist_ok=True)
    (dest / "licenses").mkdir(exist_ok=True)
    manifest: dict[str, list[str]] = {}
    for pkg in sorted(p for p in fontsource.iterdir() if p.is_dir() and p.name not in SKIP):
        files = pkg / "files"
        if not files.is_dir():
            continue
        weights = [400, 700] + ([600] if pkg.name in SEMI else []) + ([900] if pkg.name in HEAVY else [])
        picked = []
        for w in weights:
            for style in ("normal", "italic"):
                f = files / f"{pkg.name}-latin-{w}-{style}.woff"
                if f.is_file():
                    picked.append((f, w, style == "italic"))
        if not picked:
            picked = [(f, 400, False) for f in sorted(files.glob(f"{pkg.name}-latin-*-normal.woff"))[:1]]
        base = family_name(TTFont(str(picked[0][0])))[0] if picked else None
        for f, w, italic in picked:
            got = convert(f, dest, base, w, italic)
            if got:
                manifest.setdefault(got[0], [])
                if got[1] not in manifest[got[0]]:
                    manifest[got[0]].append(got[1])
        lic = pkg / "LICENSE"
        if lic.is_file():
            shutil.copyfile(lic, dest / "licenses" / f"OFL-{pkg.name}.txt")
    (dest / "fonts.json").write_text(json.dumps(manifest, indent=2, sort_keys=True))
    faces = sum(len(v) for v in manifest.values())
    print(f"[formdesign] font library: {len(manifest)} families, {faces} faces in {dest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(Path(sys.argv[1]), Path(sys.argv[2])))
