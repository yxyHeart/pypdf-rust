#!/usr/bin/env bash
# Regenerate every raw measurement used by diag_reader.md / diag_writer.md.
#
#   bash validation/optimization/scripts/run_all.sh
#
# Runs one backend at a time (never concurrently: the machine is shared and the
# two sides must see comparable load).  Each backend produces exactly one JSON in
# validation/optimization/raw/.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

export PYTHONHASHSEED=0
WORKLOADS="${WORKLOADS:-W1,W2,W5,W6,W7}"
REPEAT="${REPEAT:-7}"
RAW="validation/optimization/raw"
mkdir -p "$RAW"

echo "=== port (Rust) $(date +%H:%M:%S) load=$(sysctl -n vm.loadavg 2>/dev/null || cat /proc/loadavg)"
.venvs/dev/bin/python validation/optimization/scripts/rw_diag.py \
    --backend rust --workloads "$WORKLOADS" --mode all --repeat "$REPEAT" \
    --out "$RAW/rw_diag_rust.json"
.venvs/dev/bin/python validation/optimization/scripts/rw_microbench.py \
    --backend rust --out "$RAW/rw_micro_rust.json"

echo "=== reference $(date +%H:%M:%S) load=$(sysctl -n vm.loadavg 2>/dev/null || cat /proc/loadavg)"
.venvs/oracle/bin/python validation/optimization/scripts/rw_diag.py \
    --backend reference --workloads "$WORKLOADS" --mode all --repeat "$REPEAT" \
    --out "$RAW/rw_diag_reference.json"
.venvs/oracle/bin/python validation/optimization/scripts/rw_microbench.py \
    --backend reference --out "$RAW/rw_micro_reference.json"

echo "=== done $(date +%H:%M:%S)"
echo "Tables:"
echo "  .venvs/dev/bin/python validation/optimization/scripts/render_tables.py \\"
echo "      --rust $RAW/rw_diag_rust.json --reference $RAW/rw_diag_reference.json \\"
echo "      --microbench-rust $RAW/rw_micro_rust.json --microbench-reference $RAW/rw_micro_reference.json \\"
echo "      --section reader"
