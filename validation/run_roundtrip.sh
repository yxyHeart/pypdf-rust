#!/usr/bin/env bash
#
# §9 Round-trip + independent-tool interop pipeline.
#
# Steps (all run, even when an earlier step failed; the script exits non-zero
# if any step failed):
#
#   1. roundtrip.py --backend oracle   -> validation/results/roundtrip-oracle.json
#   2. roundtrip.py --backend dev      -> validation/results/roundtrip-dev.json
#   3. compare_roundtrip.py            -> validation/results/compare-roundtrip.json
#   4. interop.py --backend oracle     -> validation/results/interop-oracle.json
#   5. interop.py --backend dev        -> validation/results/interop-dev.json
#   6. interop.py --cross              -> validation/results/interop-cross.json
#
# Logs: validation/results/logs/<timestamp>-<step>.log
#
# Environment overrides:
#   LIMIT=25              documents sampled per workflow (0 = whole corpus)
#   CASE_TIMEOUT=120      per-case subprocess timeout in seconds
#   SEED=<int>            PRNG/sample seed (default: tools/oracle/prng default)
#   DPI=72                pdftoppm render DPI for transform outputs
#   WORKFLOWS=RW,MERGE,TRANSFORM,EDIT,ENCRYPT,INCREMENTAL
#   STRICT=0              STRICT=1 also fails on per-case round-trip failures
#   ORACLE_PY, DEV_PY     interpreter overrides
#
# Examples:
#   validation/run_roundtrip.sh
#   LIMIT=10 CASE_TIMEOUT=60 validation/run_roundtrip.sh
#   WORKFLOWS=RW,TRANSFORM LIMIT=5 validation/run_roundtrip.sh

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

LIMIT="${LIMIT:-25}"
CASE_TIMEOUT="${CASE_TIMEOUT:-120}"
SEED="${SEED:-}"
DPI="${DPI:-72}"
WORKFLOWS="${WORKFLOWS:-RW,MERGE,TRANSFORM,EDIT,ENCRYPT,INCREMENTAL}"
STRICT="${STRICT:-0}"
ORACLE_PY="${ORACLE_PY:-$ROOT/.venvs/oracle/bin/python}"
DEV_PY="${DEV_PY:-$ROOT/.venvs/dev/bin/python}"

RESULTS="$ROOT/validation/results"
LOGS="$RESULTS/logs"
mkdir -p "$RESULTS" "$LOGS" || exit 2

STAMP="$(date +%Y%m%d-%H%M%S)"
SUMMARY_LOG="$LOGS/$STAMP-summary.log"

export PYTHONHASHSEED=0

missing=0
for interpreter in "$ORACLE_PY" "$DEV_PY"; do
    if [ ! -x "$interpreter" ]; then
        echo "error: interpreter not found/executable: $interpreter" >&2
        missing=1
    fi
done
if [ "$missing" -ne 0 ]; then
    echo "error: both venvs are required (.venvs/oracle, .venvs/dev)" >&2
    exit 2
fi

FAILED_STEPS=0
STEP_NAMES=()
STEP_CODES=()
STEP_TIMES=()

# Record the step outcome and keep going; the caller decides the exit status.
record_step() {
    local name="$1"
    local code="$2"
    local seconds="$3"
    STEP_NAMES+=("$name")
    STEP_CODES+=("$code")
    STEP_TIMES+=("$seconds")
    if [ "$code" -ne 0 ]; then
        FAILED_STEPS=$((FAILED_STEPS + 1))
    fi
}

run_step() {
    local name="$1"
    shift
    local log="$LOGS/$STAMP-$name.log"
    local started ended seconds code
    printf '\n=== %s ===\n' "$name"
    printf 'cmd: %s\n' "$*"
    printf 'log: %s\n' "$log"
    started=$(date +%s)
    "$@" 2>&1 | tee "$log"
    code=${PIPESTATUS[0]}
    ended=$(date +%s)
    seconds=$((ended - started))
    if [ "$code" -eq 0 ]; then
        printf -- '--- %s: OK (exit 0, %ss)\n' "$name" "$seconds"
    else
        printf -- '--- %s: FAILED (exit %s, %ss)\n' "$name" "$code" "$seconds"
    fi
    record_step "$name" "$code" "$seconds"
}

ROUNDTRIP_ORACLE_ARGS=(
    validation/roundtrip.py
    --backend oracle
    --limit "$LIMIT"
    --timeout "$CASE_TIMEOUT"
    --workflows "$WORKFLOWS"
    --out "$RESULTS/roundtrip-oracle.json"
)
ROUNDTRIP_DEV_ARGS=(
    validation/roundtrip.py
    --backend dev
    --limit "$LIMIT"
    --timeout "$CASE_TIMEOUT"
    --workflows "$WORKFLOWS"
    --out "$RESULTS/roundtrip-dev.json"
)
if [ -n "$SEED" ]; then
    ROUNDTRIP_ORACLE_ARGS+=(--seed "$SEED")
    ROUNDTRIP_DEV_ARGS+=(--seed "$SEED")
fi
if [ "$STRICT" = "1" ]; then
    ROUNDTRIP_ORACLE_ARGS+=(--strict)
    ROUNDTRIP_DEV_ARGS+=(--strict)
fi

run_step "roundtrip-oracle" "$ORACLE_PY" "${ROUNDTRIP_ORACLE_ARGS[@]}"
run_step "roundtrip-dev" "$DEV_PY" "${ROUNDTRIP_DEV_ARGS[@]}"

run_step "compare-roundtrip" "$DEV_PY" \
    validation/compare_roundtrip.py \
    "$RESULTS/roundtrip-oracle.json" \
    "$RESULTS/roundtrip-dev.json" \
    --json-out "$RESULTS/compare-roundtrip.json"

run_step "interop-oracle" "$ORACLE_PY" \
    validation/interop.py \
    --backend oracle \
    --roundtrip "$RESULTS/roundtrip-oracle.json" \
    --out "$RESULTS/interop-oracle.json" \
    --dpi "$DPI"

run_step "interop-dev" "$DEV_PY" \
    validation/interop.py \
    --backend dev \
    --roundtrip "$RESULTS/roundtrip-dev.json" \
    --out "$RESULTS/interop-dev.json" \
    --dpi "$DPI"

run_step "compare-interop" "$DEV_PY" \
    validation/interop.py \
    --cross "$RESULTS/interop-oracle.json" "$RESULTS/interop-dev.json"

# ---------------------------------------------------------------------------
# summary
# ---------------------------------------------------------------------------
{
    printf '\n============================ SUMMARY ============================\n'
    printf 'limit=%s timeout=%ss dpi=%s workflows=%s seed=%s strict=%s\n' \
        "$LIMIT" "$CASE_TIMEOUT" "$DPI" "$WORKFLOWS" "${SEED:-<default>}" "$STRICT"
    printf '%-22s %-8s %s\n' "step" "exit" "seconds"
    index=0
    while [ "$index" -lt "${#STEP_NAMES[@]}" ]; do
        printf '%-22s %-8s %s\n' \
            "${STEP_NAMES[$index]}" "${STEP_CODES[$index]}" "${STEP_TIMES[$index]}"
        index=$((index + 1))
    done
    printf '\nartifacts:\n'
    printf '  %s\n' "$RESULTS/roundtrip-oracle.json"
    printf '  %s\n' "$RESULTS/roundtrip-dev.json"
    printf '  %s\n' "$RESULTS/compare-roundtrip.json"
    printf '  %s\n' "$RESULTS/interop-oracle.json"
    printf '  %s\n' "$RESULTS/interop-dev.json"
    printf '  %s\n' "$RESULTS/interop-cross.json"
    printf 'logs: %s\n' "$LOGS"
    if [ "$FAILED_STEPS" -ne 0 ]; then
        printf '\nPIPELINE: FAIL (%s failed step(s))\n' "$FAILED_STEPS"
    else
        printf '\nPIPELINE: PASS (all steps exited 0; inspect the JSON for per-case failures/warnings and NOT_AVAILABLE tools)\n' 
    fi
} 2>&1 | tee -a "$SUMMARY_LOG"

if [ "$FAILED_STEPS" -ne 0 ]; then
    exit 1
fi
exit 0
