#!/usr/bin/env bash
# Create the two pinned virtualenvs used by the project:
#   .venvs/oracle -- upstream pypdf 7.1.1 + optional deps (reference oracle)
#   .venvs/dev    -- our Rust port installed as ``pypdf`` (never upstream pypdf)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OPTIONAL="cryptography==50.0.2 Pillow==13.0.0 fonttools==4.68.0 brotli==1.2.1 arabic-reshaper==3.1.0 python-bidi==0.6.12 fpdf2 typing_extensions"
TESTDEPS="pytest pytest-cov pytest-socket pytest-timeout pytest-xdist PyYAML"

if [ "${1:-all}" = "oracle" ] || [ "${1:-all}" = "all" ]; then
    python3 -m venv .venvs/oracle
    .venvs/oracle/bin/pip install -q --upgrade pip setuptools wheel
    .venvs/oracle/bin/pip install -q "pypdf==7.1.1" $OPTIONAL $TESTDEPS
fi

if [ "${1:-all}" = "dev" ] || [ "${1:-all}" = "all" ]; then
    python3 -m venv .venvs/dev
    .venvs/dev/bin/pip install -q --upgrade pip setuptools wheel
    .venvs/dev/bin/pip install -q maturin $OPTIONAL $TESTDEPS
    VIRTUAL_ENV="$ROOT/.venvs/dev" .venvs/dev/bin/maturin develop --release
fi

echo "done. oracle: $(.venvs/oracle/bin/python -c 'import pypdf;print(pypdf.__version__)' 2>/dev/null || echo n/a)"
echo "done. dev:    $(.venvs/dev/bin/python -c 'import pypdf;print(pypdf.__version__)' 2>/dev/null || echo n/a)"
