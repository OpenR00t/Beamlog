#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 The Beamlog contributors
"""Build the release downloads into dist/:

  beamlog-mod.zip        the recorder and its UI app, for BeamNG's mods folder:
                         the game's folders (lua/, ui/) at the zip's root,
                         forward slashes in the entry names
  beamlog-workspace.zip  the MoTeC i2 workspace, a Beamlog/ folder to put in
                         Documents/MoTeC/i2/Workspaces

    python scripts/package.py [DIST_DIR]
"""
import pathlib
import sys
import tempfile
import xml.etree.ElementTree as ET
import zipfile

import build_workspace

ROOT = pathlib.Path(__file__).resolve().parent.parent
MOD = ROOT / "mod" / "beamlog"


def zip_dir(src: pathlib.Path, out: pathlib.Path, prefix: str = "") -> int:
    files = sorted(p for p in src.rglob("*") if p.is_file())
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for p in files:
            z.write(p, prefix + p.relative_to(src).as_posix())
        # keep the workspace's empty folders, which i2 expects
        for d in sorted(p for p in src.rglob("*") if p.is_dir() and not any(p.iterdir())):
            z.writestr(prefix + d.relative_to(src).as_posix() + "/", "")
    return len(files)


def main() -> None:
    dist = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "dist"
    dist.mkdir(parents=True, exist_ok=True)

    n = zip_dir(MOD, dist / "beamlog-mod.zip")
    print(f"{dist / 'beamlog-mod.zip'} ({n} files)")

    with tempfile.TemporaryDirectory() as tmp:
        ws = pathlib.Path(tmp) / "Beamlog"
        files = build_workspace.build(ws)
        for rel, text in files.items():
            if text.strip() != '<?xml version="1.0"?>':
                ET.fromstring(text)  # every file must be well-formed
        n = zip_dir(ws, dist / "beamlog-workspace.zip", "Beamlog/")
    print(f"{dist / 'beamlog-workspace.zip'} ({n} files)")


if __name__ == "__main__":
    main()
