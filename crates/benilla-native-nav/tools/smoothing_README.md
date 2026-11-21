# Exact-source loaded ground-mesh smoothing observer

This test-only observer compiles all four complete, unchanged methods from
CMaNGOS revision `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `PathFinder::fixupCorridor`, `PathFinder.cpp:1141`
- `PathFinder::getSteerTarget`, `PathFinder.cpp:1189`
- `PathFinder::findSmoothPath`, `PathFinder.cpp: 1226`
- `PathFinder::inRangeYZX`, `PathFinder.cpp:1438`

The original conditional macro block from `PathFinder.h` is included unchanged;
`ENABLE_PLAYERBOTS` is forbidden. The generator checks normalized-LF full-file
SHA-256 hashes for both PathFinder files, all 16 original Detour source/header
inputs, and both upstream licenses. `smoothing_source_fragments.test-fixture`
contains the exact extracted methods/constants, original notices, and licenses.
No source method body, original Detour algorithm, or source literal is rewritten.
This is GPL-3.0-or-later CMaNGOS-derived test code; Detour retains its original
zlib-style license and notices.

## Independence and bounded scope

`dtCreateNavMeshData` from the original source builds two entirely authored numeric
ground meshes. The fixture loader then reads the serialized products and invokes
original Detour queries. The existing authored two-tile products are reused
unchanged, with separate input hashes; they are not game assets. The oracle never
links or calls the production bridge, Rust implementation, an application,
installed game data, or a world database.

The minimal PathFinder storage declaration preserves the source constructor's
initial 74-slot smoothing vector even when the selected point limit is smaller.
The source methods are unchanged. A transparent `ObservedQuery` wrapper records
calls and forwards every operation to the actual original `dtNavMeshQuery`.
It supplies no fake query results. The original method still decides all steering,
movement, fixup, height adjustment, endpoint handling, loop exit and status.

Fresh endpoint lookup is an explicit bounded test dependency: direct real
`findNearestPoly` calls with 5 then 10 unit extents, owned ground filter (include=1,
exclude=0), and a 512-node query pool. It does **not** call the source cold
`getPolyByLocation`, whose audited uninitialized distance read is outside this
slice. Direct original `findPath` receives the original, unsnapped endpoints and
uses the selected 2..74 point limit as corridor capacity. Only exact `DT_SUCCESS`
corridors enter the unchanged smoothing method. Rejected partial or truncated
corridors remain separate observations.

There is no `OwnerPathFinder`, route-fact, movement-admission, or whole-source
pathfinding claim here. Source pool 1024, PLAYERBOTS, dungeon-expanded limits up
to 296, off-mesh links, owner policy, shortcutting, optimization, and final height
normalization are not established by these fixtures.

## Native representation and empirical source-lifetime limit

The tested numerical profile is required GCC14.2.0 on `x86_64-linux-gnu`, C++20,
binary32 IEEE-754, little endian, x86-64 baseline/no FMA, no fast math, no FP
contraction, no excess precision, round-to-nearest, and no flush-to-zero/DAZ.
Missing or different compilers fail, rather than skip. A `-fno-inline` build plus
`nm` confirms that all original smoothing methods and real Detour query/writer
symbols are present and production bridge symbols are absent.

Original serialized tiles align sections to four bytes although `dtLink` needs
eight. Without changing a serialized byte, the observer allocates eight extra
bytes and offsets the tile base by four when necessary to align the link section.
All other serialized types have checked four-byte alignment. Tiles are explicitly
removed before their backing allocations are freed.

The default, exact-original lane still has a known C++20 raw-storage lifetime caveat:
`dtMeshTile` declares a private copy constructor and has no default constructor,
while the upstream allocator treats raw storage as tile objects. The production
vendored copy carries its separately documented default-constructor-only safety
amendment. This observer deliberately compiles the unchanged original header;
its compile-time guard confirms the missing default constructor. Those receipts
are empirical exact-source observations under the stated compiler profile, not
proof that every original Detour C++ object lifetime is defined. A clean UBSan
run does not resolve or detect this lifetime issue. Do not conflate the serialized
section-alignment repair with the distinct original class-lifetime caveat.

A separately labeled `--repair-tile-lifetime` companion first verifies exactly the
same full original hashes, then inserts **only** `dtMeshTile() = default;` into the
one verified class declaration in a temporary header copy. The repair is guarded
against missing/duplicate insertion points and prior repair. It changes no field,
algorithm, smoothing method, or serialized byte. Both lanes compare all 15 member
offsets, 104-byte size, eight-byte alignment, standard layout and noncopyability
against an independently renamed copy of the original declaration. The companion
additionally requires trivial default construction and destruction; the original
lane explicitly confirms the constructor is absent. Each receipt records the
variant, exact observed header SHA-256, header copy and compile command. Normal
and UBSan runs in **both** profiles must reproduce all fixture bytes identically.
The amendment closes this specific tile-object lifetime issue only; it is not a
claim that the whole original C++20 program is fully defined.

`-Wno-class-memaccess` suppresses the original Detour warning about its raw-object
initialization idiom; other selected warnings remain errors. The UBSan lane
instruments both the complete observer and every original Detour translation
unit with `undefined,float-cast-overflow` and no recovery.

## Fixture files

All new fixture filenames end in `.test-fixture`, never `.mmap` or `.mmtile`.

- `params.test-fixture`: native 28-byte map parameters
- `ground.test-fixture`: native header plus 25 authored ground polygons, including
  a flat block, eight connected sloped quads, a 15-cell multi-turn corridor, and
  a 400-unit straight block; grid [32,32]
- `thin.test-fixture`: native header plus 40 adjacent .125-unit cells; grid [32,32]
- `cases.test-fixture`: 27 loaded source observations
- `fixup.test-fixture`: 9 standalone source helper observations
- `range.test-fixture`: 12 strict binary32 range-boundary observations

The loaded-case header names every tab-delimited column. All coordinate triples
are **world XYZ**, comma-separated eight-digit hexadecimal binary32 words.
Lists of point triples are separated by semicolons; polygon references are
comma-separated decimal integers. `-` means an empty list. Status fields are
hexadecimal Detour status words. `geometry` selects `ground`, `thin`, or the
existing authored two-tile mesh (`existing`). The latter uses the old fixture
files directly; no new game-like filename is introduced.

Each loaded case records original endpoints, nearest statuses/references/snaps,
nearest attempt counts, direct corridor status and references, boundary-adjusted
endpoints, separate three-cap and 74-cap straight-path probes, raw source smoothing
status and point bits, transparent real-query counts/statuses, and both source
scratch storage and final corridor. The final corridor's active length is recovered
by replaying the **unchanged source fixup helper** on the real recorded visited
arrays; the resulting prefix must exactly equal the source method's scratch
storage. This does not modify or replace the source smoothing method.

`stop` describes the observed source branch: target reached, no steer target,
point limit, or corridor rejection. Source status and stop are deliberately
separate. Exact target arrival at point limit 2 has stop `target_reached` but raw
`DT_FAILURE`. Coincident, vertical-only, and sub-slop targets return raw
`DT_SUCCESS` with only the unchanged original start point.

Important nontrivial controls:

- Unsnapped interior vertical endpoints remain original endpoint heights;
  intermediate heights show source `+0.5` adjustment
- Lower/upper boundary clamping, 5-to-10-unit lookup fallback, multiple slopes and
  original two-tile portal crossings
- Eight full straight-path points versus the source's three-point steering
  buffer: real `DT_SUCCESS | DT_BUFFER_TOO_SMALL` steering remains successful
- Exact-fit two-polygon/limit-2 direct corridor succeeds; a longer two-cap corridor is
  rejected before smoothing
- 40 thin polygons cause 73 actual `moveAlongSurface` visited-buffer truncations per
  direction. The source fixup behavior yields 74-point failure. 37 actual height
  lookups fail per direction, but their output height was already initialized by
  the successful movement and source carries it forward with `+0.5`
- Upper-boundary target also causes one real `getPolyHeight` failure. Source
  ignores this status and continues with the initialized movement result. This
  is an observed defined value carry, not an uninitialized-height claim
- Duplicate visited refs, duplicate path refs, reversed insertion, no common ref,
  tail truncation, and source `req >= maxPath` preservation
- Strict `<` slop/height tests, neighboring binary32 values and signed zero

Actual failed `moveAlongSurface` is outside these generated rows and fails the
observer before using an unsupported output. Successful zero-visited behavior
is accepted by the wrapper but not claimed exercised. No scalar injection is used
to manufacture a loaded-query branch.

## Reproduce

From the repository root, with the exact original checkout as SOURCE:

```sh
python3 crates/benilla-native-nav/tools/smoothing_generate.py SOURCE --check --receipt-dir RECEIPTS/normal
python3 crates/benilla-native-nav/tools/smoothing_generate.py SOURCE --check --sanitize --receipt-dir RECEIPTS/ubsan
python3 crates/benilla-native-nav/tools/smoothing_generate.py SOURCE --check --repair-tile-lifetime --receipt-dir RECEIPTS/lifetime-normal
python3 crates/benilla-native-nav/tools/smoothing_generate.py SOURCE --check --repair-tile-lifetime --sanitize --receipt-dir RECEIPTS/lifetime-ubsan
python3 crates/benilla-native-nav/tools/smoothing_test_generate.py SOURCE
```

Without `--check`, the generator rewrites only its source package and named test
fixtures. Check mode never repairs drift. Receipts include exact compile commands,
source harness, compiler/target/profile, symbols, and output SHA-256 hashes. The
guard suite checks provenance drift, missing source, CRLF normalization, exact
extraction, source locations, compiler failure, marker/package/fixture drift,
actual Detour symbols, bridge exclusion, and the behavior claims above. It does
not run Cargo, rustfmt, app builds, or gameplay.
