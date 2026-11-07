# Independent exact-source loaded tile-presence oracle

This GPL-covered test observer is pinned to CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It compiles the complete unchanged
`PathFinder::HaveTile` body at `PathFinder.cpp:1122` and all three complete
scalar/two-axis/three-axis `MaNGOS::IsValidMapCoord` bodies at
`GridDefines.h:177–190`. Their original coordinate macros are also extracted
unchanged. It links the actual original Detour sources and loads existing,
source-writer-generated authored numeric meshes. No production bridge is linked.

## Evidence and ownership boundary

This is only **nontransport tile presence and coordinate validity**. It is not
complete `calculate`, route admission, pathfinding, a world/terrain owner, source
owner construction, or complete gameplay evidence. Complete original `calculate`
is packaged for review, not compiled. Its destination-before-start validation
and start-before-end HaveTile short-circuit determine the observer ordering.
The remaining `calculate` conditions and side effects are outside this slice.

The small Vector3/PathFinder/Unit declarations are explicitly authored wrappers.
`m_sourceUnit` is always null, so the unchanged HaveTile body short-circuits before
`GetTransport`; the diagnostic-only method aborts if called. This supplies no
invented owner, owner callback result, or source ownership claim. The original
non-PLAYERBOTS calculate owner gate would reject null; the observer deliberately
does not execute calculate or claim it would accept this wrapper.

The HaveTile body maps world XYZ to native YZX, calls actual `calcTileLoc`, rejects
exactly `tx == -1 || ty == -1`, and queries actual layer zero. Observations also
call actual `calcTileLoc` and `getTileAt` directly, then call unchanged HaveTile;
the wrapper does not script their results. An end HaveTile observation is absent
when the start HaveTile result is false. Neither caller-declared load-grid keys,
nearest-polygon success, polygon bounds, heights nor arbitrary negative-index
rejection replaces the original tile lookup.

`pair_valid` is the original source coordinate predicate. `native_safe` is a
separate bounded observer/production domain: finite coordinates, safe binary32
horizontal division/floor-to-int, and absolute world height at most 100000.
Raw far-height rows intentionally call original HaveTile beyond the bounded
height domain because it never uses height in tile coordinates. These rows are
not evidence that the production boundary accepts that height. Source-invalid
pairs never enter HaveTile, even when their finite native arithmetic would be safe.

## Inputs and authored variants

`tile_presence_source_hashes.sha256` binds every compiled original Detour source
and header, both upstream license files, PathFinder.cpp/.h, and GridDefines.h.
CRLF is normalized to LF before original-source hash checks; content drift fails.
`tile_presence_input_hashes.sha256` binds existing `authored.mmap`,
`0003232.mmtile` (left), and `0003231.mmtile` (right). These are authored numeric
products written by original `dtCreateNavMeshData`; no install data, world DB,
download, runtime importer, or network is involved.

The observer emits five explicit authored variants of those input bytes:

- `params-minus-one.test-fixture`: only native origin X becomes +GRID
- `params-minus-two.test-fixture`: only native origin X becomes +2*GRID
- `tile-minus-one.test-fixture`: only source-writer left tile header X becomes -1
- `tile-minus-two.test-fixture`: only that header X becomes -2
- `tile-layer-one.test-fixture`: only that header layer becomes 1

GRID here is the existing authored fixture literal `533.3333f`; the source
coordinate macro is spelled `533.33333f`. Both round to binary32 `0x44055555`.
No mesh geometry is changed. At world
(-400,1,0), the shifted rows observe a present tile at -1 that HaveTile rejects,
and a present tile at -2 that HaveTile accepts. The layer-one row confirms an
actually loaded layer 1 does not satisfy layer-zero presence. Empty and layer-one
meshes are raw controls outside the validated production loader's admitted domain.
All new binary filenames use the non-admitted `.test-fixture` suffix.

## Schema and witnesses

`fixtures/tile-presence/cases.test-fixture` has one tab-separated header:

`label geometry scope from to destination_valid start_valid pair_valid native_safe
start_checked start_tx start_ty start_layer0 start_have end_checked end_tx end_ty
end_layer0 end_have pair_have nearest_status nearest_ref`

World XYZ coordinates are three comma-separated eight-digit binary32 words,
preserving signed zero, tiny subnormals and nonfinite input words. Integer tile
coordinates and references are decimal. Booleans are 0/1. A dash means a field
was not consulted, not false or tile zero. The end validity check precedes the
start validity check; invalid destination leaves start_valid as a dash. Endpoint
records are in start/end order; skipped endpoints have checked=0 and all remaining
endpoint fields as dashes. `pair_have` is true only when both actual calls succeed.
`scope` is runtime or raw; raw empty/layer-one meshes are loader controls, and raw
far heights show source behavior beyond the bounded production height domain.

Rows cover both/left/right/empty loading, present/missing start and end, axis
permutation, far finite height, both horizontal source-map edges and adjacent
binary32 values, both native tile axes at -GRID/0/+GRID and adjacent values,
NaN/infinite coordinates, signed zero and ±minimum subnormals. World-y words
`b8000000` and `b7ffffff` straddle the actual native float-subtraction rounding
edge near -2^-15 and produce different native tile X despite both being negative.
For the left-only world (-400,1,0) witness, actual original `findNearestPoly` with
extent [5,5,5] succeeds with a nonzero loaded polygon while HaveTile is false.
Nearest fields are absent for other rows; nearest is never used as presence proof.

## Compiler, floating environment, and lifetime lanes

The audited profile is GCC 14.2.0, x86_64-linux-gnu, C++20, 64-bit polygon
references, IEEE binary32, FE_TONEAREST and SSE nearest rounding with FTZ/DAZ off.
Flags disable fast math, contraction and FMA and enable dynamic rounding awareness.
`FLT_EVAL_METHOD == 0` is required; an intentional `-mfpmath=387` compile is rejected.
Both `fegetround` and MXCSR are checked before observation; deliberate rounding,
SSE rounding, FTZ and DAZ mutations must fail before any output. Missing or other
compiler profiles fail closed. Symbol guards require original HaveTile, all three
coordinate methods and actual Detour primitives, and reject production bridge
symbols. The oracle profile does not claim untested targets or compilers.

Original Detour allocates raw `dtMeshTile` storage although its declaration is not
default constructible or an aggregate in C++20. The unchanged original lane
therefore retains its known object-lifetime caveat: reproducible bytes and clean
UBSan are empirical observations, not a proof of standards-defined lifetime.
`--repair-tile-lifetime` inserts only `dtMeshTile() = default;`. Both lanes compare
size, alignment, every member offset and noncopyability against the original
layout. The repaired lane must reproduce every fixture byte; no HaveTile,
coordinate, primitive or geometry logic changes. Loaded tiles are removed before
their borrowed buffers are freed. Neither lifetime lane establishes gameplay.

## Reproduce and verify

With an existing pinned local source checkout, run from the repository root:

```sh
SOURCE=/path/to/mangos-classic-8ec338a1704e7dcb1c0213eb7ed58f9231ade40f
TOOLS=crates/benilla-native-nav/tools
python3 "$TOOLS/tile_presence_generate.py" "$SOURCE" --check --receipt-dir /tmp/tile-original-normal
python3 "$TOOLS/tile_presence_generate.py" "$SOURCE" --check --sanitize --receipt-dir /tmp/tile-original-ubsan
python3 "$TOOLS/tile_presence_generate.py" "$SOURCE" --check --repair-tile-lifetime --receipt-dir /tmp/tile-repaired-normal
python3 "$TOOLS/tile_presence_generate.py" "$SOURCE" --check --repair-tile-lifetime --sanitize --receipt-dir /tmp/tile-repaired-ubsan
python3 "$TOOLS/tile_presence_test_generate.py" "$SOURCE"
```

Omit `--check` only to regenerate products from verified source. Receipts include
extracted harness, symbols, observed tile header, compiler command/profile,
source/input manifests and output hashes. Tests cover exact methods/licenses,
source and input drift, malformed manifests/schemas/domains, short-circuit order,
constructor-only repair, environment mutations, compiler/symbol failures,
output drift without rewrite, unsafe output names and a real core.autocrlf=true
Git add/checkout-index round trip. No commit or source checkout deletion occurs.
