#!/usr/bin/env bash
#
# Orchestration for the plan §8 security / robustness validation
# (malformed input corpus, resource limits, mutation fuzzing).
#
# Runs:
#   1. validation/security.py   adversarial corpus in both venvs + comparison
#   2. validation/fuzz.py       N mutated PDFs through tools/fuzz/run.py in both venvs
#   3. validation/security.py --render-report
#      (re-renders security-report.md so it also contains the fuzz summary)
#
# Every step is logged under validation/results/logs/.  The script exits
# non-zero when either differential step found a difference, a timeout, a
# crash/panic, a missing capability or when the tree changed mid-run.
#
# Usage:
#   bash validation/run_security.sh
#   bash validation/run_security.sh --heavy
#   bash validation/run_security.sh --fuzz-count 500 --seed 7
#   bash validation/run_security.sh --skip-fuzz
#   bash validation/run_security.sh --skip-security --fuzz-count 100
#
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
PY="$ROOT/.venvs/dev/bin/python"
LOG_DIR="$HERE/results/logs"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"

HEAVY=0
KEEP_CORPUS=0
SKIP_SECURITY=0
SKIP_FUZZ=0
FUZZ_COUNT=200
FUZZ_SEED=20261124
FUZZ_TIMEOUT=10

usage() {
    sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --heavy) HEAVY=1 ;;
        --keep-corpus) KEEP_CORPUS=1 ;;
        --skip-security) SKIP_SECURITY=1 ;;
        --skip-fuzz) SKIP_FUZZ=1 ;;
        --fuzz-count)
            [ "$#" -ge 2 ] || { echo "[run] --fuzz-count needs a value" >&2; exit 2; }
            FUZZ_COUNT="$2"; shift ;;
        --seed)
            [ "$#" -ge 2 ] || { echo "[run] --seed needs a value" >&2; exit 2; }
            FUZZ_SEED="$2"; shift ;;
        --timeout)
            [ "$#" -ge 2 ] || { echo "[run] --timeout needs a value" >&2; exit 2; }
            FUZZ_TIMEOUT="$2"; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "[run] unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

if [ ! -x "$PY" ]; then
    echo "[run] missing interpreter: $PY" >&2
    exit 2
fi
for script in security.py fuzz.py; do
    if [ ! -f "$HERE/$script" ]; then
        echo "[run] missing script: $HERE/$script" >&2
        exit 2
    fi
done

mkdir -p "$LOG_DIR"
SEC_LOG="$LOG_DIR/security-$STAMP.log"
FUZZ_LOG="$LOG_DIR/fuzz-$STAMP.log"
REPORT_LOG="$LOG_DIR/report-$STAMP.log"
SUMMARY_LOG="$LOG_DIR/summary-$STAMP.log"

say() {
    printf '%s\n' "$*"
    printf '%s\n' "$*" >>"$SUMMARY_LOG"
}

say "pypdf-rust security validation run $STAMP"
say "repository: $ROOT"
say "heavy mode: $HEAVY | fuzz count: $FUZZ_COUNT | seed: $FUZZ_SEED | per-file timeout: ${FUZZ_TIMEOUT}s"
say "logs: $LOG_DIR"

security_status=0
fuzz_status=0
report_status=0

# ---------------------------------------------------------------------------
# 1. adversarial corpus (plan §8.2 / §8.3)
# ---------------------------------------------------------------------------
if [ "$SKIP_SECURITY" -eq 0 ]; then
    sec_args=()
    if [ "$HEAVY" -eq 1 ]; then sec_args+=(--heavy); fi
    if [ "$KEEP_CORPUS" -eq 1 ]; then sec_args+=(--keep-corpus); fi
    say ""
    say "=== 1/3 security corpus -> $SEC_LOG"
    "$PY" "$HERE/security.py" ${sec_args[@]+"${sec_args[@]}"} 2>&1 | tee "$SEC_LOG"
    security_status=${PIPESTATUS[0]}
    say "security exit code: $security_status"
else
    say ""
    say "=== 1/3 security corpus SKIPPED (--skip-security)"
fi

# ---------------------------------------------------------------------------
# 2. mutation fuzzing (plan §8.4)
# ---------------------------------------------------------------------------
if [ "$SKIP_FUZZ" -eq 0 ]; then
    fuzz_args=(--count "$FUZZ_COUNT" --seed "$FUZZ_SEED" --timeout "$FUZZ_TIMEOUT")
    if [ "$KEEP_CORPUS" -eq 1 ]; then fuzz_args+=(--keep-corpus); fi
    say ""
    say "=== 2/3 mutation fuzzing ($FUZZ_COUNT files) -> $FUZZ_LOG"
    "$PY" "$HERE/fuzz.py" ${fuzz_args[@]+"${fuzz_args[@]}"} 2>&1 | tee "$FUZZ_LOG"
    fuzz_status=${PIPESTATUS[0]}
    say "fuzz exit code: $fuzz_status"
else
    say ""
    say "=== 2/3 mutation fuzzing SKIPPED (--skip-fuzz)"
fi

# ---------------------------------------------------------------------------
# 3. re-render the combined report (security + fuzz)
# ---------------------------------------------------------------------------
say ""
say "=== 3/3 report -> $REPORT_LOG"
"$PY" "$HERE/security.py" --render-report 2>&1 | tee "$REPORT_LOG"
report_status=${PIPESTATUS[0]}
say "report exit code: $report_status"

# ---------------------------------------------------------------------------
# summary
# ---------------------------------------------------------------------------
overall=0
if [ "$security_status" -ne 0 ] || [ "$fuzz_status" -ne 0 ] || [ "$report_status" -ne 0 ]; then
    overall=1
fi

say ""
say "=== result: $([ "$overall" -eq 0 ] && echo PASS || echo FAIL)"
say "security-report.md : $HERE/results/security-report.md"
say "fuzz-report.md     : $HERE/results/fuzz-report.md"
say "comparison JSON    : $HERE/results/security-comparison.json"
say "fuzz JSON          : $HERE/results/fuzz-comparison.json"

if [ "$security_status" -ne 0 ]; then
    say ""
    say "security mismatches:"
    grep -E "comparison:|^\[security\] (WARNING|port|oracle)" "$SEC_LOG" 2>/dev/null | tail -n 20 | sed 's/^/  /'
fi
if [ "$fuzz_status" -ne 0 ]; then
    say ""
    say "fuzz mismatches:"
    grep -E "comparison:|WARNING|crash|panic" "$FUZZ_LOG" 2>/dev/null | tail -n 20 | sed 's/^/  /'
fi

exit "$overall"
