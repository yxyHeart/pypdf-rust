#!/usr/bin/env bash
# Gate A7: structure-aware mutation fuzzing, compared oracle-vs-port.
#
# COUNT=200 ./scripts/fuzz.sh          # quick pass
# COUNT=20000 ./scripts/fuzz.sh        # deep pass (slow)
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
COUNT="${COUNT:-200}"
SEED="${SEED:-20261124}"
OUT=".audit/fuzz"
ORACLE=".venvs/oracle/bin/python"
DEV=".venvs/dev/bin/python"
mkdir -p "$OUT"

rm -rf "$OUT/corpus" "$OUT/oracle.json" "$OUT/dev.json"
"$ORACLE" tools/fuzz/gen_corpus.py --out "$OUT/corpus" --count "$COUNT" --seed "$SEED" || exit 1

PYTHONHASHSEED=0 "$ORACLE" tools/fuzz/run.py --corpus "$OUT/corpus" --out "$OUT/oracle.json" || exit 1
PYTHONHASHSEED=0 "$DEV"    tools/fuzz/run.py --corpus "$OUT/corpus" --out "$OUT/dev.json"    || exit 1

"$ORACLE" tools/oracle/compare.py "$OUT/oracle.json" "$OUT/dev.json"
