#!/usr/bin/env bash
# §11: build a wheel and validate a clean, offline-ish install on this platform.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
OUT="validation/results"
LOGS="$OUT/logs"
mkdir -p "$OUT" "$LOGS"
VENV="${WHEEL_VENV:-/tmp/pypdf-rust-wheel-venv}"
status=0

echo "=== build wheel ==="
maturin build --release > "$LOGS/wheel-build.log" 2>&1 || { echo "BUILD FAILED"; tail -20 "$LOGS/wheel-build.log"; exit 1; }
wheel=$(ls -t target/wheels/pypdf-*.whl | head -1)
echo "wheel: $wheel"

echo "=== clean venv ==="
rm -rf "$VENV"
python3 -m venv "$VENV" >> "$LOGS/wheel-venv.log" 2>&1 || { echo "VENV FAILED"; exit 1; }

echo "=== install (no index, no deps) ==="
"$VENV/bin/pip" install --no-index --no-deps --force-reinstall "$wheel" > "$LOGS/wheel-install.log" 2>&1 \
  && echo "install: OK" || { echo "INSTALL FAILED"; tail -20 "$LOGS/wheel-install.log"; status=1; }

echo "=== smoke test on a real PDF ==="
"$VENV/bin/python" - "$ROOT" > "$LOGS/wheel-smoke.log" 2>&1 <<'PY' || status=1
import sys, pathlib
root = pathlib.Path(sys.argv[1])
import pypdf, pypdf._pypdf
from pypdf import PdfReader, PdfWriter
print("version:", pypdf.__version__)
print("extension:", pathlib.Path(pypdf._pypdf.__file__).name)
reader = PdfReader(root / "resources" / "crazyones.pdf")
print("pages:", len(reader.pages))
text = reader.pages[0].extract_text()
assert "The Crazy Ones" in text, text[:80]
writer = PdfWriter(clone_from=reader)
writer.add_metadata({"/Producer": "wheel-smoke"})
buffer = __import__("io").BytesIO()
writer.write(buffer)
buffer.seek(0)
assert len(PdfReader(buffer).pages) == 1
print("smoke: OK")
PY
tail -6 "$LOGS/wheel-smoke.log"

echo "=== platform tags / linkage ==="
"$VENV/bin/python" - <<'PY' | tee "$OUT/wheel-info.json"
import json, platform, sys, pathlib
import pypdf._pypdf as ext
info = {
    "wheel": sorted(pathlib.Path("target/wheels").glob("pypdf-*.whl"))[-1].name,
    "python": sys.version.split()[0],
    "platform": platform.platform(),
    "machine": platform.machine(),
    "extension": pathlib.Path(ext.__file__).name,
    "abi3": "no (CPython-specific ABI)",
}
print(json.dumps(info, indent=2))
PY

[ "$status" -eq 0 ] && echo "WHEEL VALIDATION: PASS" || echo "WHEEL VALIDATION: FAIL"
exit "$status"
