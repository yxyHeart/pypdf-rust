#!/usr/bin/env bash
# Build the pypdf Rust port and install it into .venvs/dev (editable).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

VIRTUAL_ENV="$ROOT/.venvs/dev" \
PYO3_PYTHON="$ROOT/.venvs/dev/bin/python" \
    .venvs/dev/bin/maturin develop --release -m crates/pypdf-py/Cargo.toml "$@"

"$ROOT/.venvs/dev/bin/python" - <<'PY'
import pypdf
assert pypdf.__version__ == "7.1.1", pypdf.__version__
assert "site-packages" in pypdf.__file__ or pypdf.__file__.endswith("python/pypdf/__init__.py"), pypdf.__file__
print(f"installed pypdf {pypdf.__version__} from {pypdf.__file__}")
PY
