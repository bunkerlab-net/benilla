# Pinned selected-tile navigation helper

`benilla-native-navigation` reads one bounded mesh request on stdin and writes one
bounded response on stdout. The protocol-only Rust library has no generator FFI or
native link dependency. `build.rs` links pinned Recast/Detour source with the
explicitly marked BV-ordering adaptation below and the marked CMaNGOS adapter into
the executable only. See [PROTOCOL.md](PROTOCOL.md) and
[CMANGOS-NOTICE.txt](CMANGOS-NOTICE.txt).

The adapter preserves the source 625-subtile rasterize/filter/liquid/compact/erode/
median/distance/regions/contours/poly/detail order, source merge order, 50° steep
marking, polygon area-to-flag mapping, full-map origin-relative tile coordinates,
raw MMap/MMTile output and empty/exactly-625 rejection. Allocation/stage failures
fail the entire request rather than allowing a partially failed source subtile.
The process owns no source, cache or install paths. The parent must enforce a hard
process timeout and cancellation/kill/reap, bind source/config/off-mesh identities,
and independently admit both output files through the production Detour reader.

Off-mesh input is deliberately unsupported; the caller must reject a selected
configured connection rather than omit it. Config bytes remain the pinned original
config/offmesh files. Numeric source configuration fields are explicit request
values, already resolved with source merge/truncation semantics. Bounded supported
config ranges and allocation limits can reject a source input; that is an explicit
resource/coverage failure, never permission to simplify the source geometry.

## Generator compatibility policy: stable BV ordering v1

The generator's `DetourNavMeshBuilder.cpp` replaces libc `qsort` with stable
merge sorting at each BV-tree split. Equal minima on the selected axis retain the
incoming subrange order, including earlier parent-axis sorts; polygon IDs are not
an extra sort key. Quantization, longest-axis selection, median split, node bounds,
escape indices and serialization are unchanged. This deliberately specifies an
order that upstream leaves to libc. It matches the existing authored Linux oracle
captures; it does not claim equivalence to every libc's unmodified `qsort`.
The production reader and independent original capture producer are unchanged.

A single reusable `polyCount * sizeof(BVItem)` scratch allocation uses the same
tracked `dtAlloc(DT_ALLOC_TEMP)` budget as the existing item array. Both have checked
size arithmetic and `dtFree` ownership, including exception unwind. Null allocation
failure now propagates through tile creation without publishing partial output;
the process's existing registry reclaims source-local allocations on throwing hooks.
The int-sized node/output fields reject unrepresentable counts before multiplication.
These are safety checks, not a new 65,535-polygon limit.

Sorting uses O(n log n) work per split, O(n log² n) over the balanced tree, O(n)
extra tracked memory and O(log n) stack. On supported ABIs each item is 16 bytes;
the two item arrays alone require 32n bytes plus two tracking headers. The existing
256 MiB hard budget therefore permits strictly fewer than 8,388,608 items (and
other live allocations reduce that further), bounding each root-to-leaf path to
at most 23 splits.
The parent still enforces the process timeout. Scratch can make an otherwise
insufficient allocation budget fail; exact-peak and one-byte-less tests retain this
explicit resource-failure contract.

The source-product identity is versioned as navigation v2 / stable-BV v1 so
regenerated products cannot reuse the previous policy identity. Protocol v1 framing
and source config bytes do not change. Historical products retain their own identity.

## Verification

Build and execute the real helper process (no conditional skip when missing):

    cargo test -p benilla-native-nav-generator
    cargo clippy -p benilla-native-nav-generator --all-targets -- -D warnings
    cargo build -p benilla-native-nav-generator --bin benilla-native-navigation
    python3 crates/benilla-native-nav-generator/tests/check_original.py \
      --helper target/debug/benilla-native-navigation

The Python comparison uses independently compiled original CMaNGOS MapBuilder
captures under `tools/native-navigation/fixtures`, checks exact MMap/MMTile bytes
for nontrivial authored ridge plus WMO and ridge plus cardinal neighbors plus WMO,
and verifies the original exactly-625 skip for the terrain-only ridge. It reports
counts, time and native allocation peak. The original capture
producer does not use this helper. Rust process tests additionally exercise empty
sentinels, exact-625 flat rejection, a nontrivial hole, deterministic regeneration,
malformed framing/config/indices, and budget failures at early and late allocations.
A small native regression compiles the actual generator builder and covers stable
ties across recursive axis changes, no-tie controls, all allocation-failure points,
exact scratch-budget failure/retry, and exact reconstruction of the unchanged
fixture BV trees. Generated-byte failures keep full equality checks and report
lengths, digests and at most eight first differing offsets rather than full arrays.
The exact measured native peak is sufficient; one byte below it must fail cleanly.

`native_peak_bytes` includes native allocation tracking headers, not total RSS.
Input/output and adapter arrays have separate fixed bounds described in the
protocol. Hook payloads are zero initialized to make partially initialized original
owners safe on exception unwind; a registry frees source-local allocations that
would otherwise remain after throwing. No exception crosses the Rust FFI boundary.
The helper handles one request per process, so native allocator globals are isolated.

This checks exact generator output against the named authored Linux captures.
It does not establish real-install memory/time requirements, cross-platform bit
parity, runtime-cache publication, world startup, NPC movement policy, or gameplay
admission. The four supported native targets require their own CI runs. On other targets the
protocol library still builds without native compilation or linking; the executable
reports unsupported and exits with code 3 before reading stdin.
