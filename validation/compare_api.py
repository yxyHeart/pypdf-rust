#!/usr/bin/env python3
"""§4.1/§4.2: compare two API-surface JSON files and classify differences."""
from __future__ import annotations

import ast
import json
import re
import sys
from pathlib import Path

ADDR = re.compile(r"0x[0-9a-fA-F]+")
CLASS_RE = re.compile(r"<class '([^']+)'>")


def sorted_set_repr(text: str) -> str:
    """Make frozenset/set reprs order-insensitive (CPython salts class hashes)."""
    match = re.fullmatch(r"(frozenset|set)\(\{(.*)\}\)", text, re.S)
    if not match:
        return text
    items = [item.strip() for item in match.group(2).split(",") if item.strip()]
    return f"{match.group(1)}({{" + ", ".join(sorted(items)) + "})"


def norm(value):
    """Normalize process-specific artifacts (object addresses) out of the surface."""
    if isinstance(value, str):
        return sorted_set_repr(ADDR.sub("0xADDR", value))
    if isinstance(value, list):
        return [norm(v) for v in value]
    return value

FIELDS = ("kind", "signature", "mro", "abstract", "has_setter", "has_deleter", "type", "repr")


def leaf_name(key: str) -> str:
    return key.rsplit(".", 1)[-1]


def sig_structure(text: str | None):
    """Return (params[{name,kind,default}], return_text) or None if unparsable."""
    if not text:
        return None
    try:
        fn = ast.parse(f"def _f{text}: pass").body[0]
    except SyntaxError:
        return None
    args = fn.args
    pos = args.posonlyargs + args.args
    defaults = [None] * (len(pos) - len(args.defaults)) + [ast.unparse(d) for d in args.defaults]
    out = []
    for index, arg in enumerate(pos):
        out.append((arg.arg, "posonly" if index < len(args.posonlyargs) else "pos", defaults[index]))
    if args.vararg:
        out.append((args.vararg.arg, "vararg", None))
    for arg, default in zip(args.kwonlyargs, args.kw_defaults):
        out.append((arg.arg, "kwonly", None if default is None else ast.unparse(default)))
    if args.kwarg:
        out.append((args.kwarg.arg, "kwarg", None))
    return out, (ast.unparse(fn.returns) if fn.returns else None)


def annotation_only(sig_a: str | None, sig_b: str | None) -> bool:
    """True when two signatures differ only in type annotations."""
    if sig_a == sig_b:
        return False
    sa, sb = sig_structure(sig_a), sig_structure(sig_b)
    if sa is None or sb is None:
        return False
    return sa[0] == sb[0]


def main() -> int:
    ref = json.loads(Path(sys.argv[1]).read_text())
    rust = json.loads(Path(sys.argv[2]).read_text())
    # Ignore the generated annotation helper module (private, pypdf-rust only).
    rust = {k: v for k, v in rust.items() if not k.startswith("pypdf._api_annotations")}
    missing = sorted(set(ref) - set(rust))          # public API not present in the port
    extra = sorted(set(rust) - set(ref))            # names only the port has
    changed = []
    annotations = []

    METHOD_KINDS = {"method", "methoddescriptor", "staticmethod", "classmethod"}

    def record(obj_key: str, field: str, a, b):
        if norm(a) == norm(b):
            return
        if field == "kind" and {a, b} <= METHOD_KINDS:
            return  # Rust pyclass methods vs Python methods: implementation detail
        if field == "signature" and annotation_only(norm(a), norm(b)):
            annotations.append((obj_key, field, a, b))
        else:
            changed.append((obj_key, field, a, b))

    for key in sorted(set(ref) & set(rust)):
        a, b = ref[key], rust[key]
        for field in FIELDS:
            record(key, field, a.get(field), b.get(field))
        if a.get("kind") == "class":
            am, bm = a.get("members", {}), b.get("members", {})
            for member in sorted(set(am) - set(bm)):
                if not member.startswith("_"):
                    changed.append((f"{key}.{member}", "missing-member", "present", "absent"))
            for member in sorted(set(am) & set(bm)):
                for field in ("kind", "signature", "has_setter", "has_deleter"):
                    record(f"{key}.{member}", field,
                           am[member].get(field), bm[member].get(field))

    private_extra = [k for k in extra if leaf_name(k).startswith("_")]
    public_extra = [k for k in extra if not leaf_name(k).startswith("_")]

    print(f"reference entries: {len(ref)}")
    print(f"port entries:      {len(rust)}")
    print(f"MISSING in port:   {len(missing)}")
    for key in missing:
        print(f"  - {key}  ({ref[key].get('kind')})")
    print(f"EXTRA in port:     {len(extra)}  (private={len(private_extra)}, public={len(public_extra)})")
    for key in public_extra:
        print(f"  + {key}  ({rust[key].get('kind')})")
    print(f"CHANGED (structural): {len(changed)}")
    print(f"ANNOTATION-ONLY:     {len(annotations)}")
    for key, field, a, b in changed[:60]:
        print(f"  ~ {key} [{field}]: {a!r} != {b!r}")
    if len(changed) > 60:
        print(f"  ... and {len(changed) - 60} more")
    for key, field, a, b in annotations[:10]:
        print(f"  ~ann {key} [{field}]: {str(a)[:70]!r} != {str(b)[:70]!r}")

    blocking = len(missing) + len(changed) + len(public_extra)
    status = "PASS"
    if blocking:
        status = "FAIL"
    elif annotations:
        status = "PASS_WITH_DOCUMENTED_VARIANCE"
    print(f"\nRESULT: {status} (missing={len(missing)}, structural={len(changed)}, "
          f"public_extra={len(public_extra)}, annotation_only={len(annotations)}, "
          f"private_extra={len(private_extra)})")
    return 0 if blocking == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
