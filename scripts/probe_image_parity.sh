#!/usr/bin/env bash
# P0 hard gate (re-runnable offline): can Rust reproduce Pillow's encoder output
# byte-for-byte?
#
# Decisive questions answered before any porting work:
#   1. JPEG: libjpeg-turbo (vendored 3.2.1, defaults + quality 75) must emit the
#      exact bytes Pillow emits.
#   2. PNG: zlib-ng (vendored 2.3.3, ZLIB_COMPAT=ON, default build options) with
#      level 6 / windowBits 15 / memLevel 9 / Z_FILTERED and row-by-row feeding
#      must reproduce the exact IDAT payload Pillow emitted.
#
# The vendored sources are the ones the Rust build will link, so a PASS here
# means byte-level image parity is achievable in the final artifact.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/.audit/imgprobe"
BUILD="$ROOT/.audit/build"
PY="$ROOT/.venvs/oracle/bin/python"
JOBS="$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"

mkdir -p "$OUT" "$BUILD"

echo "=== 1/6 generate Pillow reference output ==="
"$PY" "$ROOT/tools/probe/image_parity.py" --out "$OUT" > "$OUT/probe.log" 2>&1 || {
    echo "FAIL: image_parity.py"; tail -20 "$OUT/probe.log"; exit 1; }
grep -E '"(jpeg_len|png_len|dct_len|idat_len|filtered_len|jpeg_matches_pdf_dct)"' "$OUT/probe.json"

echo "=== 2/6 build vendored zlib-ng 2.3.3 (ZLIB_COMPAT) ==="
if [ ! -f "$BUILD/zlib-ng/libz.a" ]; then
    cmake -S "$ROOT/vendor/zlib-ng" -B "$BUILD/zlib-ng" \
        -DZLIB_COMPAT=ON -DBUILD_SHARED_LIBS=OFF \
        -DWITH_GTEST=OFF -DWITH_TESTS=OFF -DWITH_BENCHMARKS=OFF \
        > "$BUILD/zlib-ng-configure.log" 2>&1
    cmake --build "$BUILD/zlib-ng" -j "$JOBS" > "$BUILD/zlib-ng-build.log" 2>&1
fi
[ -f "$BUILD/zlib-ng/libz.a" ] || { echo "FAIL: zlib-ng build"; tail -20 "$BUILD/zlib-ng-build.log"; exit 1; }

echo "=== 3/6 build vendored libjpeg-turbo 3.2.1 ==="
if [ ! -f "$BUILD/libjpeg-turbo/libjpeg.a" ]; then
    cmake -S "$ROOT/vendor/libjpeg-turbo" -B "$BUILD/libjpeg-turbo" \
        -DENABLE_SHARED=OFF -DENABLE_STATIC=ON -DWITH_TURBOJPEG=OFF \
        -DWITH_JAVA=OFF -DWITH_TOOLS=OFF -DWITH_TESTS=OFF \
        > "$BUILD/jpeg-configure.log" 2>&1
    cmake --build "$BUILD/libjpeg-turbo" -j "$JOBS" > "$BUILD/jpeg-build.log" 2>&1
fi
[ -f "$BUILD/libjpeg-turbo/libjpeg.a" ] || { echo "FAIL: libjpeg-turbo build"; tail -20 "$BUILD/jpeg-build.log"; exit 1; }

echo "=== 4/6 compile probes against vendored static libs ==="
cc -O2 "$ROOT/tools/probe/jpeg_probe.c" \
    -I"$ROOT/vendor/libjpeg-turbo/src" -I"$BUILD/libjpeg-turbo" \
    "$BUILD/libjpeg-turbo/libjpeg.a" -o "$OUT/jpeg_probe" || { echo "FAIL: jpeg probe build"; exit 1; }
cc -O2 "$ROOT/tools/probe/zlib_probe.c" \
    -I"$BUILD/zlib-ng" -I"$ROOT/vendor/zlib-ng" \
    "$BUILD/zlib-ng/libz.a" -o "$OUT/zlib_probe" || { echo "FAIL: zlib probe build"; exit 1; }

echo "=== 5/6 JPEG parity (quality 75, Pillow defaults) ==="
"$OUT/jpeg_probe" "$OUT/raw_rgb.bin" 64 48 75 "$OUT/vendored.jpg"
jpeg_status=1
if cmp -s "$OUT/vendored.jpg" "$OUT/pillow.jpg"; then
    echo "PASS: JPEG bytes identical to Pillow"
    jpeg_status=0
else
    echo "FAIL: JPEG bytes differ from Pillow"
    ls -l "$OUT/vendored.jpg" "$OUT/pillow.jpg"
fi
grep -q '"jpeg_matches_pdf_dct": true' "$OUT/probe.json" \
    && echo "PASS: Pillow's PDF plugin embeds exactly the standalone JPEG stream" \
    || echo "WARN: PDF plugin stream differs from standalone JPEG"

echo "=== 6/6 PNG IDAT parity (vendored zlib-ng, Pillow parameters) ==="
# Pillow's ZipEncodeInit: level=compress_level(6), windowBits=15, memLevel=9,
# strategy=Z_FILTERED(1) for ZIP_PNG, one row (stride) per deflate(Z_NO_FLUSH).
row=$(( $(python3 -c "import json;d=json.load(open('$OUT/probe.json'));print(d['filtered_len']//d['size'][1])") ))
"$OUT/zlib_probe" "$OUT/png_filtered.bin" 6 15 9 1 "$OUT/vendored_idat.bin" "$row" >/dev/null
png_status=1
if cmp -s "$OUT/vendored_idat.bin" "$OUT/pillow_idat.bin"; then
    echo "PASS: PNG IDAT identical to Pillow (row size $row)"
    png_status=0
else
    echo "FAIL: PNG IDAT differs (row size $row)"
    ls -l "$OUT/vendored_idat.bin" "$OUT/pillow_idat.bin"
fi

echo
echo "record: JPEG $( [ $jpeg_status -eq 0 ] && echo PASS || echo FAIL ), PNG $( [ $png_status -eq 0 ] && echo PASS || echo FAIL )" \
    > "$OUT/parity_result.txt"
if [ "$jpeg_status" -eq 0 ] && [ "$png_status" -eq 0 ]; then
    echo "IMAGE PARITY PROBE: PASSED (byte-identical JPEG + PNG encoders)"
    exit 0
fi
echo "IMAGE PARITY PROBE: FAILED (jpeg=$jpeg_status png=$png_status)"
exit 1
