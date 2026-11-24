#!/usr/bin/env python3
"""Run a mutated-PDF corpus and dump canonical results (gate A7).

Run in the oracle venv and in the dev venv over the same corpus; the two JSON
files must be identical (same success/failure decision, same exception type and
message, same canonical object graph).

Usage:
    .venvs/oracle/bin/python tools/fuzz/run.py --corpus .audit/fuzz/corpus \
        --out .audit/fuzz/oracle.json
"""

from __future__ import annotations

import argparse
import json
import signal
import sys
import warnings
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "tools" / "oracle"))

from canon import canon  # noqa: E402
from prng import install_deterministic_secrets  # noqa: E402


class Timeout(Exception):
    pass


def _handler(signum, frame):  # noqa: ANN001, ARG001
    raise Timeout("scenario timed out")


def scenario(path: Path) -> dict:
    from pypdf import PdfReader

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        reader = PdfReader(str(path), strict=False)
        value = {
            "is_encrypted": reader.is_encrypted,
            "num_pages": len(reader.pages),
            "metadata": canon(dict(reader.metadata or {})),
            "labels": canon(reader.page_labels) if len(reader.pages) else None,
            "outline": canon(reader.outline),
            "trailer": canon(sorted(str(key) for key in reader.trailer.keys())),
        }
        if len(reader.pages):
            page = reader.pages[0]
            value["page"] = {
                "mediabox": canon(page.mediabox),
                "rotation": canon(page.rotation),
                "text": page.extract_text(),
            }
    return {
        "value": value,
        "warnings": [
            {"category": item.category.__name__, "message": str(item.message)}
            for item in caught
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=10)
    args = parser.parse_args()

    install_deterministic_secrets()
    signal.signal(signal.SIGALRM, _handler)

    results: dict[str, dict] = {}
    files = sorted(path for path in args.corpus.glob("*.pdf"))
    for path in files:
        signal.alarm(args.timeout)
        try:
            results[path.name] = {"error": None, "result": scenario(path)}
        except BaseException as exc:  # noqa: BLE001 - failures are the data
            results[path.name] = {
                "error": {"type": type(exc).__name__, "msg": str(exc)},
                "result": None,
            }
        finally:
            signal.alarm(0)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    payload = {"files": results}
    args.out.write_text(
        json.dumps(payload, indent=0, sort_keys=True, ensure_ascii=False),
        encoding="utf-8",
    )
    errors = sum(1 for item in results.values() if item["error"])
    print(f"ran {len(results)} files ({errors} raised) -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
