# Loaded dry static VMap rays

This is a safe Rust source-derived port of a bounded **static VMap ray** subset from
[CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
It loads actual serialized VMap geometry; it is not a Detour/MMap loader, PathFinder,
composite terrain height, server LOS, swept body support, or native-content admission.

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
MOD_M2 classification, finite source Euler angles within ±360 degrees and scales
0.001..1000. Worldspawns, missing bounds, liquids and unknown spawn flags reject. Models
shared by mixed M2/WMO classifications reject instead of preserving upstream shared-model
flag mutation's load-order dependence. MOGP/root/group IDs are decoded but grant no area
or liquid semantics. Empty model/group collision is explicit, never missing geometry.

Additional limits are 16 MiB total VMap input within the inventory's existing 64 MiB
limit; 262,144 elements per encoded array; 4,096 total decoded groups; 16,384 serialized
spawn records including cross-tile repeats; and 1,048,576 placement/group/vertex validation
steps across unique placements. Model costs are cached once and include empty groups;
repeating a model cannot amplify either the cost scan or transform loop without hitting
that bound. Unknown map slots are bounded by the map primitive count, not by the
number of declared tiles. Fallible reservations precede large array allocations. Peak
storage is bounded by these counts, including source bytes, decoded arrays, unresolved
spawn slots and temporary validation arrays; this is not a streaming loader. The 16 MiB
raw-input cap is explicitly not a 16 MiB resident-memory cap. At the maximum map count,
64-bit Rust's unresolved/loaded spawn-slot array alone occupies 30 MiB, and its temporary
bounds vector occupies 7 MiB. These are additional to decoded arrays and the separately
owned inventory (up to 64 MiB input plus its decoded terrain).

## Queries and current consumer

The query port preserves source binary32 expression order, G3D's float trig and cofactor
inverse, `{rotation.y, rotation.x, rotation.z}` ordering, inverse scale, source AABB gate,
nearest triangle intersection, BIH ordering/pruning, signed-zero direction bits and the
source's NaN branch behavior at parallel clips. Rays use the strict source `0 < t < max`
interval. The endpoint API performs VMapManager2's coordinate conversion before source
normalization, with no assumed lossless float roundtrip. Zero-length segments preserve
the source no-hit behavior. Points must be finite within ±100,000; segment length is
limited to 4,096. Each query has at most 32,768 work steps and an absolute deadline.
Exhaustion, malformed queries and unresolved candidates are errors, never clear rays.

A reply echoes exact endpoint bits, M2 mask, ticket and snapshot binding. The binding
contains map, content, source epoch, full inventory revision and static-ray policy-v1.
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
from full Bevy compilation/linking, other platform math behavior, real lawful local
Classic Data, and live visual/runtime tests. Real-asset validation has not been performed.
No Northshire slope/navigation claim follows. Detour internal-reference validation/loading,
actual owner-dependent PathFinder, sloped-body support, complete dynamic-participant audit,
authenticated local extraction and live source-epoch admission remain necessary.
