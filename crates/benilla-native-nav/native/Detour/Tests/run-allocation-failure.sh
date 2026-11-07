#!/usr/bin/env bash
# Standalone synthetic fault-allocation coverage; no Cargo or game data.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
build=$(mktemp -d "${TMPDIR:-/tmp}/benilla-detour-allocation.XXXXXX")
trap 'rm -rf "$build"' EXIT
"${CXX:-g++}" -std=c++20 -DDT_POLYREF64 -O1 -g \
  -fsanitize=address,undefined -fno-omit-frame-pointer \
  -I"$root/Include" "$root/Tests/ObjectLifetime.cpp" \
  -o "$build/object-lifetime"
"$build/object-lifetime"
"${CXX:-g++}" -std=c++20 -DDT_POLYREF64 -O1 -g \
  -fsanitize=address,undefined -fno-omit-frame-pointer \
  -I"$root/Include" \
  "$root/Tests/AllocationFailure.cpp" \
  "$root/Source/DetourAlloc.cpp" \
  "$root/Source/DetourAssert.cpp" \
  "$root/Source/DetourCommon.cpp" \
  "$root/Source/DetourNavMesh.cpp" \
  "$root/Source/DetourNavMeshQuery.cpp" \
  "$root/Source/DetourNode.cpp" \
  -o "$build/allocation-failure"
"$build/allocation-failure"
