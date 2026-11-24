#!/usr/bin/env python3
"""Microbenchmarks for the per-operation costs that the W1/W2/W5/W6/W7 stage
tables attribute time to.

Every benchmark is a tight loop measured with ``perf_counter_ns``; five samples
are taken and the median ns/op is reported.  The benchmarks that need the PyO3
extension (``pypdf._pypdf``) are skipped on the reference backend, and the
reference measures its own Python equivalents instead, so the table always has a
like-for-like pair.

    .venvs/dev/bin/python validation/optimization/scripts/rw_microbench.py \
        --backend rust --out validation/optimization/raw/rw_micro_rust.json
"""
from __future__ import annotations

import argparse
import io
import json
import statistics
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))

import rw_instrument as inst  # noqa: E402


def detect_backend():
    import pypdf

    try:
        import pypdf._pypdf as ext  # noqa: F401

        return "rust"
    except ImportError:
        return "reference"


def bench(label, fn, iters=200_000, samples=5, note=""):
    fn()  # warm-up (also fills caches)
    per_op = []
    for _ in range(samples):
        t0 = time.perf_counter_ns()
        for _ in range(iters):
            fn()
        per_op.append((time.perf_counter_ns() - t0) / iters)
    return {
        "label": label,
        "iters_per_sample": iters,
        "samples_ns_per_op": per_op,
        "median_ns_per_op": statistics.median(per_op),
        "min_ns_per_op": min(per_op),
        "note": note,
    }


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--backend", choices=["rust", "reference"], required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args(argv)

    backend = detect_backend()
    if backend != args.backend:
        raise SystemExit(f"backend mismatch: interpreter is {backend!r}")

    import pypdf
    from pypdf.generic import (
        ByteStringObject,
        DictionaryObject,
        FloatObject,
        IndirectObject,
        NameObject,
        NumberObject,
        read_object,
    )

    results = []

    # 1) baseline: a plain Python function call
    def plain(value):
        return value is None

    probe = object()
    results.append(bench("python_plain_function_call", lambda: plain(probe), note="reference floor for one call"))

    # 2) Python -> Rust boundary: cheapest possible pyfunction body
    if backend == "rust":
        import pypdf._pypdf as ext

        results.append(
            bench(
                "python_to_rust_pyfunction_call",
                lambda: ext.basemod_is_null_or_none(probe),
                note="one crossing, trivial Rust body (is_null_or_none)",
            )
        )
        results.append(
            bench(
                "python_to_rust_pyfunction_call_args2",
                lambda: ext.datastructures_arrayobject_check_limits(
                    remaining_elements=1, parsing_budget=None
                ),
                iters=100_000,
                note="one crossing, keyword arguments (check_limits)",
            )
        )
    else:
        from pypdf.generic import is_null_or_none

        results.append(
            bench(
                "python_to_rust_pyfunction_call",
                lambda: is_null_or_none(probe),
                note="reference equivalent (pure Python)",
            )
        )

    # 3) object parsing: the per-object cost both backends pay through the public API
    for payload, label in [
        (b"123", "parse_number"),
        (b"/SomeName", "parse_name"),
        (b"(a short literal string)", "parse_literal_string"),
        (b"[1 2 3 4]", "parse_array_4"),
        (b"<< /Type /Page /Rotate 90 >>", "parse_dict_2"),
    ]:
        def parse(payload=payload):
            stream = io.BytesIO(payload)
            return read_object(stream, None)

        results.append(
            bench(f"read_object_{label}", parse, iters=20_000, note=f"pypdf.generic.read_object({payload!r})")
        )

    # 4) the token-level stream primitive (BytesIO read(1) + seek(-1))
    stream = io.BytesIO(b"0" * 64)

    def read1_seek():
        stream.read(1)
        stream.seek(-1, 1)

    results.append(bench("bytesio_read1_seek", read1_seek, note="per-token stream access primitive"))

    # 5) object materialisation
    results.append(bench("create_NameObject", lambda: NameObject("/Foo"), note="str subclass, Rust __new__ on the port"))
    results.append(bench("create_NumberObject", lambda: NumberObject(42), note="int subclass, Rust __new__ on the port"))
    results.append(bench("create_FloatObject", lambda: FloatObject("1.5"), note="float subclass, Rust __new__ on the port"))
    results.append(
        bench("create_DictionaryObject", lambda: DictionaryObject(), note="dict subclass (no Rust __init__)")
    )
    results.append(bench("create_ByteStringObject", lambda: ByteStringObject(b"abc"), note="bytes subclass"))

    # 6) module/attribute resolution, i.e. what a Rust helper pays for
    #    ``py.import(<module>).getattr(<name>)`` on every call.  ``__import__``
    #    reproduces the sys.modules fast path pyo3's ``py.import`` uses.
    def import_and_getattr():
        module = __import__("pypdf._utils", fromlist=["read_non_whitespace"])
        getattr(module, "read_non_whitespace")

    results.append(
        bench(
            "import_module_plus_getattr",
            import_and_getattr,
            iters=100_000,
            note="approximation of py.import()+getattr() per Rust call-site resolution",
        )
    )

    payload = {
        "schema": "rw-microbench-v1",
        "backend": backend,
        "created_local": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "python": sys.version.split()[0],
        "executable": sys.executable,
        "pypdf_file": pypdf.__file__,
        "loadavg": list(__import__("os").getloadavg()),
        "results": results,
    }
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(payload, indent=1), encoding="utf-8")
    for row in results:
        print(f"{row['label']:42s} {row['median_ns_per_op']:12.1f} ns/op")
    return 0


if __name__ == "__main__":
    sys.exit(main())
