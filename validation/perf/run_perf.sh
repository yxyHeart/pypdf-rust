#!/usr/bin/env bash
# §10: run the performance validation end to end.
#
# The reference (PyPI pypdf 7.1.1) and the port (pypdf-rust) are measured in two
# sequential, separate processes so that neither imports, caches nor allocator
# state can leak into the other side.  A comparison step then writes
# validation/results/perf-report.md.
#
# Usage:
#   validation/perf/run_perf.sh [--repeat N] [--timeout SECONDS] [--only W1,W3]
#
# Environment:
#   PERF_REPEAT   default 10 (plan recommends 10, requires >= 5)
#   PERF_TIMEOUT  per-repetition timeout in seconds, default 300
#
# Results: validation/results/perf-reference.json
#          validation/results/perf-rust.json
#          validation/results/perf-report.md
# Logs:    validation/results/logs/perf-*.log
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RESULTS="$ROOT/validation/results"
LOGS="$RESULTS/logs"
ORACLE_PY="$ROOT/.venvs/oracle/bin/python"
DEV_PY="$ROOT/.venvs/dev/bin/python"

REPEAT="${PERF_REPEAT:-10}"
TIMEOUT="${PERF_TIMEOUT:-300}"
ONLY=""

usage() {
    sed -n '2,18p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --repeat) REPEAT="$2"; shift 2 ;;
        --timeout) TIMEOUT="$2"; shift 2 ;;
        --only) ONLY="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "run_perf.sh: unknown argument: $1" >&2; exit 2 ;;
    esac
done

if ! [[ "$REPEAT" =~ ^[0-9]+$ ]] || (( REPEAT < 1 )); then
    echo "run_perf.sh: --repeat must be a positive integer" >&2
    exit 2
fi
if (( REPEAT < 5 )); then
    echo "run_perf.sh: warning: --repeat $REPEAT is below the plan minimum of 5" >&2
fi
for python in "$ORACLE_PY" "$DEV_PY"; do
    if [[ ! -x "$python" ]]; then
        echo "run_perf.sh: missing venv interpreter $python" >&2
        exit 2
    fi
done

mkdir -p "$RESULTS" "$LOGS"

# Fixed hash seed and no bytecode writes on both sides (plan §10.1: same inputs,
# same parameters).  Both are exported before any process starts.
export PYTHONHASHSEED=0
export PYTHONDONTWRITEBYTECODE=1

RUN_ARGS=(--repeat "$REPEAT" --timeout "$TIMEOUT")
if [[ -n "$ONLY" ]]; then
    RUN_ARGS+=(--only "$ONLY")
fi

{
    echo "# performance validation environment"
    echo "# started: $(date -u '+%Y-%m-%dT%H:%M:%SZ') (local: $(date '+%Y-%m-%dT%H:%M:%S%z'))"
    echo "# host: $(hostname)"
    echo "# uname: $(uname -a)"
    if command -v sysctl >/dev/null 2>&1; then
        echo "# cpu: $(sysctl -n machdep.cpu.brand_string 2>/dev/null || sysctl -n hw.model 2>/dev/null || echo unknown)"
        echo "# memsize: $(sysctl -n hw.memsize 2>/dev/null || echo unknown)"
    fi
    echo "# repeat=$REPEAT timeout=$TIMEOUT only=${ONLY:-<all>} PYTHONHASHSEED=$PYTHONHASHSEED"
    echo "# python reference: $("$ORACLE_PY" -c 'import sys; print(sys.version.split()[0])')"
    echo "# python port:      $("$DEV_PY" -c 'import sys; print(sys.version.split()[0])')"
    echo "# pypdf reference:  $("$ORACLE_PY" -c 'import pypdf; print(pypdf.__version__, pypdf.__file__)' 2>/dev/null)"
    echo "# pypdf port:       $("$DEV_PY" -c 'import pypdf; print(pypdf.__version__, pypdf.__file__)' 2>/dev/null)"
} > "$LOGS/perf-env.log"
cat "$LOGS/perf-env.log"

overall=0

run_step() {
    # run_step <label> <logfile> <command...>
    local label="$1" log="$2"; shift 2
    {
        echo ""
        echo "===== $label ====="
        echo "# command: $*"
        echo "# started: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
        echo "# uptime: $(uptime)"
    } >> "$log"
    "$@" 2>&1 | tee -a "$log"
    local status="${PIPESTATUS[0]}"
    {
        echo "# exit: $status"
        echo "# finished: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
        echo "# uptime: $(uptime)"
    } >> "$log"
    if (( status != 0 )); then
        echo "run_perf.sh: $label FAILED (exit $status), see $log" >&2
        overall=1
    fi
    return 0
}

inject_machine_state() {
    # Record the shell-visible machine state around a backend run inside its JSON.
    local json="$1" before="$2" after="$3"
    [[ -f "$json" ]] || return 0
    "$ORACLE_PY" - "$json" "$before" "$after" <<'PY'
import json
import sys

path, before, after = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, "r", encoding="utf-8") as handle:
    document = json.load(handle)
document["machine_state_shell"] = {"uptime_before": before, "uptime_after": after}
with open(path, "w", encoding="utf-8") as handle:
    json.dump(document, handle, indent=1, ensure_ascii=False)
    handle.write("\n")
PY
}

extension_changed() {
    # exit 0 when the JSON reports that the compiled extension changed mid-run
    local json="$1"
    [[ -f "$json" ]] || return 1
    "$ORACLE_PY" - "$json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    document = json.load(handle)
changed = bool((document.get("environment") or {}).get("extension_changed_during_run"))
raise SystemExit(0 if changed else 1)
PY
}

# 1) reference, alone in its process
UPTIME_BEFORE="$(uptime)"
run_step "reference (pypdf 7.1.1 from PyPI, .venvs/oracle)" "$LOGS/perf-reference.log" \
    "$ORACLE_PY" "$ROOT/validation/perf/run.py" --backend reference "${RUN_ARGS[@]}" \
    --out "$RESULTS/perf-reference.json"
UPTIME_AFTER="$(uptime)"
inject_machine_state "$RESULTS/perf-reference.json" "$UPTIME_BEFORE" "$UPTIME_AFTER"

# 2) port, alone in its process
UPTIME_BEFORE="$(uptime)"
run_step "port (pypdf-rust, .venvs/dev)" "$LOGS/perf-rust.log" \
    "$DEV_PY" "$ROOT/validation/perf/run.py" --backend rust "${RUN_ARGS[@]}" \
    --out "$RESULTS/perf-rust.json"
UPTIME_AFTER="$(uptime)"
inject_machine_state "$RESULTS/perf-rust.json" "$UPTIME_BEFORE" "$UPTIME_AFTER"

# A cargo build by another process can replace the extension while the port run
# is in flight; the measurements would then be a mixed build.  Re-run once.
if extension_changed "$RESULTS/perf-rust.json"; then
    echo "run_perf.sh: the extension changed during the port run; re-running the port once" >&2
    UPTIME_BEFORE="$(uptime)"
    run_step "port (retry after mid-run extension rebuild)" "$LOGS/perf-rust-retry.log" \
        "$DEV_PY" "$ROOT/validation/perf/run.py" --backend rust "${RUN_ARGS[@]}" \
        --out "$RESULTS/perf-rust.json"
    UPTIME_AFTER="$(uptime)"
    inject_machine_state "$RESULTS/perf-rust.json" "$UPTIME_BEFORE" "$UPTIME_AFTER"
    if extension_changed "$RESULTS/perf-rust.json"; then
        echo "run_perf.sh: extension changed again during the retry; the port numbers are a mixed build" >&2
        overall=1
    fi
fi

# 3) comparison
run_step "compare" "$LOGS/perf-compare.log" \
    "$ORACLE_PY" "$ROOT/validation/perf/compare.py" \
    --ref "$RESULTS/perf-reference.json" --rust "$RESULTS/perf-rust.json" \
    --out "$RESULTS/perf-report.md"

echo ""
echo "run_perf.sh: results:"
echo "  $RESULTS/perf-reference.json"
echo "  $RESULTS/perf-rust.json"
echo "  $RESULTS/perf-report.md"
echo "  $LOGS/perf-reference.log"
echo "  $LOGS/perf-rust.log"
echo "  $LOGS/perf-compare.log"
if (( overall != 0 )); then
    echo "run_perf.sh: FAILED (see logs above)" >&2
    exit 1
fi
echo "run_perf.sh: PASS"
