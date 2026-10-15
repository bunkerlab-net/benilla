#!/usr/bin/env bash
# Standalone Linux native-boundary checks; requires C++ compiler and ASan/UBSan.
# Pass --native-only to run C++ bounds/fault checks without invoking rustc.
set -euo pipefail
case "${1:-}" in
  ""|--native-only) ;;
  *) echo "usage: $0 [--native-only]" >&2; exit 2 ;;
esac
root=$(cd "$(dirname "$0")/.." && pwd)
build=$(mktemp -d "${TMPDIR:-/tmp}/benilla-detour-boundary.XXXXXX")
trap 'rm -rf "$build"' EXIT
sources=("$root/native/bridge.cpp" "$root/native/source_smoothing.cpp" "$root/native/source_direct.cpp" "$root/native/source_warm_cache.cpp" "$root/native/source_point_path.cpp" "$root/native/source_straight_path.cpp")
for name in DetourAlloc DetourAssert DetourCommon DetourNavMesh DetourNavMeshQuery DetourNode; do sources+=("$root/native/Detour/Source/$name.cpp"); done
for source in "${sources[@]}"; do
  "${CXX:-c++}" -std=c++20 -O1 -g -DDT_POLYREF64=1 -fno-fast-math -ffp-contract=off \
    -fsanitize=address,undefined,float-cast-overflow -fno-sanitize-recover=all -fno-omit-frame-pointer -I"$root/native/Detour/Include" \
    -c "$source" -o "$build/$(basename "$source").o"
done
ar crs "$build/libnav_sanitized.a" "$build"/*.o
# Fault-injection overrides are confined to this independent native executable.
"${CXX:-c++}" -std=c++20 -O1 -g -DDT_POLYREF64=1 -fno-fast-math -ffp-contract=off \
  -fsanitize=address,undefined,float-cast-overflow -fno-sanitize-recover=all -fno-omit-frame-pointer -I"$root/native/Detour/Include" \
  -I"$root/native" "$root/tools/direct_corridor_allocation_test.cpp" \
  "$build/libnav_sanitized.a" -o "$build/direct-allocation"
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0}" "$build/direct-allocation" "$root/tools/fixtures"
"${CXX:-c++}" -std=c++20 -O1 -g -DDT_POLYREF64=1 -fno-fast-math -ffp-contract=off \
  -fsanitize=address,undefined,float-cast-overflow -fno-sanitize-recover=all -fno-omit-frame-pointer -I"$root/native/Detour/Include" \
  -I"$root/native" "$root/tools/straight_path_boundary_test.cpp" \
  "$build/libnav_sanitized.a" -o "$build/straight-boundary"
ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0}" "$build/straight-boundary" "$root/tools/fixtures"
if [[ "${1:-}" == --native-only ]]; then exit 0; fi
rustc --edition 2021 --test --cfg benilla_native_detour "$root/src/lib.rs" -L "$build" \
  -l static=nav_sanitized -l stdc++ -l asan -l ubsan -o "$build/tests"
# Rust itself is not sanitizer-instrumented; every native source/bridge access is.
LD_PRELOAD="$("${CXX:-c++}" -print-file-name=libasan.so)" ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0}" "$build/tests" --test-threads=1
