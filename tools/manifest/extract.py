#!/usr/bin/env python3
"""Extract the complete pypdf 7.1.1 symbol surface from the vendored source.

Gate A6 ("no part left unported") is enforced against this inventory: every
module, class, method, property, module-level function and module-level constant
must be mapped to a Rust implementation path.

Usage:
    python tools/manifest/extract.py -o docs/symbols.json
"""

from __future__ import annotations

import argparse
import ast
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VENDOR = ROOT / "vendor" / "pypdf-7.1.1" / "pypdf"


def module_name(path: Path) -> str:
    rel = path.relative_to(VENDOR).with_suffix("")
    parts = list(rel.parts)
    if parts[-1] == "__init__":
        parts = parts[:-1]
    return ".".join(["pypdf", *parts]) if parts else "pypdf"


def annotation(node: ast.AST | None) -> str | None:
    if node is None:
        return None
    try:
        return ast.unparse(node)
    except Exception:  # noqa: BLE001 - old syntax edge cases
        return None


def function_info(node: ast.FunctionDef | ast.AsyncFunctionDef) -> dict:
    args = node.args
    params: list[dict] = []
    positional = list(args.posonlyargs) + list(args.args)
    defaults = [None] * (len(positional) - len(args.defaults)) + list(args.defaults)
    for argument, default in zip(positional, defaults, strict=False):
        params.append(
            {
                "name": argument.arg,
                "annotation": annotation(argument.annotation),
                "default": annotation(default),
                "kind": "posonly" if argument in args.posonlyargs else "positional",
            }
        )
    if args.vararg:
        params.append({"name": f"*{args.vararg.arg}", "kind": "vararg"})
    for argument, default in zip(args.kwonlyargs, args.kw_defaults, strict=False):
        params.append(
            {
                "name": argument.arg,
                "annotation": annotation(argument.annotation),
                "default": annotation(default),
                "kind": "kwonly",
            }
        )
    if args.kwarg:
        params.append({"name": f"**{args.kwarg.arg}", "kind": "kwarg"})
    return {
        "kind": "async-function"
        if isinstance(node, ast.AsyncFunctionDef)
        else "function",
        "name": node.name,
        "lineno": node.lineno,
        "signature": params,
        "returns": annotation(node.returns),
        "decorators": [annotation(d) for d in node.decorator_list],
        "is_staticmethod": any(_is_name(d, "staticmethod") for d in node.decorator_list),
        "is_classmethod": any(
            _is_name(d, "classmethod") for d in node.decorator_list
        ),
        "is_property": any(_is_name(d, "property") for d in node.decorator_list),
        "docstring": (ast.get_docstring(node) or "").splitlines()[:1] or [],
    }


def _is_name(node: ast.AST, target: str) -> bool:
    if isinstance(node, ast.Name):
        return node.id == target
    if isinstance(node, ast.Attribute):
        return node.attr == target
    if isinstance(node, ast.Call):
        return _is_name(node.func, target)
    return False


def class_info(node: ast.ClassDef) -> dict:
    bases = [annotation(b) or "" for b in node.bases]
    members: list[dict] = []
    for child in node.body:
        if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
            members.append(function_info(child))
        elif isinstance(child, ast.AnnAssign) and isinstance(child.target, ast.Name):
            members.append(
                {
                    "kind": "annotation",
                    "name": child.target.id,
                    "annotation": annotation(child.annotation),
                    "lineno": child.lineno,
                }
            )
        elif isinstance(child, ast.Assign):
            for target in child.targets:
                if isinstance(target, ast.Name):
                    members.append(
                        {
                            "kind": "class-attribute",
                            "name": target.id,
                            "lineno": child.lineno,
                        }
                    )
    return {
        "kind": "class",
        "name": node.name,
        "lineno": node.lineno,
        "bases": bases,
        "decorators": [annotation(d) for d in node.decorator_list],
        "docstring": (ast.get_docstring(node) or "").splitlines()[:1] or [],
        "members": members,
    }


def _resolve_import(module: str, node: ast.ImportFrom, is_package: bool) -> str:
    """Resolve a relative import to an absolute module name."""
    if node.level == 0:
        return node.module or ""
    parts = module.split(".")
    if len(parts) < node.level:
        return node.module or ""
    base = parts[: len(parts) - node.level + (1 if is_package else 0)]
    return ".".join([*base, node.module] if node.module else base)


def _iter_module_nodes(body):
    """Yield nodes at module nesting level (inside try/if blocks, not functions)."""
    for node in body:
        yield node
        if isinstance(node, (ast.Try, ast.If, ast.With, ast.AsyncWith, ast.For, ast.While)):
            for field in ("body", "orelse", "finalbody"):
                yield from _iter_module_nodes(getattr(node, field, []) or [])
            for handler in getattr(node, "handlers", []) or []:
                yield from _iter_module_nodes(handler.body)


def _internal_imports(tree: ast.Module, module: str, is_package: bool) -> list[dict]:
    """Module-level imports that reference other pypdf modules (for stub generation)."""
    imports: list[dict] = []
    for node in _iter_module_nodes(tree.body):
        if isinstance(node, ast.Import):
            names = [alias.name for alias in node.names if alias.name != "__future__"]
            if names:
                imports.append(
                    {"kind": "import", "modules": names, "lineno": node.lineno}
                )
        elif isinstance(node, ast.ImportFrom):
            target = _resolve_import(module, node, is_package)
            if target == "__future__":
                continue
            names = [alias.name for alias in node.names]
            imports.append(
                {
                    "kind": "from",
                    "module": target,
                    "names": names,
                    "aliases": [
                        [alias.name, alias.asname] for alias in node.names if alias.asname
                    ],
                    "lineno": node.lineno,
                }
            )
    return imports


def module_info(path: Path) -> dict:
    is_package = path.name == "__init__.py"
    source = path.read_text(encoding="utf-8")
    tree = ast.parse(source, filename=str(path))
    members: list[dict] = []
    for node in _iter_module_nodes(tree.body):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            members.append(function_info(node))
        elif isinstance(node, ast.ClassDef):
            members.append(class_info(node))
        elif isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    members.append(
                        {
                            "kind": "constant",
                            "name": target.id,
                            "lineno": node.lineno,
                            "value_repr": annotation(node.value),
                        }
                    )
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            members.append(
                {
                    "kind": "annotation",
                    "name": node.target.id,
                    "annotation": annotation(node.annotation),
                    "lineno": node.lineno,
                }
            )
    exports: list[str] = []
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(
            isinstance(t, ast.Name) and t.id == "__all__" for t in node.targets
        ):
            try:
                exports = [str(value.value) for value in node.value.elts]  # type: ignore[attr-defined]
            except Exception:  # noqa: BLE001
                exports = []
    name = module_name(path)
    return {
        "module": name,
        "path": str(path.relative_to(ROOT)),
        "is_package": is_package,
        "imports": _internal_imports(tree, name, is_package),
        "docstring": (ast.get_docstring(tree) or "").splitlines()[:1] or [],
        "exports": exports,
        "members": members,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("-o", "--out", type=Path, default=ROOT / "docs" / "symbols.json")
    args = parser.parse_args()

    modules = [module_info(path) for path in sorted(VENDOR.rglob("*.py"))]
    classes = 0
    functions = 0
    methods = 0
    constants = 0
    for module in modules:
        for member in module["members"]:
            if member["kind"] == "class":
                classes += 1
                for child in member["members"]:
                    if child["kind"] in ("function", "async-function"):
                        methods += 1
                    else:
                        constants += 1
            elif member["kind"] in ("function", "async-function"):
                functions += 1
            else:
                constants += 1

    payload = {
        "upstream": "pypdf 7.1.1",
        "counts": {
            "modules": len(modules),
            "classes": classes,
            "class_methods": methods,
            "module_functions": functions,
            "constants_and_annotations": constants,
        },
        "modules": modules,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=1), encoding="utf-8")
    print(json.dumps(payload["counts"], indent=1))
    print(f"written {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
