# Pinned selected-tile navigation helper

`benilla-native-navigation` reads one bounded mesh request on stdin and writes one
bounded response on stdout. The protocol-only Rust library has no generator FFI or
native link dependency. `build.rs` links the unchanged pinned Recast/Detour source
and the marked CMaNGOS adapter into the executable only. See [PROTOCOL.md](PROTOCOL.md)
and [CMANGOS-NOTICE.txt](CMANGOS-NOTICE.txt).

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
The exact measured native peak is sufficient; one byte below it must fail cleanly.

`native_peak_bytes` includes native allocation tracking headers, not total RSS.
Input/output and adapter arrays have separate fixed bounds described in the
protocol. Hook payloads are zero initialized to make partially initialized original
owners safe on exception unwind; a registry frees source-local allocations that
would otherwise remain after throwing. No exception crosses the Rust FFI boundary.
The helper handles one request per process, so native allocator globals are isolated.

This establishes Linux generator equivalence for the named authored captures.
It does not establish real-install memory/time requirements, cross-platform bit
parity, runtime-cache publication, world startup, NPC movement policy, or gameplay
admission. The four supported native targets require their own CI runs. On other targets the
protocol library still builds without native compilation or linking; the executable
reports unsupported and exits with code 3 before reading stdin.
