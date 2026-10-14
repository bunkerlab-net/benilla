#!/usr/bin/env bash
# Standalone Linux native-boundary checks; requires canonical rustc, C++ compiler, ASan/UBSan.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
build=$(mktemp -d "${TMPDIR:-/tmp}/benilla-detour-boundary.XXXXXX")
trap 'rm -rf "$build"' EXIT
sources=("$root/native/bridge.cpp")
for name in DetourAlloc DetourAssert DetourCommon DetourNavMesh DetourNavMeshQuery DetourNode; do sources+=("$root/native/Detour/Source/$name.cpp"); done
for source in "${sources[@]}"; do
  "${CXX:-c++}" -std=c++20 -O1 -g -DDT_POLYREF64=1 -fno-fast-math -ffp-contract=off \
    -fsanitize=address,undefined -fno-omit-frame-pointer -I"$root/native/Detour/Include" \
    -c "$source" -o "$build/$(basename "$source").o"
done
ar crs "$build/libnav_sanitized.a" "$build"/*.o
rustc --edition 2021 --test --cfg benilla_native_detour "$root/src/lib.rs" -L "$build" \
  -l static=nav_sanitized -l stdc++ -l asan -l ubsan -o "$build/tests"
# Rust itself is not sanitizer-instrumented; every native source/bridge access is.
LD_PRELOAD="$("${CXX:-c++}" -print-file-name=libasan.so)" ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0}" "$build/tests" --test-threads=1
