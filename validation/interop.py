#!/usr/bin/env python3
"""§9.2 Independent-tool interop for outputs produced by both backends.

For every output artifact recorded by ``validation/roundtrip.py`` this script
runs independent PDF tools and cross-checks their answers against the backend's
own semantic digest:

* ``qpdf --check``            structural / xref validation
* ``qpdf --qdf``              QDF rewrite, re-checked and re-read with pypdf
* ``pdfinfo``                 page count, PDF version, document info
* ``pdftotext``               text extraction vs pypdf ``extract_text()``
* ``mutool info``             page count
* ``mutool clean``            rewrite, re-read with pypdf
* ``pdftoppm``                page 0 at 72 DPI, PNG hash (transform outputs)

Missing binaries are reported as ``NOT_AVAILABLE`` and never as a pass; the
per-backend status becomes ``INCOMPLETE`` when anything could not be verified.

Comparison policy
-----------------
``qpdf --check`` exit code 0 -> PASS, 3 -> WARN (qpdf found warnings but no
errors), anything else -> FAIL.  ``qpdf --qdf`` exit 3 means "operation
succeeded with warnings" (common for incremental inputs); the QDF output is
then verified with ``qpdf --check`` and a pypdf re-read and the check is
recorded as WARN, not FAIL.  For ``pdftotext`` the text is compared
exactly first; a whitespace-free comparison (``remove_all_whitespace``, all
Unicode whitespace removed, including form feeds) is also computed and used as
the pass criterion, while the exact diff is always recorded.  When the two
extractors disagree even after that normalisation the check is recorded as
``WARN`` with ``text_divergence: true`` (it is a pypdf-vs-poppler difference,
not proof of a structural failure); the two backends are then compared against
each other by ``--cross``, so a real port-only divergence still fails.  No
other normalisation is applied silently.

Cross-backend mode
------------------
``--cross <oracle.json> <dev.json>`` compares the rendered page-0 PNG hashes
and the ``pdftotext`` whitespace-free hashes of the two backends, plus tool
status/count fields, and exits non-zero on any difference.  This is the
"identical content renders identically" check from §9.3.

Usage
-----
    .venvs/oracle/bin/python validation/interop.py --backend oracle
    .venvs/dev/bin/python    validation/interop.py --backend dev
    .venvs/dev/bin/python    validation/interop.py --cross \
        validation/results/interop-oracle.json validation/results/interop-dev.json
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import rt_common as rc  # noqa: E402

TOOL_NAMES = ("qpdf", "pdfinfo", "pdftotext", "mutool", "pdftoppm")
VERSION_ARGS = {
    "qpdf": ["qpdf", "--version"],
    "pdfinfo": ["pdfinfo", "-v"],
    "pdftotext": ["pdftotext", "-v"],
    "mutool": ["mutool", "-v"],
    "pdftoppm": ["pdftoppm", "-v"],
}
PASS_STATUSES = {"PASS", "WARN"}

NORMALIZATIONS = {
    "pdftotext": (
        "exact text is recorded; the pass criterion is remove_all_whitespace "
        "(all Unicode whitespace, including form feeds, removed) because "
        "pdftotext and pypdf use different layout algorithms"
    ),
    "qpdf_check": "exit code 0 = PASS, 3 = WARN (qpdf warnings only), else FAIL",
    "qpdf_qdf": "exit 3 (operation succeeded with warnings) = WARN when the QDF output passes qpdf --check and pypdf re-read",
    "render": "pdftoppm -r 72 -f 1 -l 1 -png -singlefile, page 0, PNG bytes hashed",
}


# ---------------------------------------------------------------------------
# process helpers
# ---------------------------------------------------------------------------
def run(argv: list[str], timeout: float) -> dict[str, Any]:
    started = time.time()
    try:
        completed = subprocess.run(
            argv, capture_output=True, text=True, timeout=timeout
        )
        return {
            "rc": completed.returncode,
            "stdout": completed.stdout,
            "stderr": completed.stderr,
            "duration_ms": int((time.time() - started) * 1000),
            "error": None,
        }
    except FileNotFoundError:
        return {
            "rc": None,
            "stdout": "",
            "stderr": "",
            "duration_ms": int((time.time() - started) * 1000),
            "error": "NOT_AVAILABLE",
        }
    except subprocess.TimeoutExpired:
        return {
            "rc": None,
            "stdout": "",
            "stderr": "",
            "duration_ms": int((time.time() - started) * 1000),
            "error": "TIMEOUT",
        }


def _tail(text: str, limit: int = 1200) -> str:
    text = text.strip()
    return text[-limit:] if len(text) > limit else text


def discover_tools() -> dict[str, dict[str, Any]]:
    tools: dict[str, dict[str, Any]] = {}
    for name in TOOL_NAMES:
        path = shutil.which(name)
        entry: dict[str, Any] = {
            "available": bool(path),
            "path": path,
            "version": None,
        }
        if path:
            result = run(VERSION_ARGS[name], timeout=20)
            output = (result["stdout"] or "") + (result["stderr"] or "")
            lines = [line.strip() for line in output.splitlines() if line.strip()]
            entry["version"] = lines[0][:200] if lines else None
        tools[name] = entry
    return tools


# ---------------------------------------------------------------------------
# status / parsing helpers
# ---------------------------------------------------------------------------
def qpdf_status(result: dict[str, Any]) -> tuple[str, str]:
    if result["error"] == "NOT_AVAILABLE":
        return "NOT_AVAILABLE", "qpdf not installed"
    if result["error"]:
        return "FAIL", result["error"]
    rc = result["rc"]
    if rc == 0:
        return "PASS", ""
    if rc == 3:
        return "WARN", "qpdf reported warnings (exit 3)"
    return "FAIL", f"qpdf exit {rc}"


def pdfinfo_parse(stdout: str) -> dict[str, Any]:
    fields: dict[str, str] = {}
    for line in stdout.splitlines():
        if ": " in line:
            key, value = line.split(": ", 1)
            fields[key.strip()] = value.strip()
    pages = None
    if "Pages" in fields:
        try:
            pages = int(fields["Pages"])
        except ValueError:
            pages = None
    return {
        "pages": pages,
        "version": fields.get("PDF version"),
        "encrypted": fields.get("Encrypted"),
        "title": fields.get("Title"),
        "producer": fields.get("Producer"),
        "page_size": fields.get("Page size"),
    }


def mutool_pages(stdout: str) -> int | None:
    match = re.search(r"^Pages:\s*(\d+)", stdout, re.MULTILINE)
    return int(match.group(1)) if match else None


def remove_whitespace(text: str) -> str:
    return "".join(text.split())


def first_difference(left: str, right: str) -> int | None:
    limit = min(len(left), len(right))
    for index in range(limit):
        if left[index] != right[index]:
            return index
    if len(left) != len(right):
        return limit
    return None


def _context(text: str, index: int, radius: int = 100) -> str:
    return text[max(0, index - radius) : index + radius]


def text_comparison(expected: str, extracted: str, truncated: bool) -> dict[str, Any]:
    exact_match = expected == extracted
    normalized_expected = remove_whitespace(expected)
    normalized_extracted = remove_whitespace(extracted)
    whitespace_free_match = normalized_expected == normalized_extracted
    normalization = "remove_all_whitespace"
    if not whitespace_free_match and truncated:
        # The stored expectation is truncated to TEXT_CAP; compare the common
        # prefix and record that the check was weakened by the truncation.
        whitespace_free_match = normalized_extracted.startswith(
            normalized_expected
        ) or normalized_expected.startswith(normalized_extracted[: len(normalized_expected)])
        normalization = "remove_all_whitespace_prefix_due_to_text_truncation"
    result: dict[str, Any] = {
        "exact_match": exact_match,
        "whitespace_free_match": whitespace_free_match,
        "normalization": normalization,
        "expected_len": len(expected),
        "expected_whitespace_free_len": len(normalized_expected),
        "extracted_len": len(extracted),
        "extracted_whitespace_free_len": len(normalized_extracted),
        "text_ws_free_sha256": rc.sha256_text(normalized_extracted),
        "expected_text_truncated": truncated,
    }
    if not exact_match:
        index = first_difference(expected, extracted)
        result["exact_diff"] = {
            "index": index,
            "pypdf_context": _context(expected, index if index is not None else 0),
            "pdftotext_context": _context(
                extracted, index if index is not None else 0
            ),
        }
    if not whitespace_free_match:
        index = first_difference(normalized_expected, normalized_extracted)
        result["whitespace_free_diff"] = {
            "index": index,
            "pypdf_context": _context(
                normalized_expected, index if index is not None else 0
            ),
            "pdftotext_context": _context(
                normalized_extracted, index if index is not None else 0
            ),
        }
    return result


# ---------------------------------------------------------------------------
# per-target checks
# ---------------------------------------------------------------------------
def _qpdf_password_args(password: str | None) -> list[str]:
    return [f"--password={password}"] if password else []


def _poppler_password_args(password: str | None) -> list[str]:
    return ["-upw", password] if password else []


def _mutool_password_args(password: str | None) -> list[str]:
    return ["-p", password] if password else []


def check_target(
    case: dict[str, Any],
    target: dict[str, Any],
    tools: dict[str, dict[str, Any]],
    workdir: Path,
    *,
    dpi: int,
    timeout: float,
) -> dict[str, Any]:
    file_path = Path(target["file"])
    password = target.get("password")
    selected = target.get("checks", "all")
    variant = case.get("variants", {}).get(target.get("digest_variant", target["variant"]), {})
    digest = variant.get("digest") or {}
    pages = digest.get("pages") or []
    edge = pages[0] if pages else {}
    expected_pages = digest.get("page_count")
    target_dir = workdir / "cases" / rc.safe_name(case["case_id"]) / rc.safe_name(target["variant"])
    target_dir.mkdir(parents=True, exist_ok=True)

    entry: dict[str, Any] = {
        "file": str(file_path),
        "workflow": case.get("workflow"),
        "variant": target.get("variant"),
        "digest_variant": target.get("digest_variant", target.get("variant")),
        "expected_page_count": expected_pages,
        "password_protected": bool(password),
    }
    checks: dict[str, Any] = {}

    def wanted(name: str) -> bool:
        return selected == "all" or name in selected

    qpdf_available = tools["qpdf"]["available"]

    # -- qpdf --check -------------------------------------------------------
    if wanted("qpdf_check"):
        if not qpdf_available:
            checks["qpdf_check"] = {"status": "NOT_AVAILABLE"}
        else:
            result = run(
                ["qpdf", *_qpdf_password_args(password), "--check", str(file_path)],
                timeout=timeout,
            )
            status, detail = qpdf_status(result)
            checks["qpdf_check"] = {
                "status": status,
                "detail": detail or None,
                "rc": result["rc"],
                "stdout_tail": _tail(result["stdout"]),
                "stderr_tail": _tail(result["stderr"]),
                "duration_ms": result["duration_ms"],
            }

    # -- qpdf --qdf ---------------------------------------------------------
    if wanted("qpdf_qdf"):
        if not qpdf_available:
            checks["qpdf_qdf"] = {"status": "NOT_AVAILABLE"}
        else:
            qdf_path = target_dir / "qdf.pdf"
            convert = run(
                [
                    "qpdf",
                    *_qpdf_password_args(password),
                    "--qdf",
                    str(file_path),
                    str(qdf_path),
                ],
                timeout=timeout,
            )
            check: dict[str, Any] = {
                "convert_rc": convert["rc"],
                "convert_stderr_tail": _tail(convert["stderr"]),
            }
            if convert["error"]:
                check["status"] = "FAIL"
                check["detail"] = convert["error"]
            elif convert["rc"] not in (0, 3) or not qdf_path.exists():
                # qpdf uses exit 3 for "operation succeeded with warnings"
                # (e.g. an incremental file with several %%EOF markers); the
                # QDF output is still written and is verified below.
                check["status"] = "FAIL"
                check["detail"] = f"qpdf --qdf exit {convert['rc']}"
            else:
                verified = run(["qpdf", "--check", str(qdf_path)], timeout=timeout)
                status, detail = qpdf_status(verified)
                check["check_rc"] = verified["rc"]
                check["check_status"] = status
                # Round-trip sanity: pypdf must still be able to read the QDF file.
                pages_read: Any = None
                read_error = None
                try:
                    rc.install_deterministic(rc.seed_for(case["case_id"]))
                    reader, _, _ = rc.open_reader(qdf_path, candidates=[""])
                    pages_read = len(reader.pages)
                except Exception as exc:  # noqa: BLE001
                    read_error = f"{type(exc).__name__}: {exc}"
                check["pypdf_page_count"] = pages_read
                check["pypdf_read_error"] = read_error
                if status not in PASS_STATUSES or pages_read != expected_pages:
                    check["status"] = "FAIL"
                    check["detail"] = (
                        f"qpdf check={status}, pypdf pages={pages_read} "
                        f"expected={expected_pages}"
                    )
                elif convert["rc"] == 3 or status == "WARN":
                    check["status"] = "WARN"
                    check["detail"] = (
                        "qpdf --qdf reported warnings (exit 3); the QDF output "
                        "passed qpdf --check and pypdf re-read"
                    )
                else:
                    check["status"] = "PASS"
                    check["detail"] = detail or None
            checks["qpdf_qdf"] = check

    # -- pdfinfo ------------------------------------------------------------
    if wanted("pdfinfo"):
        if not tools["pdfinfo"]["available"]:
            checks["pdfinfo"] = {"status": "NOT_AVAILABLE"}
        else:
            result = run(
                ["pdfinfo", *_poppler_password_args(password), str(file_path)],
                timeout=timeout,
            )
            parsed = pdfinfo_parse(result["stdout"])
            parsed.update(
                {
                    "rc": result["rc"],
                    "stderr_tail": _tail(result["stderr"]),
                    "expected_pages": expected_pages,
                }
            )
            if result["error"]:
                parsed["status"] = "FAIL"
                parsed["detail"] = result["error"]
            elif result["rc"] != 0:
                parsed["status"] = "FAIL"
                parsed["detail"] = f"pdfinfo exit {result['rc']}"
            elif parsed["pages"] != expected_pages:
                parsed["status"] = "FAIL"
                parsed["detail"] = (
                    f"pdfinfo pages={parsed['pages']} expected={expected_pages}"
                )
            else:
                parsed["status"] = "PASS"
                parsed["detail"] = None
            checks["pdfinfo"] = parsed

    # -- pdftotext ----------------------------------------------------------
    if wanted("pdftotext"):
        expected_text = edge.get("text")
        if not tools["pdftotext"]["available"]:
            checks["pdftotext"] = {"status": "NOT_AVAILABLE"}
        elif expected_text is None or edge.get("text_skipped"):
            checks["pdftotext"] = {
                "status": "SKIP",
                "detail": "no pypdf text available for page 0",
            }
        else:
            result = run(
                [
                    "pdftotext",
                    "-f",
                    "1",
                    "-l",
                    "1",
                    "-enc",
                    "UTF-8",
                    *_poppler_password_args(password),
                    str(file_path),
                    "-",
                ],
                timeout=timeout,
            )
            if result["error"]:
                checks["pdftotext"] = {
                    "status": "FAIL",
                    "detail": result["error"],
                    "stderr_tail": _tail(result["stderr"]),
                }
            elif result["rc"] != 0:
                checks["pdftotext"] = {
                    "status": "FAIL",
                    "detail": f"pdftotext exit {result['rc']}",
                    "stderr_tail": _tail(result["stderr"]),
                }
            else:
                comparison = text_comparison(
                    expected_text,
                    result["stdout"],
                    bool(edge.get("text_truncated")),
                )
                if comparison["whitespace_free_match"]:
                    comparison["status"] = "PASS"
                    comparison["detail"] = None
                else:
                    # A pypdf-vs-pdftotext divergence is not a structural
                    # failure and is identical in both backends (checked by
                    # --cross); record it as WARN with the exact diff so it is
                    # analysed instead of silently passing or failing the gate.
                    comparison["status"] = "WARN"
                    comparison["text_divergence"] = True
                    comparison["detail"] = (
                        "pypdf extract_text() and pdftotext disagree "
                        "(order/content); exact diff recorded"
                    )
                comparison["rc"] = result["rc"]
                checks["pdftotext"] = comparison

    # -- mutool info --------------------------------------------------------
    if wanted("mutool_info"):
        if not tools["mutool"]["available"]:
            checks["mutool_info"] = {"status": "NOT_AVAILABLE"}
        else:
            result = run(
                ["mutool", "info", *_mutool_password_args(password), str(file_path)],
                timeout=timeout,
            )
            pages_found = mutool_pages(result["stdout"])
            info: dict[str, Any] = {
                "rc": result["rc"],
                "pages": pages_found,
                "expected_pages": expected_pages,
                "stderr_tail": _tail(result["stderr"]),
            }
            if result["error"]:
                info["status"] = "FAIL"
                info["detail"] = result["error"]
            elif result["rc"] != 0:
                info["status"] = "FAIL"
                info["detail"] = f"mutool info exit {result['rc']}"
            elif pages_found != expected_pages:
                info["status"] = "FAIL"
                info["detail"] = (
                    f"mutool info pages={pages_found} expected={expected_pages}"
                )
            else:
                info["status"] = "PASS"
                info["detail"] = None
            checks["mutool_info"] = info

    # -- mutool clean + pypdf re-read --------------------------------------
    if wanted("mutool_clean"):
        if not tools["mutool"]["available"]:
            checks["mutool_clean"] = {"status": "NOT_AVAILABLE"}
        elif password:
            checks["mutool_clean"] = {
                "status": "SKIP",
                "detail": "encrypted target: mutool clean password handling not covered",
            }
        else:
            cleaned = target_dir / "mutool-clean.pdf"
            result = run(
                ["mutool", "clean", str(file_path), str(cleaned)], timeout=timeout
            )
            info = {
                "rc": result["rc"],
                "stderr_tail": _tail(result["stderr"]),
            }
            if result["error"]:
                info["status"] = "FAIL"
                info["detail"] = result["error"]
            elif result["rc"] != 0 or not cleaned.exists():
                info["status"] = "FAIL"
                info["detail"] = f"mutool clean exit {result['rc']}"
            else:
                try:
                    reader, _, _ = rc.open_reader(cleaned, candidates=[""])
                    rewritten = rc.digest_reader(reader, text="sha", text_pages=1)
                    info["page_count"] = rewritten["page_count"]
                    info["expected_page_count"] = expected_pages
                    first_page = (rewritten.get("pages") or [{}])[0]
                    info["text_preserved"] = (
                        first_page.get("text_sha256") == edge.get("text_sha256")
                        if edge.get("text_sha256") is not None
                        else None
                    )
                    ok = rewritten["page_count"] == expected_pages and info[
                        "text_preserved"
                    ] in (True, None)
                    info["status"] = "PASS" if ok else "FAIL"
                    info["detail"] = None if ok else "rewritten document differs"
                except Exception as exc:  # noqa: BLE001
                    info["status"] = "FAIL"
                    info["detail"] = f"{type(exc).__name__}: {exc}"
            checks["mutool_clean"] = info

    # -- pdftoppm render ----------------------------------------------------
    if target.get("render") and wanted("pdftoppm"):
        if not tools["pdftoppm"]["available"]:
            checks["pdftoppm_render"] = {"status": "NOT_AVAILABLE"}
        elif password:
            checks["pdftoppm_render"] = {
                "status": "SKIP",
                "detail": "encrypted target: render comparison not covered",
            }
        else:
            prefix = target_dir / "page0"
            result = run(
                [
                    "pdftoppm",
                    "-r",
                    str(dpi),
                    "-f",
                    "1",
                    "-l",
                    "1",
                    "-png",
                    "-singlefile",
                    str(file_path),
                    str(prefix),
                ],
                timeout=timeout,
            )
            png_path = Path(f"{prefix}.png")
            info = {
                "rc": result["rc"],
                "page": 0,
                "dpi": dpi,
                "stderr_tail": _tail(result["stderr"]),
            }
            if result["error"]:
                info["status"] = "FAIL"
                info["detail"] = result["error"]
            elif result["rc"] != 0 or not png_path.exists():
                info["status"] = "FAIL"
                info["detail"] = f"pdftoppm exit {result['rc']} / no PNG"
            else:
                data = png_path.read_bytes()
                info.update(
                    {
                        "status": "PASS",
                        "detail": None,
                        "png_sha256": rc.sha256_bytes(data),
                        "png_bytes": len(data),
                    }
                )
            checks["pdftoppm_render"] = info

    entry["checks"] = checks
    entry["status"] = overall_status(checks)
    return entry


def overall_status(checks: dict[str, Any]) -> str:
    statuses = {check.get("status") for check in checks.values()}
    if "FAIL" in statuses:
        return "FAIL"
    if "NOT_AVAILABLE" in statuses or "SKIP" in statuses:
        return "INCOMPLETE"
    if "WARN" in statuses:
        return "WARN"
    if not statuses:
        return "SKIP"
    return "PASS"


# ---------------------------------------------------------------------------
# backend run
# ---------------------------------------------------------------------------
def summarize(cases: dict[str, Any]) -> dict[str, Any]:
    counts: dict[str, dict[str, int]] = {}
    case_statuses: dict[str, int] = {}
    failures: list[dict[str, Any]] = []
    warnings: list[dict[str, Any]] = []
    targets = 0
    for case_id, case in cases.items():
        case_statuses[case["status"]] = case_statuses.get(case["status"], 0) + 1
        for variant, target in case["targets"].items():
            targets += 1
            for name, check in target["checks"].items():
                bucket = counts.setdefault(
                    name,
                    {
                        "pass": 0,
                        "warn": 0,
                        "fail": 0,
                        "not_available": 0,
                        "skip": 0,
                    },
                )
                status = check.get("status")
                key = {
                    "PASS": "pass",
                    "WARN": "warn",
                    "FAIL": "fail",
                    "NOT_AVAILABLE": "not_available",
                    "SKIP": "skip",
                }.get(status, "fail")
                bucket[key] += 1
                if status == "FAIL":
                    failures.append(
                        {
                            "case_id": case_id,
                            "variant": variant,
                            "check": name,
                            "detail": check.get("detail"),
                        }
                    )
                elif status == "WARN":
                    warnings.append(
                        {
                            "case_id": case_id,
                            "variant": variant,
                            "check": name,
                            "detail": check.get("detail"),
                            "text_divergence": bool(check.get("text_divergence")),
                        }
                    )
    if failures:
        status = "FAIL"
    elif any(
        bucket["not_available"] or bucket["skip"] for bucket in counts.values()
    ):
        status = "INCOMPLETE"
    elif warnings:
        status = "WARN"
    else:
        status = "PASS"
    return {
        "cases": len(cases),
        "targets": targets,
        "case_status": case_statuses,
        "checks": counts,
        "failures": failures,
        "warnings": warnings,
        "status": status,
    }


def run_backend(args: argparse.Namespace) -> int:
    backend = rc.detect_backend()
    if backend != args.backend:
        print(
            f"error: this interpreter provides backend {backend!r}, but --backend "
            f"{args.backend!r} was requested; run with .venvs/{args.backend}/bin/python",
            file=sys.stderr,
        )
        return 2

    roundtrip_path = (
        Path(args.roundtrip)
        if args.roundtrip
        else rc.RESULTS / f"roundtrip-{args.backend}.json"
    )
    if not roundtrip_path.exists():
        print(f"error: roundtrip results not found: {roundtrip_path}", file=sys.stderr)
        return 2
    payload = rc.read_json(roundtrip_path)

    workdir = (
        Path(args.workdir).resolve()
        if args.workdir
        else Path(tempfile.mkdtemp(prefix=f"pypdf-interop-{args.backend}-"))
    )
    workdir.mkdir(parents=True, exist_ok=True)

    tools = discover_tools()
    missing = [name for name, info in tools.items() if not info["available"]]
    if missing:
        print(f"WARNING: tools not available: {missing} (recorded as NOT_AVAILABLE)")

    case_specs = sorted(payload.get("cases", []), key=lambda case: case["case_id"])
    if args.match:
        case_specs = [case for case in case_specs if args.match in case["case_id"]]
    if args.limit and args.limit > 0:
        case_specs = case_specs[: args.limit]

    cases: dict[str, Any] = {}
    print(
        f"interop: backend={args.backend} cases={len(case_specs)} "
        f"workdir={workdir} dpi={args.dpi}",
        flush=True,
    )
    started = time.time()
    for index, case in enumerate(case_specs, 1):
        case_id = case["case_id"]
        targets = case.get("interop_targets") or []
        if not targets:
            case_entry: dict[str, Any] = {
                "workflow": case.get("workflow"),
                "status": "SKIP",
                "detail": "no interop target recorded"
                + (f" ({case.get('skip_reason')})" if case.get("skip_reason") else ""),
                "targets": {},
            }
            cases[case_id] = case_entry
            print(f"[{index:>3}/{len(case_specs)}] SKIP    {case_id}", flush=True)
            continue
        case_targets: dict[str, Any] = {}
        case_failed = False
        for target in targets:
            try:
                result = check_target(
                    case,
                    target,
                    tools,
                    workdir,
                    dpi=args.dpi,
                    timeout=args.timeout,
                )
            except Exception as exc:  # noqa: BLE001 - one broken artifact must not stop the run
                result = {
                    "file": target.get("file"),
                    "variant": target.get("variant"),
                    "checks": {},
                    "status": "FAIL",
                    "detail": f"interop harness error: {type(exc).__name__}: {exc}",
                }
            case_targets[target["variant"]] = result
            if result["status"] == "FAIL":
                case_failed = True
        statuses = [target["status"] for target in case_targets.values()]
        if case_failed:
            case_status = "FAIL"
        elif "INCOMPLETE" in statuses:
            case_status = "INCOMPLETE"
        elif "WARN" in statuses:
            case_status = "WARN"
        else:
            case_status = "PASS"
        cases[case_id] = {
            "workflow": case.get("workflow"),
            "status": case_status,
            "detail": None,
            "targets": case_targets,
        }
        marker = case_status
        print(f"[{index:>3}/{len(case_specs)}] {marker:<10} {case_id}", flush=True)

    summary = summarize(cases)
    output = {
        "generator": "validation/interop.py",
        "backend": args.backend,
        "pypdf": rc.backend_info(args.backend),
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "duration_s": round(time.time() - started, 3),
        "roundtrip_file": str(roundtrip_path),
        "render_dpi": args.dpi,
        "workdir": str(workdir),
        "tools": tools,
        "normalizations": NORMALIZATIONS,
        "cases": cases,
        "summary": summary,
    }
    out_path = Path(args.out) if args.out else rc.RESULTS / f"interop-{args.backend}.json"
    rc.write_json(out_path, output)
    print_summary(output)
    print(f"\nresults -> {out_path}")
    return 1 if summary["status"] == "FAIL" else 0


def print_summary(output: dict[str, Any]) -> None:
    summary = output["summary"]
    print()
    print(f"backend={output['backend']} cases={summary['cases']} targets={summary['targets']}")
    tool_bits = []
    for name, info in output["tools"].items():
        tool_bits.append(f"{name}={'yes' if info['available'] else 'NOT_AVAILABLE'}")
    print("tools: " + " ".join(tool_bits))
    print()
    print(f"{'check':<20}{'pass':>7}{'warn':>7}{'fail':>7}{'n/a':>7}{'skip':>7}")
    for name, bucket in summary["checks"].items():
        print(
            f"{name:<20}{bucket['pass']:>7}{bucket['warn']:>7}{bucket['fail']:>7}"
            f"{bucket['not_available']:>7}{bucket['skip']:>7}"
        )
    print()
    print(f"status: {summary['status']}")
    if summary["failures"]:
        print(f"\nFAILURES ({len(summary['failures'])}):")
        for failure in summary["failures"][:50]:
            print(
                f"  - {failure['case_id']} [{failure['variant']}] "
                f"{failure['check']}: {failure['detail']}"
            )
        if len(summary["failures"]) > 50:
            print(f"  ... and {len(summary['failures']) - 50} more (see JSON)")
    if summary.get("warnings"):
        print(f"\nWARNINGS ({len(summary['warnings'])}):")
        for warning in summary["warnings"][:50]:
            print(
                f"  - {warning['case_id']} [{warning['variant']}] "
                f"{warning['check']}: {warning['detail']}"
            )
        if len(summary["warnings"]) > 50:
            print(f"  ... and {len(summary['warnings']) - 50} more (see JSON)")
    if summary["status"] == "INCOMPLETE":
        print(
            "\nINCOMPLETE: at least one check could not be run (tool missing or "
            "no applicable artifact); these are NOT counted as passes."
        )


# ---------------------------------------------------------------------------
# cross-backend comparison
# ---------------------------------------------------------------------------
def cross_compare(left_path: Path, right_path: Path) -> int:
    left = rc.read_json(left_path)
    right = rc.read_json(right_path)
    left_backend = left.get("backend")
    right_backend = right.get("backend")
    if {left_backend, right_backend} != {"oracle", "dev"}:
        print(
            f"error: expected oracle/dev interop files, got {left_backend!r}/{right_backend!r}",
            file=sys.stderr,
        )
        return 2

    diffs: list[dict[str, Any]] = []
    compared_renders = 0
    compared_text = 0
    compared_status = 0
    left_cases = left.get("cases", {})
    right_cases = right.get("cases", {})
    for case_id in sorted(set(left_cases) | set(right_cases)):
        left_case = left_cases.get(case_id)
        right_case = right_cases.get(case_id)
        if left_case is None or right_case is None:
            diffs.append(
                {
                    "case_id": case_id,
                    "field": "<case>",
                    "oracle": "present" if left_case else "missing",
                    "dev": "present" if right_case else "missing",
                }
            )
            continue
        left_targets = left_case.get("targets", {})
        right_targets = right_case.get("targets", {})
        for variant in sorted(set(left_targets) | set(right_targets)):
            left_target = left_targets.get(variant)
            right_target = right_targets.get(variant)
            if left_target is None or right_target is None:
                diffs.append(
                    {
                        "case_id": case_id,
                        "field": f"{variant}.<target>",
                        "oracle": "present" if left_target else "missing",
                        "dev": "present" if right_target else "missing",
                    }
                )
                continue
            left_checks = left_target.get("checks", {})
            right_checks = right_target.get("checks", {})
            for check_name in sorted(set(left_checks) | set(right_checks)):
                left_check = left_checks.get(check_name, {})
                right_check = right_checks.get(check_name, {})
                if check_name == "pdftoppm_render":
                    compared_renders += 1
                    left_hash = left_check.get("png_sha256")
                    right_hash = right_check.get("png_sha256")
                    if left_hash != right_hash:
                        diffs.append(
                            {
                                "case_id": case_id,
                                "field": f"{variant}.{check_name}.png_sha256",
                                "oracle": left_hash,
                                "dev": right_hash,
                            }
                        )
                elif check_name == "pdftotext":
                    compared_text += 1
                    left_hash = left_check.get("text_ws_free_sha256")
                    right_hash = right_check.get("text_ws_free_sha256")
                    if left_hash != right_hash:
                        diffs.append(
                            {
                                "case_id": case_id,
                                "field": f"{variant}.{check_name}.text_ws_free_sha256",
                                "oracle": left_hash,
                                "dev": right_hash,
                            }
                        )
                else:
                    compared_status += 1
                    left_status = left_check.get("status")
                    right_status = right_check.get("status")
                    if left_status != right_status:
                        diffs.append(
                            {
                                "case_id": case_id,
                                "field": f"{variant}.{check_name}.status",
                                "oracle": left_status,
                                "dev": right_status,
                            }
                        )

    tool_diffs = []
    for name in TOOL_NAMES:
        left_tool = left.get("tools", {}).get(name, {})
        right_tool = right.get("tools", {}).get(name, {})
        if left_tool.get("available") != right_tool.get("available"):
            tool_diffs.append(
                {
                    "tool": name,
                    "oracle": left_tool.get("available"),
                    "dev": right_tool.get("available"),
                }
            )

    status = "MATCH" if not diffs and not tool_diffs else "DIFF"
    report = {
        "generator": "validation/interop.py --cross",
        "oracle_file": str(left_path),
        "dev_file": str(right_path),
        "compared_renders": compared_renders,
        "compared_text": compared_text,
        "compared_status_fields": compared_status,
        "diffs": diffs,
        "tool_availability_diffs": tool_diffs,
        "status": status,
    }
    print(
        f"interop cross-compare: renders={compared_renders} "
        f"texts={compared_text} status_fields={compared_status}"
    )
    if tool_diffs:
        print("\ntool availability differs:")
        for diff in tool_diffs:
            print(f"  {diff['tool']}: oracle={diff['oracle']} dev={diff['dev']}")
    if diffs:
        print(f"\nRESULT: DIFF - {len(diffs)} difference(s)")
        for diff in diffs[:100]:
            print(f"  [{diff['case_id']}] {diff['field']}")
            print(f"    oracle: {diff['oracle']}")
            print(f"    dev:    {diff['dev']}")
    else:
        print("\nRESULT: MATCH - rendered PNG bytes and pdftotext digests are identical")
    out_path = rc.RESULTS / "interop-cross.json"
    rc.write_json(out_path, report)
    print(f"\nreport -> {out_path}")
    return 0 if status == "MATCH" else 1


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------
def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--backend", choices=["oracle", "dev"])
    parser.add_argument("--roundtrip", default=None)
    parser.add_argument("--out", default=None)
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--match", default=None, help="only cases whose id contains this substring")
    parser.add_argument("--dpi", type=int, default=72)
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--workdir", default=None)
    parser.add_argument(
        "--cross",
        nargs=2,
        metavar=("ORACLE_JSON", "DEV_JSON"),
        default=None,
        help="compare two interop result files instead of running tools",
    )
    args = parser.parse_args(argv)

    if args.cross:
        return cross_compare(Path(args.cross[0]), Path(args.cross[1]))
    if not args.backend:
        parser.error("--backend is required unless --cross is used")
    return run_backend(args)


if __name__ == "__main__":
    raise SystemExit(main())
