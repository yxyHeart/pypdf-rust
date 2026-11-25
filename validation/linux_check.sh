#!/usr/bin/env bash
# Cross-compilation check for the Linux targets.
#
# This only proves that the Rust/PyO3 code compiles for aarch64/x86_64 Linux
# (PYO3_CROSS mode, no linking, no runtime). Building a wheel, installing it and
# running the test suite still require an actual Linux host or a working
# container runtime; see reports/OPTIMIZATION_COMPLETION.md §1 item 10.
#
# Usage: ./validation/linux_check.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

for target in aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
    rustup target add "$target" >/dev/null 2>&1 || true
    echo "=== cargo check --target $target ==="
    env -u PYO3_CROSS_LIB_DIR \
        PYO3_CROSS=1 \
        PYO3_CROSS_PYTHON_VERSION=3.14 \
        cargo check -p pypdf-py --target "$target"
done

echo "LINUX CROSS-CHECK PASSED (compile only; wheel build/install/runtime need a Linux host)"
