# Loaded dry static VMap queries

This is a safe Rust source-derived port of a bounded **static VMap ray** subset from
[CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
It loads actual serialized VMap geometry for bounded rays, vertical heights and
location floors; it is not a Detour/MMap loader, PathFinder, composite liquid query,
complete server LOS, swept body support, or native-content admission. The separate
[terrain selector](../PROVENANCE.md#loaded-static-terraininfo-height-prerequisite)
combines raw terrain and this VMap height under an explicit source configuration.

## Source and grants

The port follows `src/game/vmap/{BIH,WorldModel,ModelInstance,MapTree,VMapManager2,TileAssembler}`
and G3D's `Matrix3`, `Vector3`, `Ray`, AABox collision and fuzzy-comparison methods.
[Full-file SHA-256 guards](../../../../../../../../benilla-native-geometry-tests/tools/vmap_source_hashes.sha256)
cover the exact 137 files/headers used by the independent C++ oracle.
CMaNGOS's [GPL-2.0-or-later source notice](../CMANGOS-NOTICE.txt) remains applicable;
this translation and authored harness are 2026 Benilla fork contributors, GPL-3.0-or-later.
The original G3D notice and Modified BSD grant are preserved without changes in
[the upstream license source](../../../../../../../../benilla-native-geometry-tests/tools/vmap_G3D_LICENSE.cpp.txt).
The [original BIH notice](BIH-NOTICE.txt) credits the Sunflow MIT/X11 work of Christopher
Kulla, copyright 2003–2007.
[Detour's notice](../DETOUR-NOTICE.txt) is retained; no Detour implementation is linked.
No install assets, database content, or native executable are included.

The app now includes the single math and BIH implementation from
`benilla-native-vmap/src/live/{math,bih}.rs`. The stricter snapshot contract
below remains unchanged. The separate live manager preserves source
residency and shared flags, but refuses nonzero rotations until the glibc
2.36 FMA trig port is exact. See that crate's `PROVENANCE.md`.

## Loading and bounds

`InventoriedProducts::load_static_vmap` consumes only the immutable, hash-bound product
bytes already owned by the inventory. Loading is off-thread and all-or-nothing. Native
parsers are never called, and no unchecked pointer arithmetic or Rust unsafe code is used.
Map, model-group and triangle BIHs are decoded rather than treated as opaque framing.
Before a snapshot exists, each hierarchy has complete unique reachable nodes, acyclic
references, a permutation of primitive indices, exactly-once leaf-slot coverage, valid
child offsets, and depth at most 63. The pinned format's absent-child ±infinity sentinels
are recognized; NaN and other infinite clips reject. Loaded primitive bounds must fit the
root and every ancestor clip. Vertex and triangle references, finite coordinates,
transformed vertex containment, and cross-tile spawn identity are checked.

Map trees may contain unloaded spawns. Their slots remain explicitly unresolved; a ray
which visits one returns unavailable, even if another loaded primitive already intersects.
A miss concerns only this static closure. Missing/excluded content and runtime-created
objects still need the independent production source/participant audit.

The accepted profile is tiled exterior `VMAP_7.0`, dry models, explicit MOD_HAS_BOUND,
MOD_M2 classification, finite source Euler angles within ±1800 degrees and scales
0.001..1000. Worldspawns, missing bounds, liquids and unknown spawn flags reject. Models
shared by mixed M2/WMO classifications reject instead of preserving upstream shared-model
flag mutation's load-order dependence. MOGP/root/group IDs are retained for selected
location identity but grant no area or liquid semantics. Empty model/group collision is explicit, never missing geometry.

### Extracted M2 group bounds

Pinned `contrib/vmap_extractor/vmapextract/model.cpp:91-93` writes root ID, group
flags, group ID and all six group-bound floats as positive zero for its sole M2
group. Those bounds do not describe the nonzero collision vertices.
`TileAssembler.cpp:319-325` preserves the raw group bound when creating the
`GroupModel`; its mesh BIH is built from the triangles, and its group BIH retains
the zero bound. `WorldModel.cpp:428-440` bypasses that group BIH for a single-group
ray, and `GroupModel::IntersectRay` uses the actual mesh BIH. The separate
`ModelInstance::intersectRay` instance-bound gate remains in effect.

The loader indexes model names, then validates and reconciles every supplied tile
spawn before decoding models. VMO bytes carry no `modelFlags`, so neither a file
name nor zero bounds identify M2. Only a model consistently referenced by validated
`MOD_M2` spawns may use the source representation: root ID zero, exactly one group,
group flags/ID zero, all six bound words exactly positive zero, and no liquid.
Only the otherwise-invalid group-to-vertex containment check is omitted for that
shape. Generic/unclassified and WMO model decoding remains strict. Negative-zero
group-bound words do not match this extractor signature; signed-zero collision
vertices retain their exact bits and remain valid. Model/triangle indices, finite
coordinates, mesh-BIH bounds/clips, group-BIH structure/containment, transformed
instance bounds, map-BIH bounds and validation budgets remain checked. Neither
group nor group-BIH bounds are rewritten to collision-hull bounds.

Point/area/location behavior is unchanged: `ModelInstance.cpp:75-79,109-113`
excludes M2, while direct model/group point queries still use the serialized group
bound. The opt-in liquid geometry loader accepts the same dry M2 representation
but rejects liquid attached to that exception. Ray policy v3 and liquid geometry
policy v2 bind this source-compatible loading profile.

The [authored M2 source fixtures](../../../../../../../../benilla-native-geometry-tests/tools/m2_vmap_README.md)
exercise exact pinned extractor-to-assembler serialization from original off-origin
and signed-zero M2 collision inputs, including a rotated/scaled source placement,
separately from the older manually bounded
`ramp.m2.vmo` fixture. Regression tests retain both source-written zero bounds and
the nonzero hull, compare 1,530 ray, 366 height and 18 point-query cases, and reject
changed identity/signature, unreferenced models, WMO/mixed classification in both
encounter orders, malformed geometry/hierarchies, hidden instance bounds, liquid
and truncation. This is a bounded reader compatibility correction, not
real-asset validation or production content admission.

Additional limits are 32 MiB total VMap input within the inventory's existing 64 MiB
limit; 262,144 elements per non-BIH encoded array; 4,096 total decoded groups; 16,384 serialized
spawn records including cross-tile repeats; and 1,048,576 placement/group/vertex validation
steps across unique placements. Model costs are cached once and include empty groups;
repeating a model cannot amplify either the cost scan or transform loop without hitting
that bound. Unknown map slots are bounded by the map primitive count, not by the
number of declared tiles. Fallible reservations precede large array allocations. Peak
storage is bounded by these counts, including source bytes, decoded arrays, unresolved
spawn slots and temporary validation arrays; this is not a streaming loader. The 32 MiB
raw-input cap is explicitly not a 32 MiB resident-memory cap. Unresolved spawn
slots and temporary bounds vectors scale with the map's primitive count and
remain additional to decoded arrays and the separately owned inventory.
All 354 emitted map-0 single-tile closures measure at most 18,657,687 input
bytes, including the complete map tree and deduplicated resident models.
The next power of two, 33,554,432, leaves 14,896,745 bytes of margin.

BIH tree arrays accept at most 16,777,216 words; object arrays accept at most
1,048,576. Map 0 measures 534,990 tree words and 129,804 objects. A complete map-1
capture refuses a missing required member, so sizing uses the producer's bounded
construction rather than claiming a map-1 measurement.
The framing envelope and loaded BIH decoder share these two constants. Non-BIH
element counts retain their separate 262,144-element cap.
For N positive objects, the binary core has at most 2N-1 nodes. Each core node
can have at most three preceding BVH2 clips, one per tightened axis, and each
binary split can add at most one empty-space clip. Thus there are at most
4(2N-1)+(N-1) = 9N-5 nodes and 27N-15 tree words. Empty trees retain one leaf.
The source branches are `BIH.cpp:105-127,175-250`. The producer already limits
N to 1,048,576 and tree words to 12,582,912; rounding those admitted maxima to
the next powers of two gives the array caps above. The 64 MiB aggregate source
index-byte bound and all resident-input, depth, structure and work limits remain.

## Portable Euler numeric profile

The original G3D float `sin`/`cos` overloads inherit the native C library and are not
bit-identical across platforms. For example, after the source degree conversion, 13°
is binary32 `0x3e685696`: the Linux fixture profile's float sine is `0x3e665992`, whereas
another legitimate approximation is `0x3e665991`. The latter moves the authored rotated
ramp corner's world z from `0x3fcb69e8` to `0x3fcb69e6`, outside the exact serialized
bound. Bounds remain strict; widening them or accepting an epsilon would hide geometry.

`trig.rs` selects the pinned
[glibc 2.36 FMA kernel](../../../../../../../../benilla-sim/SOURCE-TRIG-FMA.md).
The degree and assembler-radian bounds remain unchanged. The wrapper retains
G3D's binary64 PI conversion before the binary32 radian cast. Both inverse
query transforms and forward vertex validation use this profile. Basic matrix
operations remain separate, not implicitly fused.

The static VMap policy is
`benilla-dry-static-vmap-ray-v5-glibc236-fma-euler1800`. Cached ray, height,
location, liquid, and composed route/near-point bindings with the previous
digest cannot be reused. No source product bytes or geometric tolerances change.
The offline TerrainBuilder navigation-mesh transform keeps its separate
unfused profile because that executable's libm dispatch is not bound here.
The independent location binding includes the parent VMap numeric policy.
The liquid policy selects FMA and retains both parent policy digests.

The kernel's LGPL notice and complete licence ship through benilla-sim.
The existing local [notice](GLIBC-NOTICE.txt), [licence](GLIBC-LICENSE.txt),
and unfused fixture manifest remain intact for the retained offline profile.
The pinned-image degree oracle calls the image's actual resolved libm functions.
All 244 existing signed boundary expectations remain unchanged.
It adds 14 authored degree vectors, including every known cosine discriminator.
The 4,251,072-sample degree digest changes from
`c4a91f378dea062d0159217e6910b374c0ceacf90f644fe05d1a2ac465841998` to
`19a2791f8c35c9b147a8753bb2cd1e32f523b9028a55db830d10dce34d61e4be`.
Exactly two grid cases differ, degree bits `44777ff0` and `c4777ff0`.
Their radian bits are `418a3add` and `c18a3add`; cosine changes from
`b794f76f` to `b794f770`, and sine stays unchanged.
The masked oracle exactly reproduces the old unfused digest.
The full-domain exhaustive kernel receipt is separate from this degree sample.
Neither component proof removes the live manager's nonzero-rotation refusal
or establishes historical dispatch before the bound recordings.

## Queries and current consumer

The query port preserves source binary32 expression order, G3D's float-argument trig
contract under the pinned profile above, and its cofactor inverse, `{rotation.y, rotation.x, rotation.z}` ordering, inverse scale, source AABB gate,
nearest triangle intersection, BIH ordering/pruning, signed-zero direction bits and the
source's NaN branch behavior at parallel clips. Rays use the strict source `0 < t < max`
interval. The endpoint API performs VMapManager2's coordinate conversion before source
normalization, with no assumed lossless float roundtrip. Zero-length segments preserve
the source no-hit behavior. Points must be finite within ±100,000; segment length is
limited to 4,096. Each query has at most 32,768 work steps and an absolute deadline.
Exhaustion, malformed queries and unresolved candidates are errors, never clear rays.

A reply echoes exact endpoint bits, M2 mask, ticket and snapshot binding. The binding
contains map, content, source epoch, full inventory revision and static-ray policy-v5
with bounded glibc 2.36 FMA trig and the source M2 loading profile.
Those are consistency declarations, not authenticated provenance or a live source lease.

The app's optional `install_with_static_vmap` connects this snapshot through
`SourceVmapProvider` to the existing `Corridor`. Its session geometry revision must bind
both the current raw support-geometry revision and the complete VMap snapshot binding.
The raw corridor still proves current source inventory, continuous flat support, dry
space, body sweep and participant exclusion first. A source VMap hit then adds an
obstacle veto; a miss does not replace any positive proof. The provider checks the exact
session/request/body and inherits the original mailbox/frame deadline. Source-product
replacement changes the combined revision; replacing/uninstalling the service revokes
its old authority queue, including reply validation after a reply was already buffered.
Already-applied runtime state is not retroactively undone. Production entry still must
implement actual source-epoch lifecycle and atomic world/runtime replacement.

## Differential evidence and remaining gates

The [authored fixture contract](../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/README.md)
explains reproducible offline generation and the exact source methods compiled. Tests
compare 2,508 ray vectors, 2,470 normalized endpoint vectors, 78 world-conversion vectors
and 188 BIH callback traces bit-for-bit. Six VMap products contain original numeric ramp,
stacked-floor, wall and rotated/scaled geometry; source native writers construct their
serialization and hierarchy. No native parser loads untrusted input in the oracle.
Malformed counts/references, hiding bounds, cross-tile conflicts, every truncated model
prefix, missing intermediate primitives, budget/deadline failures and stale request
bindings have separate rejection tests. Provider tests show that source hits block while
unsupported support or participants remain unavailable.

Linux x86-64 source-oracle comparison and the exact-code headless harness are separate
from full Bevy compilation/linking, cross-platform execution gates, real lawful local
Classic Data, and live visual/runtime tests. Real-asset validation has not been performed.
No Northshire slope/navigation claim follows. The separately bounded dry Detour reader
adds a query veto; actual owner-dependent PathFinder, sloped-body support, complete dynamic-participant audit,
authenticated local extraction and live source-epoch admission remain necessary.

The distinct [bound static source LOS adapter](../../README.md#bound-static-source-los)
uses this same loaded snapshot for two current-phase queries, explicit LOS-enabled
policy and separate profile-owned endpoint heights with `ignore_m2=true`. It does
not reuse the movement veto. Dynamic participants remain excluded by the existing
unconstructible production proof; visibility/accessibility and native admission
remain unavailable. Authored adapter/controller tests add no real-asset evidence.


## Location floors

The [loaded location query](../PROVENANCE.md#loaded-dry-vmap-location-floor-prerequisite)
uses original point-BIH traversal, group containment/inside-object rays and strict
nearest-group/highest-instance selection. Unlike the optional ray closure, its
public request requires every map slot resolved. The query-specific policy prevents
relabeling a ray or height binding as a location binding. An explicit location miss
is valid only within this complete dry snapshot; it never supplies water, support,
accessibility, participant or native-content admission.
