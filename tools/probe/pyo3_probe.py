#!/usr/bin/env python3
"""P0 probe: verify the PyO3 object-model assumptions the port depends on.

Run after building/installing ``pyo3_probe`` (see scripts/probe_pyo3.sh).
"""

from __future__ import annotations

import copy
import sys


def main() -> int:
    import pyo3_probe as probe

    failures: list[str] = []

    def check(name: str, condition: bool, detail: str = "") -> None:
        status = "ok" if condition else "FAIL"
        print(f"[{status}] {name}{(' - ' + detail) if detail else ''}")
        if not condition:
            failures.append(name)

    d = probe.DictObj({"/Type": "/Page", "/Count": 3})
    check("DictObj is dict", isinstance(d, dict), f"type={type(d).__mro__}")
    check("dict methods", d["/Count"] == 3 and list(d.keys()) == ["/Type", "/Count"])
    check("dict equality", d == {"/Type": "/Page", "/Count": 3})
    d["/MediaBox"] = [0, 0, 200, 200]
    check("dict mutation", d["/MediaBox"] == [0, 0, 200, 200])
    check("describe method", d.describe() == "DictObj")

    # dynamic attributes (pypdf hangs `indirect_reference` / `pdf` on objects)
    d.arbitrary_attr = 42
    check("arbitrary attribute", d.arbitrary_attr == 42)
    d.indirect_reference = 7
    check("declared attribute", d.indirect_reference == 7)
    check("vars()", "arbitrary_attr" in vars(d))

    # Python-level subclassing (tests do `class MyPage(PageObject)`)
    class MyDict(probe.DictObj):
        def __init__(self) -> None:
            super().__init__({"/Custom": True})

        def describe(self) -> str:
            return "MyDict:" + super().describe()

    m = MyDict()
    check("subclass isinstance", isinstance(m, probe.DictObj) and isinstance(m, dict))
    check("subclass method dispatch", m.describe() == "MyDict:DictObj")
    check("subclass init content", m["/Custom"] is True)

    a = probe.ArrayObj()
    a.extend([1, 2, 3])
    check("ArrayObj is list", isinstance(a, list))
    check("list behaviour", a[0] == 1 and len(a) == 3 and list(reversed(a)) == [3, 2, 1])
    check("list equality", a == [1, 2, 3])

    f = probe.FloatObj(1.25)
    check("FloatObj is float", isinstance(f, float))
    check("float arithmetic", float(f) * 2 == 2.5 and f == 1.25)
    check("float repr", repr(f) == "1.25")

    # deepcopy semantics (tests deepcopy page boxes)
    dc = copy.deepcopy(d)
    dc["/Count"] = 99
    check("deepcopy independent", d["/Count"] == 3 and dc["/Count"] == 99)
    check("deepcopy type", isinstance(dc, probe.DictObj) and isinstance(dc, dict))

    # hashability of containers is not required (dicts are unhashable) but the
    # nested values must survive round-trips through dict/list APIs.
    check("nested containers", isinstance(d["/MediaBox"], list))

    print()
    if failures:
        print(f"PROBE FAILED: {len(failures)} failing checks: {failures}")
        return 1
    print("PROBE PASSED: object model assumptions hold")
    return 0


if __name__ == "__main__":
    sys.exit(main())
