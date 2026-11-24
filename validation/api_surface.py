#!/usr/bin/env python3
"""§4.1/§4.2: capture the public Python API surface for differential comparison.

Dumps a canonical JSON describing, for every reachable public module under
`pypdf`: classes (bases, MRO of local names, methods/properties, signatures),
functions, constants and their reprs.  Run in the reference venv and in the
Rust venv; `compare_api.py` diffs the two JSON files.
"""
from __future__ import annotations

import importlib
import inspect
import json
import pkgutil
import sys
import typing
from pathlib import Path

import pypdf

SKIP_MODULES = {"pypdf._pypdf"}


def local_mro(cls) -> list[str]:
    return [f"{c.__module__}.{c.__qualname__}" for c in cls.__mro__]


def sig_of(obj) -> str | None:
    try:
        return str(inspect.signature(obj))
    except (ValueError, TypeError):
        return None


def describe_callable(obj) -> dict:
    return {
        "kind": "callable",
        "signature": sig_of(obj),
        "is_async": inspect.iscoroutinefunction(obj),
        "has_doc": bool(inspect.getdoc(obj)),
        "module": getattr(obj, "__module__", None),
    }


def walk_module(mod_name: str, seen: set[str]) -> dict:
    if mod_name in seen:
        return {}
    seen.add(mod_name)
    mod = importlib.import_module(mod_name)
    out: dict = {}
    for name, obj in sorted(vars(mod).items(), key=lambda kv: kv[0]):
        if name.startswith("__"):
            continue
        qual = f"{mod_name}.{name}"
        if inspect.ismodule(obj):
            if obj.__name__.startswith("pypdf.") and obj.__name__ not in SKIP_MODULES:
                out[qual] = {"kind": "module"}
            continue
        if inspect.isclass(obj):
            entry = {
                "kind": "class",
                "mro": local_mro(obj)[:6],
                "abstract": bool(getattr(obj, "__abstractmethods__", None)),
                "has_doc": bool(inspect.getdoc(obj)),
                "members": {},
            }
            for mname, mobj in sorted(vars(obj).items(), key=lambda kv: kv[0]):
                if mname.startswith("__") and mname not in ("__init__", "__call__", "__iter__",
                                                            "__getitem__", "__setitem__",
                                                            "__eq__", "__hash__", "__add__",
                                                            "__str__", "__repr__"):
                    continue
                if inspect.isfunction(mobj) or isinstance(mobj, (staticmethod, classmethod)):
                    raw = mobj.__func__ if isinstance(mobj, (staticmethod, classmethod)) else mobj
                    entry["members"][mname] = {
                        "kind": "staticmethod" if isinstance(mobj, staticmethod) else
                                "classmethod" if isinstance(mobj, classmethod) else "method",
                        "signature": sig_of(raw),
                        "has_doc": bool(inspect.getdoc(raw)),
                    }
                elif isinstance(mobj, property):
                    entry["members"][mname] = {
                        "kind": "property",
                        "has_setter": mobj.fset is not None,
                        "has_deleter": mobj.fdel is not None,
                        "doc": bool(inspect.getdoc(mobj)),
                    }
                elif inspect.ismethoddescriptor(mobj):
                    entry["members"][mname] = {
                        "kind": "methoddescriptor",
                        "signature": sig_of(mobj),
                        "has_doc": bool(inspect.getdoc(mobj)),
                    }
                elif inspect.isdatadescriptor(mobj):
                    entry["members"][mname] = {"kind": "datadescriptor"}
            out[qual] = entry
            continue
        if callable(obj) and not isinstance(obj, type):
            out[qual] = describe_callable(obj)
            continue
        # constants / data values
        try:
            rep = repr(obj)
        except Exception:
            rep = "<unrepr>"
        out[qual] = {"kind": "data", "type": type(obj).__name__,
                     "repr": rep[:1500]}
    return out


def main() -> int:
    seen: set[str] = set()
    surface: dict[str, dict] = {}
    mods: list[str] = []
    for info in pkgutil.walk_packages(pypdf.__path__, prefix="pypdf."):
        mods.append(info.name)
    mods = [m for m in mods if m not in SKIP_MODULES]
    for mod_name in sorted(mods):
        try:
            surface.update(walk_module(mod_name, seen))
        except Exception as exc:  # noqa: BLE001 - record, never crash
            surface[f"<import-error:{mod_name}>"] = {"kind": "error", "error": repr(exc)}
    surface["pypdf"] = walk_module("pypdf", set())
    out = Path(sys.argv[1])
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(surface, indent=1, sort_keys=True, ensure_ascii=False), encoding="utf-8")
    print(f"api surface: {len(surface)} entries, {len(mods)} modules -> {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
