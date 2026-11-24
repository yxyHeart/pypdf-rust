"""Instrumentation helpers for the W1/W2 (Reader) and W5/W6/W7 (Writer) diagnostics.

Nothing here touches the library: every hook is a runtime monkeypatch applied by
the diagnostic runner and undone when the run ends.  Three independent kinds of
instrumentation are provided, and they are *never* enabled at the same time, so
that the numbers of one kind are not polluted by the overhead of another:

1. ``StageTimer``  -- inclusive wall time per named stage (wraps a handful of
   Python-level shim methods; ~0.2 us per call of overhead, reported explicitly).
2. ``BoundaryPatcher`` -- counts calls from the pypdf Python shims into the PyO3
   extension (i.e. real Python -> Rust boundary crossings), plus one counter per
   materialised PDF object class (``__new__``/``__init__`` wrappers).
3. ``cProfile`` -- statistical per-function attribution, classified into Rust
   builtin vs Python by the ``{built-in method ...}`` profile entry names.

The counting wrappers add ~0.3-0.5 us per intercepted call, so counting runs are
reported for call counts only; their wall time is recorded but explicitly marked
as perturbed.
"""
from __future__ import annotations

import functools
import io
import pstats
import statistics
import sys
import time
import types

PERF = time.perf_counter_ns

# ---------------------------------------------------------------------------
# statistics
# ---------------------------------------------------------------------------


def percentile(values, q):
    if not values:
        return None
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    pos = (len(ordered) - 1) * q
    lo = int(pos)
    hi = min(lo + 1, len(ordered) - 1)
    if lo == hi:
        return ordered[lo]
    return ordered[lo] + (ordered[hi] - ordered[lo]) * (pos - lo)


def describe(values, digits=6):
    if not values:
        return {"count": 0, "median": None, "p10": None, "p90": None, "min": None, "max": None}
    rnd = lambda v: None if v is None else round(float(v), digits)  # noqa: E731
    return {
        "count": len(values),
        "median": rnd(statistics.median(values)),
        "mean": rnd(statistics.fmean(values)),
        "p10": rnd(percentile(values, 0.10)),
        "p90": rnd(percentile(values, 0.90)),
        "min": rnd(min(values)),
        "max": rnd(max(values)),
    }


# ---------------------------------------------------------------------------
# monkeypatch bookkeeping
# ---------------------------------------------------------------------------


class PatchSet:
    """Applies and reverts attribute patches (module globals / class attrs)."""

    def __init__(self):
        self._undo = []
        self.failures = []

    def set_module_attr(self, module_name, attr, value):
        mod = sys.modules.get(module_name)
        if mod is None:
            self.failures.append(f"module not imported: {module_name}")
            return False
        old = mod.__dict__.get(attr, _MISSING)
        try:
            setattr(mod, attr, value)
        except Exception as exc:  # noqa: BLE001 - patch best effort, recorded
            self.failures.append(f"{module_name}.{attr}: {exc!r}")
            return False
        self._undo.append((mod, attr, old))
        return True

    def set_class_attr(self, cls, attr, value):
        old = cls.__dict__.get(attr, _MISSING)
        try:
            setattr(cls, attr, value)
        except Exception as exc:  # noqa: BLE001
            self.failures.append(f"{cls.__name__}.{attr}: {exc!r}")
            return False
        self._undo.append((cls, attr, old))
        return True

    def restore(self):
        for holder, attr, old in reversed(self._undo):
            if old is _MISSING:
                try:
                    delattr(holder, attr)
                except Exception:  # noqa: BLE001
                    pass
            else:
                try:
                    setattr(holder, attr, old)
                except Exception:  # noqa: BLE001
                    pass
        self._undo.clear()


class _Missing:
    pass


_MISSING = _Missing()


# ---------------------------------------------------------------------------
# 1) stage timing
# ---------------------------------------------------------------------------


class StageTimer:
    """Inclusive wall time + call count per stage name.

    ``begin()`` clears the accumulator; ``table()`` returns the rows for one
    measured repetition.
    """

    def __init__(self):
        self.calls = {}
        self.ns = {}
        self.max_ns = {}

    def begin(self):
        self.calls.clear()
        self.ns.clear()
        self.max_ns.clear()

    def add(self, name, elapsed_ns):
        self.calls[name] = self.calls.get(name, 0) + 1
        self.ns[name] = self.ns.get(name, 0) + elapsed_ns
        if elapsed_ns > self.max_ns.get(name, 0):
            self.max_ns[name] = elapsed_ns

    def wrap(self, name, fn):
        calls = self.calls
        totals = self.ns
        maxima = self.max_ns

        @functools.wraps(fn)
        def wrapper(*args, **kwargs):
            t0 = PERF()
            try:
                return fn(*args, **kwargs)
            finally:
                elapsed = PERF() - t0
                calls[name] = calls.get(name, 0) + 1
                totals[name] = totals.get(name, 0) + elapsed
                if elapsed > maxima.get(name, 0):
                    maxima[name] = elapsed

        return wrapper

    def snapshot(self):
        return {
            name: {
                "calls": self.calls[name],
                "total_ms": self.ns[name] / 1e6,
                "max_ms": self.max_ns[name] / 1e6,
            }
            for name in self.calls
        }

    # -- convenience patchers -------------------------------------------------
    def patch_method(self, patches, cls, attr, stage=None):
        fn = cls.__dict__.get(attr)
        if fn is None or not isinstance(fn, types.FunctionType) and not callable(fn):
            patches.failures.append(f"no attr {cls.__name__}.{attr}")
            return False
        if isinstance(fn, (staticmethod, classmethod)):
            patches.failures.append(f"{cls.__name__}.{attr} is descriptor; skipped")
            return False
        return patches.set_class_attr(cls, attr, self.wrap(stage or attr, fn))

    def patch_module_function(self, patches, module_name, attr, stage=None):
        mod = sys.modules.get(module_name)
        if mod is None:
            patches.failures.append(f"module not imported: {module_name}")
            return False
        fn = mod.__dict__.get(attr)
        if not isinstance(fn, types.FunctionType):
            patches.failures.append(f"{module_name}.{attr} not a python function")
            return False
        return patches.set_module_attr(module_name, attr, self.wrap(stage or attr, fn))

    def patch_property(self, patches, cls, attr, stage=None):
        prop = cls.__dict__.get(attr)
        if not isinstance(prop, property) or prop.fget is None:
            patches.failures.append(f"{cls.__name__}.{attr} is not a readable property")
            return False
        wrapped = property(
            self.wrap(stage or attr, prop.fget), prop.fset, prop.fdel, prop.__doc__
        )
        return patches.set_class_attr(cls, attr, wrapped)


# ---------------------------------------------------------------------------
# 2) Python -> Rust boundary + object materialisation counters
# ---------------------------------------------------------------------------

#: classes whose instantiation is counted as "Python PDF object materialised"
MATERIALIZED_CLASSES = [
    ("pypdf.generic._base", "NullObject"),
    ("pypdf.generic._base", "BooleanObject"),
    ("pypdf.generic._base", "IndirectObject"),
    ("pypdf.generic._base", "FloatObject"),
    ("pypdf.generic._base", "NumberObject"),
    ("pypdf.generic._base", "ByteStringObject"),
    ("pypdf.generic._base", "TextStringObject"),
    ("pypdf.generic._base", "NameObject"),
    ("pypdf.generic._data_structures", "DictionaryObject"),
    ("pypdf.generic._data_structures", "ArrayObject"),
    ("pypdf.generic._data_structures", "StreamObject"),
    ("pypdf.generic._data_structures", "EncodedStreamObject"),
    ("pypdf.generic._data_structures", "DecodedStreamObject"),
    ("pypdf.generic._data_structures", "ContentStream"),
    ("pypdf.generic._rectangle", "RectangleObject"),
    ("pypdf._page", "PageObject"),
    ("pypdf._doc_common", "DocumentInformation"),
]


class BoundaryCounter:
    """Counts calls from pypdf Python shims into the PyO3 extension module.

    Two patch routes are needed because the port uses both import styles:

    * ``from pypdf._pypdf import reader_get_object as _get_object`` binds the
      function into the shim module globals -- the *global* is replaced;
    * ``_writermod.writermod_pdfwriter_append(...)`` resolves the attribute on
      the extension module at call time -- the *extension attribute* is replaced.

    A call is counted exactly once because the two routes reference different
    wrapper objects (the global wrapper calls the original builtin directly).
    """

    def __init__(self):
        self.counts = {}
        self.by_phase = {}
        self._phase = None
        self.patched_globals = 0
        self.patched_ext_attrs = 0
        self.materialize_patched = 0
        self._impl_names = set()

    # -- counting -----------------------------------------------------------
    def _bump(self, key):
        counts = self.counts
        counts[key] = counts.get(key, 0) + 1

    def _phase_bump(self, key):
        phase = self._phase
        if phase is not None:
            slot = self.by_phase.setdefault(phase, {})
            slot[key] = slot.get(key, 0) + 1

    def start_phase(self, name):
        self._phase = name

    def end_phase(self, name=None):
        self._phase = None

    def _make_global_wrapper(self, fn, key):
        counts = self.counts
        phases = self.by_phase

        @functools.wraps(fn)
        def wrapper(*args, **kwargs):
            counts[key] = counts.get(key, 0) + 1
            phase = self._phase
            if phase is not None:
                slot = phases.setdefault(phase, {})
                slot[key] = slot.get(key, 0) + 1
            return fn(*args, **kwargs)

        wrapper.__wrapped__ = fn
        return wrapper

    def _make_ext_wrapper(self, fn, key):
        counts = self.counts
        phases = self.by_phase

        @functools.wraps(fn)
        def wrapper(*args, **kwargs):
            counts[key] = counts.get(key, 0) + 1
            phase = self._phase
            if phase is not None:
                slot = phases.setdefault(phase, {})
                slot[key] = slot.get(key, 0) + 1
            return fn(*args, **kwargs)

        wrapper.__wrapped__ = fn
        return wrapper

    # -- patching -----------------------------------------------------------
    def patch_boundary(self, patches):
        try:
            import pypdf._pypdf as ext
        except ImportError:
            patches.failures.append("pypdf._pypdf not importable (reference backend)")
            return False

        ext_funcs = {}
        for name, value in list(vars(ext).items()):
            if isinstance(value, types.BuiltinFunctionType):
                ext_funcs[id(value)] = name
        self._impl_names = set(ext_funcs.values())

        # route (a): extension module attributes (dynamic ``_writermod.x`` style)
        for name in list(ext_funcs.values()):
            original = getattr(ext, name)
            wrapper = self._make_ext_wrapper(original, name)
            if patches.set_module_attr("pypdf._pypdf", name, wrapper):
                self.patched_ext_attrs += 1

        # route (b): shim module globals bound by ``from pypdf._pypdf import x``
        for mod_name, mod in list(sys.modules.items()):
            if mod is None or mod is ext:
                continue
            if not (mod_name == "pypdf" or mod_name.startswith("pypdf.")):
                continue
            for attr, value in list(vars(mod).items()):
                if isinstance(value, types.BuiltinFunctionType):
                    key = ext_funcs.get(id(value))
                    if key is None:
                        continue
                    wrapper = self._make_global_wrapper(value, key)
                    if patches.set_module_attr(mod_name, attr, wrapper):
                        self.patched_globals += 1
        return True

    def patch_class_method(self, patches, cls, attr, key=None):
        """Count calls of a Python-level method (works on both backends)."""
        raw = cls.__dict__.get(attr)
        if isinstance(raw, (staticmethod, classmethod)):
            fn = raw.__func__
        elif isinstance(raw, types.FunctionType):
            fn = raw
        elif isinstance(raw, property):
            patches.failures.append(f"{cls.__name__}.{attr} is property; use property patcher")
            return False
        else:
            patches.failures.append(f"{cls.__name__}.{attr} not patchable ({type(raw).__name__})")
            return False
        name = key or f"py:{cls.__name__}.{attr}"
        wrapper = self._make_global_wrapper(fn, name)
        if isinstance(raw, staticmethod):
            return patches.set_class_attr(cls, attr, staticmethod(wrapper))
        if isinstance(raw, classmethod):
            return patches.set_class_attr(cls, attr, classmethod(wrapper))
        return patches.set_class_attr(cls, attr, wrapper)

    def patch_module_counted(self, patches, module_name, attr, key=None):
        mod = sys.modules.get(module_name)
        if mod is None:
            patches.failures.append(f"module not imported: {module_name}")
            return False
        fn = mod.__dict__.get(attr)
        if not isinstance(fn, types.FunctionType):
            patches.failures.append(f"{module_name}.{attr} not a python function")
            return False
        return patches.set_module_attr(
            module_name, attr, self._make_global_wrapper(fn, key or f"py:{module_name}.{attr}")
        )

    def patch_property_counted(self, patches, cls, attr, key=None):
        prop = cls.__dict__.get(attr)
        if not isinstance(prop, property) or prop.fget is None:
            patches.failures.append(f"{cls.__name__}.{attr} is not a readable property")
            return False
        wrapped = property(
            self._make_global_wrapper(prop.fget, key or f"py:{cls.__name__}.{attr}"),
            prop.fset,
            prop.fdel,
            prop.__doc__,
        )
        return patches.set_class_attr(cls, attr, wrapped)

    def patch_materialization(self, patches):
        for module_name, class_name in MATERIALIZED_CLASSES:
            mod = sys.modules.get(module_name)
            if mod is None:
                patches.failures.append(f"materialize: module missing {module_name}")
                continue
            cls = getattr(mod, class_name, None)
            if cls is None:
                patches.failures.append(f"materialize: class missing {module_name}.{class_name}")
                continue
            key = f"new:{class_name}"
            if self._patch_new(patches, cls, key):
                self.materialize_patched += 1

    def _patch_new(self, patches, cls, key):
        """Intercept instance creation for ``cls`` without changing semantics."""
        own = cls.__dict__.get("__new__")
        counts = self.counts
        phases = self.by_phase

        def bump():
            counts[key] = counts.get(key, 0) + 1
            phase = self._phase
            if phase is not None:
                slot = phases.setdefault(phase, {})
                slot[key] = slot.get(key, 0) + 1

        if isinstance(own, staticmethod):
            fn = own.__func__
            if getattr(fn, "__code__", None) is not None:  # python __new__
                @functools.wraps(fn)
                def wrapper(sub_cls, *args, **kwargs):
                    if sub_cls is cls:
                        bump()
                    return fn(sub_cls, *args, **kwargs)

                return patches.set_class_attr(cls, "__new__", staticmethod(wrapper))
            return False  # C-level __new__ already handled below

        # no Python __new__: pick the correct C base allocator
        if issubclass(cls, dict):
            base_new = dict.__new__
        elif issubclass(cls, list):
            base_new = list.__new__
        elif issubclass(cls, str):
            base_new = str.__new__
        elif issubclass(cls, bytes):
            base_new = bytes.__new__
        elif issubclass(cls, int):
            base_new = int.__new__
        elif issubclass(cls, float):
            base_new = float.__new__
        else:
            base_new = None

        if base_new is None:
            @functools.wraps(object.__new__)
            def wrapper(sub_cls, *args, **kwargs):
                if sub_cls is cls:
                    bump()
                return object.__new__(sub_cls)

            return patches.set_class_attr(cls, "__new__", staticmethod(wrapper))

        @functools.wraps(base_new)
        def wrapper(sub_cls, *args, **kwargs):
            if sub_cls is cls:
                bump()
            return base_new(sub_cls, *args, **kwargs)

        return patches.set_class_attr(cls, "__new__", staticmethod(wrapper))

    # -- reporting ----------------------------------------------------------
    def top(self, n=30):
        return sorted(self.counts.items(), key=lambda kv: -kv[1])[:n]

    def total_boundary(self):
        return sum(v for k, v in self.counts.items() if not k.startswith("new:"))

    def total_materialized(self):
        return sum(v for k, v in self.counts.items() if k.startswith("new:"))

    def to_json(self):
        return {
            "total_boundary_calls": self.total_boundary(),
            "total_materialized": self.total_materialized(),
            "patched_globals": self.patched_globals,
            "patched_ext_attrs": self.patched_ext_attrs,
            "materialize_patched": self.materialize_patched,
            "counts": dict(sorted(self.counts.items(), key=lambda kv: -kv[1])),
            "by_phase": {
                phase: dict(sorted(slot.items(), key=lambda kv: -kv[1]))
                for phase, slot in self.by_phase.items()
            },
        }


# ---------------------------------------------------------------------------
# 3) cProfile attribution
# ---------------------------------------------------------------------------


def profile_callable(fn, repeats=3):
    """Profile ``fn`` (called ``repeats`` times) and merge the per-call stats."""
    import cProfile

    profiler = cProfile.Profile()
    profiler.enable()
    for _ in range(repeats):
        fn()
    profiler.disable()
    stats = pstats.Stats(profiler)
    return stats


def profile_rows(stats, limit=None):
    """Return per-function rows with cumulative + self time, classified.

    Classification rule (documented in diag_*.md):
    * ``rust_builtin`` -- the profile entry is a C function of the extension
      module (``{built-in method pypdf._pypdf.<name>}``): the Python callback
      into Rust;
    * ``python_shim`` -- a Python frame of the pypdf package (the one-line
      delegation shims and the helper wrappers);
    * ``other_builtin`` / ``other_python`` -- interpreter/library overhead.
    """
    rows = []
    for (filename, lineno, funcname), (cc, nc, tt, ct, callers) in stats.stats.items():
        if filename == "~" or (isinstance(filename, str) and filename.startswith("{")) or "<built-in" in str(funcname):
            kind = "rust_builtin" if "pypdf._pypdf" in str(funcname) else "other_builtin"
            label = str(funcname)
        else:
            kind = "python_shim" if "pypdf" in str(filename) else "other_python"
            label = f"{funcname} ({_short_path(filename)}:{lineno})"
        rows.append(
            {
                "label": label,
                "kind": kind,
                "ncalls": nc,
                "tottime_s": tt,
                "cumtime_s": ct,
            }
        )
    rows.sort(key=lambda row: -row["tottime_s"])
    return rows[:limit] if limit else rows


def _short_path(path):
    text = str(path)
    marker = "/python/pypdf/"
    if marker in text:
        return "pypdf/" + text.split(marker, 1)[1]
    marker = "site-packages/pypdf/"
    if marker in text:
        return "pypdf/" + text.split(marker, 1)[1]
    return text.rsplit("/", 1)[-1]


def classify_totals(rows):
    totals = {}
    for row in rows:
        totals[row["kind"]] = totals.get(row["kind"], 0.0) + row["tottime_s"]
    return totals


def format_profile_table(rows, stats, top=25):
    total_calls = stats.total_calls
    total_tt = stats.total_tt
    lines = [
        "| # | site | kind | ncalls | tottime (ms) | tottime % | cumtime (ms) |",
        "|---|---|---|---:|---:|---:|---:|",
    ]
    for index, row in enumerate(rows[:top], start=1):
        pct = (row["tottime_s"] / total_tt * 100.0) if total_tt else 0.0
        lines.append(
            f"| {index} | `{row['label']}` | {row['kind']} | {row['ncalls']} | "
            f"{row['tottime_s'] * 1e3:.2f} | {pct:.1f}% | {row['cumtime_s'] * 1e3:.2f} |"
        )
    lines.append(f"| | **total** | | {total_calls} | {total_tt * 1e3:.2f} | 100.0% | |")
    return "\n".join(lines)


def stats_summary(stats):
    return {
        "total_calls": stats.total_calls,
        "primitive_calls": stats.prim_calls,
        "total_tt_s": stats.total_tt,
    }


def print_stats_head(stats, n=3):
    buffer = io.StringIO()
    pstats.Stats(stats, stream=buffer).sort_stats("cumtime").print_stats(n)
    return buffer.getvalue()
