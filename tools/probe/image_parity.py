#!/usr/bin/env python3
"""P0 probe: can Rust reproduce Pillow's image output byte-for-byte?

Produces, in ``--out``:

* ``raw_rgb.bin`` / ``raw_l.bin`` -- deterministic source pixels
* ``pillow.jpg``                  -- Pillow default JPEG encoding (DCTDecode)
* ``pillow.png``                  -- Pillow default PNG encoding
* ``pillow.pdf``                  -- Pillow's PDF plugin output (what pypdf's
                                     ``ImageFile.replace`` relies on)
* ``pillow_idat.bin``             -- concatenated PNG IDAT payload
* ``png_filtered.bin``            -- the *filtered* scanlines (decompressed IDAT),
                                     i.e. the exact input Pillow's zlib saw
* ``pdf_dct.bin``                 -- the JPEG stream extracted from pillow.pdf
* ``probe.json``                  -- sizes/parameters for the comparison harness

The zlib part of the parity question is answered by feeding ``png_filtered.bin``
to the same zlib-ng build Pillow uses and comparing against ``pillow_idat.bin``
(see ``tools/probe/zlib_probe.c``).  The JPEG part is answered by
``tools/probe/jpeg_probe.c`` against ``raw_rgb.bin``.
"""

from __future__ import annotations

import argparse
import json
import zlib
from pathlib import Path


def make_rgb(width: int = 64, height: int = 48) -> bytes:
    out = bytearray()
    for y in range(height):
        for x in range(width):
            out += bytes(
                (
                    (x * 7 + y * 13) % 256,
                    (x * 3 + y * 29) % 256,
                    (x * 17 + y * 5) % 256,
                )
            )
    return bytes(out)


def make_l(width: int = 64, height: int = 48) -> bytes:
    return bytes(((x * 5 + y * 11) % 256) for y in range(height) for x in range(width))


def png_chunks(data: bytes):
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    offset = 8
    while offset < len(data):
        length = int.from_bytes(data[offset : offset + 4], "big")
        kind = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        yield kind, payload
        offset += 12 + length


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    from PIL import Image

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    width, height = 64, 48
    raw_rgb = make_rgb(width, height)
    raw_l = make_l(width, height)
    (out / "raw_rgb.bin").write_bytes(raw_rgb)
    (out / "raw_l.bin").write_bytes(raw_l)

    rgb = Image.frombytes("RGB", (width, height), raw_rgb)
    gray = Image.frombytes("L", (width, height), raw_l)

    rgb.save(out / "pillow.jpg", "JPEG")
    rgb.save(out / "pillow.png", "PNG")
    rgb.save(out / "pillow.pdf", "PDF")
    gray.save(out / "pillow_gray.jpg", "JPEG")
    gray.save(out / "pillow_gray.png", "PNG")

    png = (out / "pillow.png").read_bytes()
    idat = b""
    filters: list[int] = []
    for kind, payload in png_chunks(png):
        if kind == b"IDAT":
            idat += payload
    (out / "pillow_idat.bin").write_bytes(idat)
    filtered = zlib.decompress(idat)
    (out / "png_filtered.bin").write_bytes(filtered)

    # Row filters used by Pillow's PNG encoder (first byte of every scanline).
    stride = width * 3 + 1
    filters = [filtered[i * stride] for i in range(height)]

    pdf = (out / "pillow.pdf").read_bytes()
    marker = b"stream\n"
    start = pdf.index(marker) + len(marker) if marker in pdf else -1
    end = pdf.index(b"\nendstream", start) if start >= 0 else -1
    assert start >= 0 and end >= 0, "could not find image stream in pillow.pdf"
    (out / "pdf_dct.bin").write_bytes(pdf[start:end])

    info = {
        "pillow": Image.__version__ if hasattr(Image, "__version__") else None,
        "size": [width, height],
        "raw_rgb_sha": __import__("hashlib").sha256(raw_rgb).hexdigest(),
        "jpeg_len": (out / "pillow.jpg").stat().st_size,
        "png_len": (out / "pillow.png").stat().st_size,
        "pdf_len": len(pdf),
        "dct_len": end - start,
        "idat_len": len(idat),
        "filtered_len": len(filtered),
        "png_row_filters": filters,
        "jpeg_matches_pdf_dct": (out / "pillow.jpg").read_bytes() == pdf[start:end],
    }
    (out / "probe.json").write_text(json.dumps(info, indent=1), encoding="utf-8")
    print(json.dumps(info, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
