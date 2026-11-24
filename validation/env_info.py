#!/usr/bin/env python3
"""§2.1: capture environment facts for the reference and port backends."""
from __future__ import annotations

import json
import platform
import sys
from pathlib import Path


def main() -> int:
    import pypdf

    try:
        import pypdf._pypdf as _ext
        rust_extension = getattr(_ext, "__file__", None)
    except ImportError:
        rust_extension = None

    info = {
        "python": sys.version.split()[0],
        "platform": platform.platform(),
        "machine": platform.machine(),
        "pypdf_version": pypdf.__version__,
        "module": pypdf.__file__,
        "rust_extension": rust_extension,
        "executable": sys.executable,
    }
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else None
    text = json.dumps(info, indent=2, ensure_ascii=False)
    if out:
        out.write_text(text + "\n")
    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
