#!/usr/bin/env bash
# One-shot acceptance gate (A1-A7).  Prints a PASS/FAIL line per gate and exits
# non-zero if any gate fails.
#
# Usage: ./scripts/gate.sh [--quick]
#   --quick  skip the fuzzy stage and the 200k float cross-check
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
QUICK=0
[ "${1:-}" = "--quick" ] && QUICK=1

ORACLE=".venvs/oracle/bin/python"
DEV=".venvs/dev/bin/python"
mkdir -p .audit/gate
declare -A STATUS
overall=0

step() { printf '\n=== %s ===\n' "$1"; }
record() { STATUS["$1"]=$2; [ "$2" = PASS ] || overall=1; printf 'GATE %-4s %s\n' "$1" "$2"; }

step "build"
if ./scripts/build.sh > .audit/gate/build.log 2>&1; then record BUILD PASS; else record BUILD FAIL; tail -20 .audit/gate/build.log; fi

step "A1 official test suite"
if "$DEV" -m pytest tests -n auto -q -p no:benchmark \
        --junitxml=.audit/rust-junit.xml > .audit/gate/pytest.log 2>&1 \
   && "$ORACLE" tools/baseline/snapshot.py .audit/rust-junit.xml \
        --compare .audit/baseline-outcomes.json > .audit/gate/snapshot.log 2>&1; then
    record A1 PASS
else
    record A1 FAIL
    tail -12 .audit/gate/pytest.log
    tail -12 .audit/gate/snapshot.log 2>/dev/null
fi

step "A2/A3/A4 differential oracle"
PYTHONHASHSEED=0 "$ORACLE" tools/oracle/audit.py --out .audit/oracle --seed 42 > .audit/gate/oracle.log 2>&1
PYTHONHASHSEED=0 "$DEV" tools/oracle/audit.py --out .audit/rust --seed 42 > .audit/gate/rust-oracle.log 2>&1
if PYTHONHASHSEED=0 "$ORACLE" tools/oracle/compare.py .audit/oracle .audit/rust > .audit/gate/compare.log 2>&1; then
    record A2A3A4 PASS
else
    record A2A3A4 FAIL
    head -20 .audit/gate/compare.log
fi

step "A5 image byte parity"
if ./scripts/probe_image_parity.sh > .audit/gate/image-parity.log 2>&1; then record A5 PASS; else record A5 FAIL; tail -10 .audit/gate/image-parity.log; fi

step "A6 symbol coverage"
if "$ORACLE" tools/manifest/check.py --require 100 --write-report docs/SYMBOL_COVERAGE.md > .audit/gate/coverage.log 2>&1; then
    record A6 PASS
else
    record A6 FAIL
    head -3 .audit/gate/coverage.log
fi

if [ "$QUICK" -eq 0 ]; then
    step "A7 fuzz + formatting cross-check"
    if "$ORACLE" tools/probe/pyfmt_crosscheck.py --count 200000 > .audit/gate/pyfmt.log 2>&1 \
       && { [ ! -x scripts/fuzz.sh ] || ./scripts/fuzz.sh > .audit/gate/fuzz.log 2>&1; }; then
        record A7 PASS
    else
        record A7 FAIL
        tail -10 .audit/gate/pyfmt.log
    fi
fi

step "summary"
for gate in BUILD A1 A2A3A4 A5 A6 A7; do
    [ -n "${STATUS[$gate]:-}" ] && printf '%-8s %s\n' "$gate" "${STATUS[$gate]}"
done
if [ "$overall" -eq 0 ]; then
    echo "ALL GATES PASSED"
else
    echo "GATE FAILURES PRESENT"
fi
exit "$overall"
