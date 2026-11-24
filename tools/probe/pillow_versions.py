#!/usr/bin/env python3
"""Report the exact native library versions bundled with the oracle's Pillow.

Gate A5 requires byte-identical image output.  That is only achievable if the
Rust side links the same codec libraries (libjpeg-turbo / zlib / libtiff /
openjpeg) as the Pillow wheel used by the oracle.  Record the ground truth here
and pin it in ``requirements-oracle.lock`` + ``docs/FIDELITY_CONTRACT.md``.

Usage:
    .venvs/oracle/bin/python tools/probe/pillow_versions.py [--json out.json]
"""

from __future__ import annotations

import argparse
import json
import platform
import sys
from pathlib import Path


def collect() -> dict:
    from PIL import features

    info: dict = {
        "python": sys.version,
        "platform": platform.platform(),
        "machine": platform.machine(),
    }
    try:
        import PIL

        info["pillow"] = PIL.__version__
    except Exception as exc:  # noqa: BLE001
        info["pillow_error"] = repr(exc)

    feature_names = [
        "jpg",
        "jpg_2000",
        "zlib",
        "libtiff",
        "webp",
        "webp_anim",
        "raqm",
        "freetype2",
        "harfbuzz",
        "fribidi",
        "lcms",
        "xcb",
    ]
    for name in feature_names:
        entry: dict = {}
        try:
            entry["supported"] = features.check(name)
        except Exception as exc:  # noqa: BLE001
            entry["supported"] = f"error: {exc!r}"
        try:
            entry["version"] = features.version(name)
        except Exception as exc:  # noqa: BLE001
            entry["version"] = f"error: {exc!r}"
        info[f"feature:{name}"] = entry

    try:
        info["features_report"] = features.get_supported()
    except Exception as exc:  # noqa: BLE001
        info["features_report"] = repr(exc)
    return info


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json")
    args = parser.parse_args()

    info = collect()
    text = json.dumps(info, indent=1, sort_keys=True, ensure_ascii=False)
    print(text)
    if args.json:
        out = Path(args.json)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(text, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
