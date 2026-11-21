# Independent original warm-to-point-path oracle

The observer is pinned to CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It compiles complete, unchanged
`getPathPolyByPosition`, `getPolyByLocation`, `fixupCorridor`, `getSteerTarget`,
`findSmoothPath`, `BuildPointPath`, `BuildShortcut`, `NormalizePath`,
`inRangeYZX`, `inRange`, and `dist3DSqr` methods against the original Detour
translation units and **actual original G3D::Vector3 headers**. It never links
the production navigation bridge or substitutes vector arithmetic.

## Evidence boundary

`fixtures/point-path/cases.test-fixture` records actual loaded warm selection,
selected-reference cache transition, original smoothing, and full original
BuildPointPath execution. Authored geometry is reused from the existing
warm-cache, smoothing, and two-tile oracle products. No install assets, world
database, importer, network, or download are used.

The selected-reference transition compiles exact BuildPolyPath fragments:

- The complete same-polygon branch, including BuildShortcut, cache assignment,
  type assignment, and early return in their original order
- First-start/last-end active scanning and cutting
- Original clear followed by original direct findPath, with 74 capacity and
  the original MoveMap query pool of 1024 nodes
- The pre-BuildPointPath type classification, including the incoming
  PATHFIND_INCOMPLETE bit

Complete BuildPolyPath is included only as review context. Its owner/terrain,
missing-polygon, far-distance, straight-line, and broader world behaviors are
not exercised. Every loaded row has nonempty valid active storage before warm
lookup, finite selected distances at most 7, valid loaded references, and a
complete direct status if replacement is used. Raw cold uninitialized-distance
lookup remains excluded. Full retained cache storage is recorded, including
inactive zeros and tail slots; only active length is cleared by shortcuts.
The 4096 retained-slot bound is an observer/port operational cap, not an
original vector bound.

An isolated identical-input original smoothing probe records raw status/points.
The full original BuildPointPath then runs its own original findSmoothPath;
its cached raw float output must equal that probe byte-for-byte. Smoothing's
private m_smoothPathPolyRefs is separate from m_pathPolyRefs, so smoothing
fixup does not rewrite the selected polygon cache. Same-polygon rows bypass
both smoothing and BuildPointPath, exactly as the original branch does.

Every configuration flag is an explicit fixture input. ConfigSnapshot supplies
only the two named original getConfig values. NormalizePath is compiled in
full and can only take its original disabled/ignored early return. A nonnull
diagnostic-only owner prevents null-owner short-circuiting from masking this
restriction. All transport/terrain/owner callbacks abort if reached; there is
no fabricated world provider. Straight-line and useStraightPath modes remain
false. These are bounded cache-dependency observations, not a source-produced
initial Chase, an admitted Route, complete owner pathfinding, or gameplay.

## Raw postprocessing controls

`postprocess.test-fixture` records two exact BuildPointPath source spans, from
its optimization/conversion branch through terminal classification, force
handling, and NormalizePath. These spans run on explicitly authored raw
point/status inputs. They **do not claim that original loaded smoothing can
produce every such combination**. In particular, original smoothing reports
failure at its point limit, so the successful equal-limit SHORT control is
raw-only. A smaller raw point limit is also helper-only; composed rows use 74.

The controls independently observe:

- YZX to XYZ conversion, fewer-than-two and failed-status precedence, and SHORT
- Optimization before the final count limit test
- Strict LINE_FAULT 0.5 with adjacent binary32 inputs and SKIP_POINT_LIMIT 6
- Previous-point mutation to the zero sentinel, which is not ordinary
  collinearity simplification; every next iteration reads the modified vector
- Strict force-destination distance ratio 0.3, strict horizontal and vertical
  inRange boundaries of 1, and incoming type dependence
- Force-near endpoint replacement versus force-far BuildShortcut, with both
  producing NORMAL | NOT_USING_PATH (17) and setting actual end to destination

Original XYZ zero input during a greater-than-two optimized path is a special
raw-source observation. Compaction skips genuine zeros as well as removed
points, leaving an unwritten suffix in resized m_pathPoints. The original
G3D default constructor explicitly initializes all coordinates to zero; if
prior point storage survives a Cut, the unwritten suffix instead keeps its
prior values. Fixtures provide both explicit prior points and fresh storage,
including signed-zero inputs. The production bounded checkpoint rejects this
branch rather than inventing or discarding prior history. This is a defined
source dependency, not fabricated initialized-cold lookup evidence.

## Schema and provenance

Both products have one tab-separated `# label` header. All points are world
XYZ binary32 words: three comma-separated eight-digit hex words per point,
semicolon-separated point sequences. `-` denotes an empty sequence or absent
same-polygon smoothing status. Reference vectors are decimal comma lists;
path types and counts are decimal; Detour statuses are eight-digit hex words.

Loaded rows record from/to, the complete initial cache, optimization and
normalization settings, force flag, prior path type, selected transition and
complete cache, raw smooth status/points, and final active/full cache, path
type, actual end, and points. Raw controls additionally record point limit,
explicit prior point history, and raw status/input points. The generator's
FIELDS and RAW_FIELDS lists are the exact schema contracts.

`point_path_source_hashes.sha256` binds all 46 normalized-LF original inputs:
PathFinder, MoveMap, original config/type context, every compiled Detour source
and header, the complete included G3D header dependency closure, and CMaNGOS,
Detour, and G3D license inputs. Source-content drift and missing/duplicate/
unsafe manifest entries fail closed. Complete methods have exact source-line
guards. `point_path_source_fragments.test-fixture` preserves the source spans,
full methods, context, original notices, and license texts.
`point_path_input_hashes.sha256` binds all reused authored mesh products.
All new products use the non-admitted `.test-fixture` suffix.

## Compiler and object lifetime

The audited profile is GCC 14.2.0, x86_64-linux-gnu, C++20, 64-bit polygon
references, IEEE binary32, nearest rounding, no FTZ/DAZ, no fast math, no FMA,
and no contraction. Original G3D headers compile in their NDEBUG release
profile; no vector implementation is replaced. Missing/different compiler or
missing source/G3D/Detour symbols fail closed. Any production bridge symbol
also fails the guard.

The unchanged original dtMeshTile declaration is not default-constructible or
aggregate in C++20 while original allocation uses raw storage. Its known
object-lifetime caveat remains: matching output and clean UBSan are empirical
observations, not a standards-defined lifetime proof. The explicit
`--repair-tile-lifetime` lane inserts only `dtMeshTile() = default;`, verifies
identical layout/alignment/every field offset and noncopyability, and must
reproduce every fixture byte. Neither lane proves complete world correctness.

## Reproduce

With an existing local pinned source checkout, from the repository root:

```sh
SOURCE=/path/to/mangos-classic-8ec338a1704e7dcb1c0213eb7ed58f9231ade40f
TOOLS=crates/benilla-native-nav/tools
python3 "$TOOLS/point_path_generate.py" "$SOURCE" --check --receipt-dir /tmp/point-original-normal
python3 "$TOOLS/point_path_generate.py" "$SOURCE" --check --sanitize --receipt-dir /tmp/point-original-ubsan
python3 "$TOOLS/point_path_generate.py" "$SOURCE" --check --repair-tile-lifetime --receipt-dir /tmp/point-repaired-normal
python3 "$TOOLS/point_path_generate.py" "$SOURCE" --check --repair-tile-lifetime --sanitize --receipt-dir /tmp/point-repaired-ubsan
python3 "$TOOLS/point_path_test_generate.py" "$SOURCE"
```

Omit `--check` only to regenerate from verified original source. Receipts
preserve compiled harness, symbols, observed tile header, compiler command and
profile, source/input manifest digests, and output hashes. Guards cover
malformed bounds/schemas, exact original code, configuration exclusion, unsafe
names, compiler/symbol drift, bridge exclusion, nonrewriting check mode,
source drift, numeric boundaries, and an actual core.autocrlf=true Git
add/checkout-index round trip without any commit.
