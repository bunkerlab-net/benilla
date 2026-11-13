# Bounded raw collision extraction

The dry WMO raw geometry adapter has its own [source contract and scope](WMO-PROVENANCE.md).
The following sections describe the M2 adapter.

The M2 adapter ports only `Model::open`, `fixCoordSystem` and
`Model::ConvertToVMAPModel` from
[CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
The source files are `contrib/vmap_extractor/vmapextract/model.cpp`,
`modelheaders.h`, `vec3d.h` and the `szRawVMAPMagic` definition in
`vmapexport.cpp`. Their original hashes are recorded in
[SOURCE-SHA256SUMS](SOURCE-SHA256SUMS).

CMaNGOS's original code is GPL-2.0-or-later; its
[notice](CMANGOS-NOTICE.txt) is retained. Upstream authorship is recorded in
[AUTHORS.md](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/AUTHORS.md)
and the upstream git history. This Rust port and its validation and tests are
2026 Benilla fork contributors, GPL-3.0-or-later, exercising the compatible
later-license option. The [GPL-3.0 text](../../LICENSE-GPL-3.0) ships with the
repository. No game-install bytes, models or third-party world database rows
are bundled.

## Exact source behavior

`extract_m2_collision(&[u8])` is a pure immutable-byte interface. It returns an
explicit `NoCollision` for a zero `nBoundingTriangles`, or complete raw
`VMAPs05\0` bytes for a validated nonempty hull. The count is the number of
`uint16` indices, not the number of triangles. The packed 324-byte source
header locates indices at `0xec` and vertices at `0xf4`; each count is followed
by its absolute file offset.

The source's two coordinate passes are retained: `(x,y,z) -> (x,z,-y)` when
opening, then `(x,y,z) -> (x,-z,y)` when writing. Together they restore the
original finite vertex bits, including signed zero. The second and third
indices of every triangle are exchanged. Vertices remain in their original
order, including unreferenced vertices; triangles are not sorted, welded,
filtered or repaired.

Raw output has exactly one group and the source's zero root WMO ID, group
flags, group ID, group AABB and liquid flags. `GRP ` has one branch containing
the index count; `INDX` and `VERT` sizes and counts match the source writer.
No vertex-derived bounds are substituted for the serialized zero group box.
Every integer and binary32 value is encoded explicitly little-endian to match
the pinned source's little-endian ABI, without depending on host struct layout.

Normals, authored bounds, render geometry and unrelated arrays are not read
by these source methods and are not validated by this adapter. A zero index
count ignores all collision offsets/counts other than that count, including
stale or otherwise invalid vertex fields. This matches `Model::open`'s early
return, rather than the separate render/native collision witness's stricter
empty-hull and normals contract.

## Explicit safety envelope

The original extractor copies and dereferences input without these checks.
They are deliberate adapter restrictions, not claims about upstream validation:

- A complete packed header, `MD20`, and versions 256 or 257 are required,
  including for `NoCollision`. This profile matches the established strict
  collision witness in
  [benilla-formats](../benilla-formats/src/models/native_collision.rs), not
  the unchecked upstream `Model::open`; later versions are refused
- Complete raw input is capped at 64 MiB, collision vertices at 65,536 and
  indices at 196,608, yielding at most 1,179,744 output bytes
- Nonempty consumed arrays must start after the header and fit the input;
  multiplication, 32-bit source offset addition, host-size conversion and
  output-size arithmetic are checked. No alignment restriction is imposed
- Nonempty index arrays must contain whole triangles and every index must
  name an existing vertex. Every vertex coordinate must be finite, including
  unused vertices. Degenerate triangles and finite extreme values are retained
- Output uses one `try_reserve_exact` allocation, after validation, and has no
  intermediate index or vertex allocation. Allocation failure is an error

The limits bound input traversal and output allocation; they are not a claim
of a hard operating-system time or resident-memory limit. Safe rejection is
never converted into `NoCollision`. Overlapping consumed arrays or trailing
unconsumed data are not silently rewritten.

## Evidence and scope

The unit tests compare full outputs byte-for-byte against the independent
pinned C++ extractor fixtures
[authored version 256](../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/authored-m2.m2.test-fixture)
and
[signed-zero version 257](../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/signed-zero-m2.m2.test-fixture).
These are original numeric test inputs. The oracle executes the pinned source
methods with only the input-buffer seam replaced; see the separate
[fixture generator](../benilla-native-geometry-tests/tools/generate_m2_vmap_fixture.py).
The Rust writer never generates its own expected oracle outputs.

Additional tests cover every truncated input prefix, invalid profile, index
counts/ranges, nonfinite and unused vertices, signed zeros and subnormals,
source offset overflow, allocation-size arithmetic and both count ceilings.
The independent assembler and `.vmo` component-query evidence lives in the
geometry test crate. Raw serialization alone does not establish a source
lease, gameplay admission or complete production geometry.

## Live static owners

`live::Manager` reads retained diagnostic `VMAP_7.0` products. It owns map
trees, logical tile entries, first-installed instances and filename-keyed
WorldModels. Queries borrow those owners, not a canonical resident snapshot.
CMaNGOS 8ec338a1 `VMapManager2.cpp:97-183,310-346` and
`MapTree.cpp:272-493` define load, flags, references, unload and reload.
Known fopen absence differs from an unavailable native product. Source bytes
cannot change after observation.

Non-tiled global models and source `map,y,x.vmtile` names are retained.
Repeated slots retain their first instance but acquire another shared model
reference and overwrite its flags. Source last-logical-tile deletion on a
non-tiled map leaks the global model reference because the destructor does
not call UnloadMap, `VMapManager2.cpp:176-180`, `MapTree.cpp:133-136`.
Unloaded instance storage stays allocated until replacement or tree deletion,
`ModelInstance.h:69`. Authored tests cover these branches.

The app includes the single shared `live/{math,bih}.rs` implementation.
Its stricter immutable component contract is unchanged. The retained
[BIH notice](../benilla-app/src/net/p2p/world/native_geometry/products/vmap/BIH-NOTICE.txt)
credits Christopher Kulla's Sunflow MIT/X11 work. G3D's
[Modified BSD notice](../benilla-native-geometry-tests/tools/vmap_G3D_LICENSE.cpp.txt)
also applies. The new source decoder retains source tree/group/triangle order,
strict triangle intervals and independent area floor/metadata writes,
`WorldModel.cpp:35-85,379-390,428-513`, `ModelInstance.cpp:35-153`.

Basic operations use separate native IEEE RNE arithmetic on x86-64 and
aarch64. The read-only MXCSR gate is unchanged. The aarch64 gate reads
FPCR and requires RMode[23:22]=0, FZ[24]=0, AH[1]=0 and FIZ[0]=0.
The [Arm ARM FPCR field definitions](https://developer.arm.com/documentation/ddi0601/latest/AArch64-Registers/FPCR--Floating-point-Control-Register)
define these rounding, underflow and alternate-handling controls.
FZ16[19] affects half precision only, which this code does not use.
DN[25] changes NaN propagation, not ordered comparisons or `is_nan`.
Other architectures remain refused. No host transcendental function
supplies an answer. Signed-zero Euler identities are exact. All nonzero
rotations remain refused, not approximated.

Rust [primitive floating-point arithmetic](https://doc.rust-lang.org/std/primitive.f32.html)
rounds each addition, subtraction, multiplication and division to nearest,
ties to even. Contracting `a*b+c` into one FMA would violate that rule;
this code uses neither `mul_add` nor algebraic operators.
Rust [`f32::sqrt`](https://doc.rust-lang.org/std/primitive.f32.html#method.sqrt)
guarantees the rounded infinite-precision IEEE `squareRoot` result on
both supported architectures.

The live NaN audit distinguishes internal unordered comparisons from
observable results. `live/mod.rs` rejects nonfinite product floats;
`live/bih.rs` rejects nonfinite point/ray inputs and NaN distances.
Parallel-clip NaNs feed only ordered comparisons, never sign/bit tests.
`live/math.rs` writes triangle distance only inside its strict positive
interval. Its box sign test sees only a negative sentinel or an outside
plane distance, never a NaN. Its square root feeds an infinity comparison.
Only signed-zero rotations reach stored inverse matrices. The unchecked
`to_internal` helper retains its existing nonfinite-input behaviour.

Finite liquid heights can overflow during interpolation and produce a NaN
returned to `LiquidOutput.level`. Hit-position modification can also
produce returned NaN coordinates. On non-x86-64 these operations refuse
with `LiquidInterpolationNaN` and `HitPositionNaN`, respectively; x86-64
keeps its source behaviour. Tests retain normal results on both platforms,
pin x86-64 NaN results, and assert the aarch64 refusals only on aarch64.
No signalling NaN enters from product bytes. Public NaN operands either
reach existing input gates or hit-position modification's result refusal;
no quiet/signalling operand pair can change a later sign/bit decision.

The component kernel is now
[`benilla_sim::source_trig_fma`](../benilla-sim/SOURCE-TRIG-FMA.md).
Its pinned glibc 2.36 FMA oracle matches both functions on all 2^32 encodings.
It retains the LGPL-2.1-or-later FSF 2018-2022 notice and complete licence in
benilla-sim; the player package includes those notices. No coefficients are
copied into this crate.

The live wrapper retains the assembler-radian domain and calls that kernel
only for signed-zero identities. Nonzero rotations still require an
acquisition-bound execution binding. `NumericBinding` and `RotationDomain`
retain separate refusal reasons. Diagnostic products contain 704,523 nonzero
angle occurrences inside that domain and one outside it. The app's immutable
ModelInstance component wrapper selects FMA across its existing bounded domain.
The separately built offline tool profiles remain unfused and unbound.
Equality at
the source's unchecked slot boundary and missing model bounds have typed
undefined-operation refusals, with authored boundary tests. Their diagnostic
stock-reach counts are both zero. No new extraction or reference recording
occurred, and these historical products are not D1 acceptance evidence.

TerrainInfo now uses this live manager. A positive WMO hit follows full
GetGrid before terrain occlusion, `GridMap.cpp:904-918`. Missing MMap loading
remains a later refusal, not a terrain-only shortcut. Map-only startup misses
keep their existing calculation. Native format remains 91.

TerrainInfo retains the early both-loaded predicate at
`GridMap.cpp:1243-1247`. An already loaded VMap tile reaches the missing MMap
loaded-tile lookup before GridMap allocation unless an existing GridMap
short-circuits a map-only call. The predicate is not assumed false.

Model handles bind to their live canonical name allocation and generation.
Matching filename bytes and counters from another manager do not authorise
a query. A retained handle keeps name bytes alive, not collision geometry.
