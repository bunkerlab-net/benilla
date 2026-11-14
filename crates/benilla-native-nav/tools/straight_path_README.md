# Independent original warm straight-line oracle

This test-only observer pins CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It compiles unchanged complete
`getPathPolyByPosition`, `getPolyByLocation`, `BuildPointPath`, `NormalizePath`,
`BuildShortcut`, `fixupCorridor`, `getSteerTarget`, `findSmoothPath`, `inRangeYZX`,
`inRange`, and `dist3DSqr` bodies against original Detour translation units and
actual original G3D::Vector3 headers. It does not link production navigation
code or replace G3D arithmetic with a lookalike implementation.

## Exact scope and external inputs

The generated harness inserts the exact BuildPolyPath suffix from the
same-polygon branch through its final BuildPointPath call. This includes
first-start/last-end active scanning, Cut, clear-before-Replacement, actual
Detour raycast, hit processing, direct findPath for the later smooth call,
and source terminal type ordering. The wrapper supplies already selected
start/end refs and `farFromPoly=false`. Every loaded fixture obtains those
refs through the unchanged original warm lookup and requires finite selected
distances at most seven. Complete BuildPolyPath and Chase's dispatch method
are packaged as review context, not claimed as compiled owner behavior.

A diagnostic owner supplies one explicit, nonnegative finite scalar:
GetCollisionWidth's **full width**. The observer does not derive owner width,
body support, radius, capabilities, or world geometry. It preserves the source
operation `hit = hit - width`, where hit is Detour's dimensionless ray fraction;
there is no distance normalization or half-width conversion. ConfigSnapshot
supplies only explicit optimize/normalize booleans. NormalizePath is compiled
unchanged but may only take its original disabled/ignored early return.
Owner, terrain, transport and diagnostic callbacks abort if unexpectedly
reached. The owner is nonnull, so null-owner short-circuiting cannot mask an
unsupported normalization path.

Input references are valid on the actual loaded mesh; active refs are nonzero.
All backing slots, including inactive zeros and retained tail refs, are
recorded. A zero-active input is admissible only with a nonempty eligible tail
scan, which initializes the original distance before nearest lookup can read
it. Cold uninitialized-distance lookup is excluded; it is never repaired by
preinitializing the source's local distance. Coordinate magnitude is bounded
by 100,000, retained storage by 4,096 slots, and sample length by 4,096 (at
most 1,026 observed points). These are observer/port operational bounds,
not original source semantic point limits. This observer additionally limits
width to 100,000; production accepts any finite nonnegative width, with large
widths rejected by the original adjusted-hit comparison. The source limit
remains 74 throughout, with the original 1,024-node query pool.

Authored serialized geometry is reused from the warm-cache, smoothing and
direct-corridor observers: two floors, a sloped snake and straight strip, thin
detail triangles, and a 600-polygon corridor. No game install, world database,
network source, importer, or game asset is required. The input manifest also
retains hashes for the existing two-tile fixture family, which is not executed
by this corpus.

## Actual output coverage

- 35 cases: 24 loaded warm transitions and 11 direct BuildPointPath helper
  cases. Helpers explicitly provide initialized warm state and a prior path
  type; they do not pretend that every input came from a full owner request
- 5 same-object straight-to-smooth continuations, using the prior terminal
  type, complete mutated backing storage and active count. The original
  NOPATH/INCOMPLETE fallback predicate is asserted. A second independent
  original smooth probe verifies the first invocation's raw cached float
  stream byte for byte; it does not replace that invocation
- 1,568 actual Detour primitive trace rows, including 18 raycasts, 307 height
  calls, and 3 replacement direct searches. Each wrapper delegates to real
  original Detour before recording status, point bits and returned refs
- 10 classified, non-invoked rejected domains: undefined cold distance,
  invalid active/tail refs, active overflow, nonfinite endpoints/width,
  negative width, active normalization, invalid prior type, and sample bound

The cases establish:

- Same-polygon shortcut bypass and zero-length two-point output; Cut bypasses
  raycast even where the straight segment crosses a wall
- Original full-width subtraction and adjacent binary32 results below, equal
  to, and above `0.1f`, with rejection only below
- Squared hit distance strictly greater than the **double** literal `0.01`,
  using source-produced values on both sides. There is no exact-equality
  fixture: a binary32 squared result cannot equal that binary64 literal
- Ignored getPolyHeight failure status with already initialized interpolated
  hit Z, or already initialized mutable sample Z. Failed height calls preserve
  those exact bits; no synthetic height is supplied
- A hit branch's final INCOMPLETE assignment after BuildPointPath, including
  force-destination handling that would otherwise assign type 17
- FLT_MAX no-hit behavior, no width call on no-hit, incoming INCOMPLETE type
  retention, and a SUCCESS|BUFFER_TOO_SMALL ray that keeps 74 active refs
  while subsequent sampling appends additional retained tail refs
- Strict sampling length `> 0.1f`, step size four, remainder `< 0.3f` merge,
  duplicate endpoint at exactly one step, and later samples starting from
  the previous height-mutated vector
- Optimization is ignored by the straight sampling branch. Paired straight
  cases match exactly; their later smooth continuations produce equal raw
  smooth points but 9 versus 5 postprocessed points
- Source count **equality**, not a capacity clamp: generated counts 73, 74,
  and 75 yield NORMAL/73 points, SHORT/two-point shortcut, and NORMAL/75 points.
  The 74 case clears only active length and preserves full retained storage

## Evidence limits

These are explicit authored warm dependency observations. They establish no
initial cold Chase, owner movement, complete PathFinder, NormalizeZ world
provider, transport, swimming/flying, playerbot/dungeon limit, complete route,
or gameplay admission. A read-only source trace does not authorize any of
those broader claims. The exact selected suffix includes unsupported failure
branches, but diagnostics abort if they are reached outside the validated
fixture domain. There is no observed ray failure or zero-ref-hit recovery
claim, and this corpus does not reach the 5-to-10 nearest-search fallback
(covered separately by the warm-cache observer). The ten rejection rows
record the harness classifier and `invoked=0`,
not undefined upstream behavior. Helper rows and the test fixture extension
are not production inputs or authority-minting interfaces.

## Schemas and provenance

Every product has one tab-separated `# label` header. World XYZ coordinates
are three comma-separated, eight-digit binary32 hex words; point lists use
semicolons, with `-` for empty. Reference storage is a decimal comma list;
counts and path types are decimal; Detour statuses are eight-digit hex words.
The generator's FIELDS, SEQUENCE_FIELDS, TRACE_FIELDS and DOMAIN_FIELDS are
the complete schema contracts. Trace labels combine their case/continuation
label and a contiguous four-digit ordinal. Scalar ray values remain fractions.

`straight_path_source_hashes.sha256` guards all 47 normalized-LF source inputs,
including original config/type/Chase context, the Detour and G3D dependency
closure, and all three license inputs. `straight_path_source_fragments.test-fixture`
preserves full unchanged methods, exact suffixes, review-only context, original
notices and license texts. Method line guards additionally pin source locations.
`straight_path_input_hashes.sha256` guards the reused authored mesh products.
All generated observations use the non-admitted `.test-fixture` suffix.

The generator verifies schemas, numeric domains and storage lengths, exact
same-object continuation state, trace ancestry/order, ray/height summary
agreement, and binary32 full-width subtraction before publishing output.
Check mode never rewrites drift. Tests deliberately damage source hashes,
manifests, markers, symbols, warning diagnostics, schemas, output names and
cross-product relationships. A real Git core.autocrlf=true add/checkout-index
round trip verifies exact bytes without a commit; .gitattributes pins these
text products to LF and preserves original source whitespace.

## Compiler, warning and object-lifetime profiles

The audited compiler is GCC 14.2.0 targeting x86_64-linux-gnu, C++20, 64-bit
polygon references, little-endian IEEE binary32, nearest rounding, no FTZ/DAZ,
no FMA, no fast math and no contraction. `-O2 -fno-inline` retains original
symbols for inspection. Original G3D headers use their NDEBUG release profile.
The build requires all unchanged method and actual G3D/Detour symbols and
rejects production bridge symbols.

`-Wall -Wextra -Werror` remain enabled. The explicit legacy source allowances
are class-memaccess and deprecated-copy; unused-variable remains enabled but
is nonfatal only after a second guard matches the one expected diagnostic
exactly. That diagnostic must name the generated oracle file and precise line
and column for original PathFinder.cpp:738's unchanged ignored-height local
`dtStatus dtResult`. Missing, additional or displaced warnings fail closed.
LC_ALL=C and color-free diagnostics make matching deterministic. The observer
does not delete or rewrite the original assignment to avoid the warning.

The exact original dtMeshTile declaration is neither default-constructible nor
an aggregate under C++20 while its allocation uses raw storage. That original
object-lifetime caveat remains. Equal outputs and clean UBSan are empirical
observations, not a standards-defined proof for the unchanged lane.
`--repair-tile-lifetime` inserts only `dtMeshTile() = default;` in the copied
header; it changes no source algorithm. Static assertions compare size,
alignment, every field offset, noncopyability and trivial default construction
with the exact original layout. Both lanes must reproduce every output byte.

## Reproduce

With an existing pinned local source checkout, from the repository root:

```sh
SOURCE=/path/to/mangos-classic-8ec338a1704e7dcb1c0213eb7ed58f9231ade40f
TOOLS=crates/benilla-native-nav/tools
python3 "$TOOLS/straight_path_generate.py" "$SOURCE" --check --receipt-dir /tmp/straight-original-normal
python3 "$TOOLS/straight_path_generate.py" "$SOURCE" --check --sanitize --receipt-dir /tmp/straight-original-ubsan
python3 "$TOOLS/straight_path_generate.py" "$SOURCE" --check --repair-tile-lifetime --receipt-dir /tmp/straight-repaired-normal
python3 "$TOOLS/straight_path_generate.py" "$SOURCE" --check --repair-tile-lifetime --sanitize --receipt-dir /tmp/straight-repaired-ubsan
python3 "$TOOLS/straight_path_test_generate.py" "$SOURCE"
```

The sanitizer lanes use `undefined,float-cast-overflow` with no recovery.
Receipts retain the harness, symbol list, observed tile header, compiler
stderr/command/profile, manifest hashes and output hashes. Omit `--check` only
to regenerate from independently hash-verified originals. The four profiles
are separate observations; these Linux-only checks do not establish other
platforms or full workspace/gameplay validity.
