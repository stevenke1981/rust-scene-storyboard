#!/usr/bin/env python3
"""Rebuild assets/fonts/NotoSansCJKtc-Subset.otf.br.

Requires: pip install fonttools brotli
Source:   Noto Sans CJK Regular 2.004 (.ttc, face 3 = TC) or NotoSansCJKtc-Regular.otf.

    python3 tools/subset_font.py /usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc 3

Glyphs are subset to tools/font_codepoints.txt (unlike the v0.1 asset, which kept all
32k glyphs). Layout tables, hinting and vertical metrics are dropped because the
renderers (ab_glyph / egui) never use them; CFF is desubroutinised, which compresses
better under Brotli.
"""
import pathlib, subprocess, sys, tempfile

import brotli

root = pathlib.Path(__file__).resolve().parent.parent
src = sys.argv[1] if len(sys.argv) > 1 else "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
face = sys.argv[2] if len(sys.argv) > 2 else "3"
codes = [l.strip() for l in (root / "tools/font_codepoints.txt").read_text().splitlines()
         if l.strip() and not l.startswith("#")]
with tempfile.TemporaryDirectory() as td:
    uni = pathlib.Path(td, "u.txt")
    uni.write_text(",".join(codes))
    out = pathlib.Path(td, "sub.otf")
    cmd = ["pyftsubset", src, f"--unicodes-file={uni}", f"--output-file={out}",
           "--layout-features=", "--no-hinting", "--desubroutinize", "--name-IDs=*",
           "--drop-tables+=GSUB,GPOS,GDEF,BASE,VORG,vhea,vmtx,DSIG"]
    if src.lower().endswith((".ttc", ".otc")):
        cmd.append(f"--font-number={face}")
    subprocess.run(cmd, check=True)
    raw = out.read_bytes()
    br = brotli.compress(raw, quality=11, lgwin=24)
dst = root / "assets/fonts/NotoSansCJKtc-Subset.otf.br"
dst.write_bytes(br)
print(f"{len(raw):,} B otf -> {len(br):,} B brotli -> {dst}")
