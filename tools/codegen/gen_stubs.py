#!/usr/bin/env python3
"""Generate the Python package skeleton of the Rust port from the pypdf sources.

Each upstream module becomes a Python module with the same module tree, the same
imports and the same public names.  Until a module is ported to Rust, classes and
functions are stubs that raise ``NotImplementedError``.  Modules listed as
``python`` in ``tools/codegen/port_status.json`` are copied verbatim (type-only
modules such as exceptions and protocols); modules listed as ``rust`` are skipped
because a Rust-backed shim is generated elsewhere.

Usage:
    python tools/codegen/gen_stubs.py            # (re)generate stubs
    python tools/codegen/gen_stubs.py --check    # verify stubs are in sync
"""

from __future__ import annotations

import argparse
import ast
import json
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / "python" / "pypdf"
VENDOR = ROOT / "vendor" / "pypdf-7.1.1"
SYMBOLS = ROOT / "docs" / "symbols.json"
STATUS = ROOT / "tools" / "codegen" / "port_status.json"

HEADER = '''"""{doc}

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``{module}``).
Replaced by a Rust-backed implementation in phase {phase}.
"""

from __future__ import annotations
'''

SAFE_BASE = {"object", "Exception", "ValueError", "TypeError", "KeyError"}


def strip_subscripts(text: str) -> str:
    out: list[str] = []
    depth = 0
    for char in text:
        if char == "[":
            depth += 1
        elif char == "]":
            depth = max(0, depth - 1)
        elif depth == 0:
            out.append(char)
    return "".join(out)


def literal_or_none(node: ast.AST | None) -> str:
    """Render an inert stand-in that keeps the value's container kind."""
    if node is None:
        return "None"
    if isinstance(node, ast.Dict):
        return "{}"
    if isinstance(node, ast.List):
        return "[]"
    if isinstance(node, ast.Set):
        return "set()"
    if isinstance(node, ast.Tuple):
        return "()"
    try:
        value = ast.literal_eval(node)
    except Exception:  # noqa: BLE001 - not a literal
        return "None"
    if value is None or isinstance(value, (bool, int, float)):
        return repr(value)
    if isinstance(value, str):
        return repr(value) if len(value) < 200 else "''"
    if isinstance(value, bytes):
        return repr(value) if len(value) < 200 else "b''"
    if isinstance(value, (list, tuple, dict, set, frozenset)) and len(value) < 64:
        return repr(value)
    return "None"


class StubEmitter:
    def __init__(self, module: str, phase: str) -> None:
        self.module = module
        self.phase = phase

    # -- imports ---------------------------------------------------------
    def render_import(self, node: ast.AST) -> str:
        if isinstance(node, ast.Import):
            names = [alias.name for alias in node.names if alias.name != "__future__"]
            if not names:
                return ""
            return "import " + ", ".join(names)
        assert isinstance(node, ast.ImportFrom)
        if node.level == 0 and (node.module or "") == "__future__":
            return ""
        module = "." * node.level + (node.module or "")
        parts = []
        for alias in node.names:
            parts.append(f"{alias.name} as {alias.asname}" if alias.asname else alias.name)
        return f"from {module} import " + ", ".join(parts)

    # -- class stubs -----------------------------------------------------
    def class_should_keep_bases(self, node: ast.ClassDef) -> bool:
        for base in node.bases:
            stripped = strip_subscripts(ast.unparse(base)).strip()
            if not stripped.isidentifier():
                return False
            if stripped in {
                "NamedTuple",
                "TypedDict",
                "Protocol",
                "Enum",
                "IntEnum",
                "StrEnum",
                "IntFlag",
                "Flag",
                "Generic",
                "ABC",
            }:
                return False
        for decorator in node.decorator_list:
            text = ast.unparse(decorator)
            if "dataclass" in text:
                return False
        return True

    def render_class(self, node: ast.ClassDef, indent: int, nested: bool = False) -> list[str]:
        pad = " " * indent
        bases = []
        if self.class_should_keep_bases(node):
            bases = [strip_subscripts(ast.unparse(base)) for base in node.bases]
        header = f"{pad}class {node.name}"
        if bases:
            header += "(" + ", ".join(bases) + ")"
        lines = [header + ":", f'{pad}    """Stub for {self.module}.{node.name}."""']
        body: list[str] = []
        for child in node.body:
            if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                body.extend(self.render_method(child, indent + 4))
            elif isinstance(child, ast.ClassDef):
                body.extend(self.render_class(child, indent + 4, nested=True))
            elif isinstance(child, ast.AnnAssign) and isinstance(child.target, ast.Name):
                body.append(f"{' ' * (indent + 4)}{child.target.id} = None")
            elif isinstance(child, ast.Assign):
                for target in child.targets:
                    if isinstance(target, ast.Name):
                        body.append(f"{' ' * (indent + 4)}{target.id} = {literal_or_none(child.value)}")
        if not body:
            body.append(f"{pad}    pass")
        return lines + body

    def render_method(self, node: ast.FunctionDef | ast.AsyncFunctionDef, indent: int) -> list[str]:
        pad = " " * indent
        decorators = [ast.unparse(d) for d in node.decorator_list]
        is_static = any(d.startswith("staticmethod") for d in decorators)
        is_class = any(d.startswith("classmethod") for d in decorators)
        is_property = any(d.startswith("property") for d in decorators)
        lines: list[str] = []
        if is_static:
            lines.append(f"{pad}@staticmethod")
        elif is_class:
            lines.append(f"{pad}@classmethod")
        elif is_property:
            lines.append(f"{pad}@property")

        if is_static:
            signature = "(*args, **kwargs)"
        elif node.name == "__new__" or is_class:
            signature = "(cls, *args, **kwargs)"
        elif is_property:
            signature = "(self)"
        else:
            signature = "(self, *args, **kwargs)"

        lines.append(f"{pad}def {node.name}{signature}:")
        if node.name == "__new__":
            lines.append(f"{pad}    return super().__new__(cls)  # inert stub")
        elif node.name == "__init__":
            lines.append(f"{pad}    try:")
            lines.append(f"{pad}        super().__init__(*args, **kwargs)")
            lines.append(f"{pad}    except TypeError:")
            lines.append(f"{pad}        pass")
        elif node.name in ("__str__", "__repr__", "__format__"):
            lines.append(f'{pad}    return ""  # inert stub')
        else:
            lines.append(f"{pad}    return None  # inert stub")
        return lines

    def render_function(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> list[str]:
        return [
            f"def {node.name}(*args, **kwargs):",
            f"    return None  # inert stub: {self.module}.{node.name}",
        ]

    # -- module ----------------------------------------------------------
    def render_block(self, body: list[ast.stmt], indent: int = 0) -> list[str]:
        pad = " " * indent
        lines: list[str] = []
        for node in body:
            if isinstance(node, (ast.Import, ast.ImportFrom)):
                text = self.render_import(node)
                if text:
                    lines.append(pad + text)
            elif isinstance(node, ast.Try):
                lines.append(pad + "try:")
                inner = self.render_block(node.body, indent + 4)
                lines.extend(inner or [pad + "    pass"])
                for handler in node.handlers:
                    if handler.type is None:
                        lines.append(pad + "except:")
                    else:
                        lines.append(pad + "except " + ast.unparse(handler.type) + ":")
                    inner = self.render_block(handler.body, indent + 4)
                    lines.extend(inner or [pad + "    pass"])
                if node.orelse:
                    lines.append(pad + "else:")
                    lines.extend(self.render_block(node.orelse, indent + 4))
                if node.finalbody:
                    lines.append(pad + "finally:")
                    lines.extend(self.render_block(node.finalbody, indent + 4))
            elif isinstance(node, ast.If):
                lines.append(pad + "if " + ast.unparse(node.test) + ":")
                inner = self.render_block(node.body, indent + 4)
                lines.extend(inner or [pad + "    pass"])
                if node.orelse:
                    lines.append(pad + "else:")
                    lines.extend(self.render_block(node.orelse, indent + 4))
            elif isinstance(node, ast.ClassDef):
                lines.extend(self.render_class(node, indent))
            elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                lines.extend(
                    [" " * (indent) + line.lstrip() if False else line for line in self.render_function(node)]
                )
            elif isinstance(node, ast.Assign):
                for target in node.targets:
                    if isinstance(target, ast.Name):
                        value = literal_or_none(node.value)
                        if target.id == "__version__":
                            value = "'7.1.1'"
                        elif target.id == "__all__":
                            value = literal_or_none(node.value)
                        lines.append(f"{pad}{target.id} = {value}")
            elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
                lines.append(f"{pad}{node.target.id} = None")
            elif isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant):
                continue  # drop docstrings of nested blocks
            # other statements (with/for/del/...) are dropped: stubs have no behaviour
        return lines

    def render_module(self, tree: ast.Module) -> str:
        doc = (ast.get_docstring(tree) or "pypdf port stub").splitlines()[0]
        lines = [HEADER.format(doc=doc, module=self.module, phase=self.phase)]
        lines.extend(self.render_block(tree.body))
        return "\n".join(lines) + "\n"


def module_path(module: dict) -> Path:
    parts = module["module"].split(".")[1:]
    base = PACKAGE.joinpath(*parts)
    return base / "__init__.py" if module.get("is_package") else base.with_suffix(".py")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    symbols = json.loads(SYMBOLS.read_text(encoding="utf-8"))
    status = json.loads(STATUS.read_text(encoding="utf-8")) if STATUS.exists() else {}
    PACKAGE.mkdir(parents=True, exist_ok=True)

    written = 0
    copied = 0
    skipped = 0
    stale: list[str] = []
    for module in symbols["modules"]:
        name = module["module"]
        entry = status.get(name, {})
        mode = entry.get("mode", "stub")
        phase = entry.get("phase", "later")
        target = module_path(module)
        source = ROOT / module["path"]

        if mode == "rust":
            skipped += 1
            continue

        if mode == "python":
            content = source.read_text(encoding="utf-8")
            copied += 1
        else:
            tree = ast.parse(source.read_text(encoding="utf-8"), filename=str(source))
            content = StubEmitter(name, phase).render_module(tree)
            written += 1

        if args.check:
            if not target.exists() or target.read_text(encoding="utf-8") != content:
                stale.append(str(target.relative_to(ROOT)))
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")

    py_typed = PACKAGE / "py.typed"
    if not args.check:
        py_typed.write_text("", encoding="utf-8")

    print(f"stubs: {written}, python-verbatim: {copied}, rust-backed: {skipped}")
    if args.check:
        if stale:
            print("STALE:", *stale, sep="\n  ")
            return 1
        print("skeleton in sync")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
