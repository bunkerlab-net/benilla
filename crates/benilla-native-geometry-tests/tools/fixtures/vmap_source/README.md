# Authored, exact-source VMap ray fixtures

These files contain only synthetic numeric geometry authored for tests. They contain no
installation assets, database rows, extracted game models or native executable. They are
not evidence of gameplay, admission, body support, pathfinding or real-world coverage.

Regenerate offline from a local copy of CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

```sh
python3 crates/benilla-native-geometry-tests/tools/generate_vmap_fixture.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/generate_vmap_fixture.py /path/to/mangos-classic --check
```

The generator hash-checks the complete files in
[`vmap_source_hashes.sha256`](../../vmap_source_hashes.sha256), including every G3D header,
before invoking g++. It compiles the original, unmodified `BIH.cpp`, `WorldModel.cpp`,
`ModelInstance.cpp`, and G3D `Ray.cpp`, `Vector3.cpp`, `AABox.cpp`, `Matrix3.cpp` and
`g3dmath.cpp`. Only the fixed-width integer aliases in `Platform/Define.h` are stubbed,
avoiding an unrelated Boost/core dependency. No math, query or serialization method is
stubbed. Compiler settings are C++17, `-O0`, no fast math, no contraction, no strict aliasing,
and `FE_TONEAREST`. The checked profile is little-endian IEEE binary32 on Linux x86-64.
The portable consumer pins the Linux float-trig profile instead of inheriting another
platform C library; the original fixture products and queries are not regenerated for it.
Cross-platform execution still requires its own native CI verification.

`CollisionDetection.cpp` has a static `Array` requiring unrelated allocator/input-grab
subsystems. Its two AABox collision methods are therefore extracted verbatim from the
hash-checked full source. The same extraction preserves `StaticMapTree`'s constructor,
destructor, `MapRayCallback` and `getIntersectionTime`, `ModelPosition::transform`, and
`VMapManager2::convertPositionToInternalRep`. The `calculateTransformedBound` transform
setup and bounds-merge loop are source-exact; authored vertex arrays replace only the
asset-reading prefix. Original header declarations and methods are used. The map tree's
private members are exposed only for fixture construction without invoking a loader.
The manager conversion has a minimal declaration; no manager loading behavior is emulated.

CMaNGOS's original GPL-2.0-or-later grant is retained in
[the source notice](../../../../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt).
The authored harness is GPL-3.0-or-later. G3D's original notice and Modified BSD grant are
preserved byte-for-byte in [`vmap_G3D_LICENSE.cpp.txt`](../../vmap_G3D_LICENSE.cpp.txt).
The pinned BIH header also retains its Sunflow-derived MIT/X11 attribution to Christopher
Kulla (2003–2007). The generator does not download or vendor any implementation.

## Products and geometry

- `000.vmtree`: original `BIH::writeToFile` output in tiled `VMAP_7.0` framing
- `000_32_32.vmtile`: original `ModelSpawn::writeToFile` records plus source-format node IDs
- `ramp.wmo.vmo`: 32 triangles on `z = 0.5*x + 0.25*y`, within `[-4,4]^2`
- `stack.wmo.vmo`: four 32-triangle groups at z = 0, 3, 6 and 9
- `wall.wmo.vmo`: 32 triangles on x = 0, y in [-4,4], z in [0,8]
- `ramp.m2.vmo`: separate ramp model, classified M2 through its spawn

Model placements are the five serialized spawn records: an identity ramp at the origin,
a stack at (20,0,0), wall at (40,0,0), the same WMO ramp at (70,20,4) with source Euler
rotation fields (13,37,-9) degrees and scale 1.75, and M2 ramp at (100,-15,2) with scale 0.5.
Model/group/map BIHs are built by the original implementation. All bounds derive from
actual authored vertices using the source transform and merge operation, without invented
padding. The tile declaration is test framing, not a claim about terrain coverage.
Native read/load methods are never called, including for these authored products.

## Expectations

All float fields are eight hexadecimal digits representing exact binary32 bits. Every
full-scene case is run with `stopFirst=false` and both `ignoreM2Model` values. On a miss,
the distance remains the supplied maximum. `rays.tsv` has 2,508 rows. `segments.tsv` has
2,470 internal endpoint rows and includes the exact source-style normalized direction and
maximum distance. `world-segments.tsv` has 78 rows, including original world endpoints,
source-converted internal endpoints, normalized direction and nearest-ray result. Its
world inputs deliberately exercise the conversion's float rounding; no lossless roundtrip
is assumed. Headers list all fields, with boolean values as decimal 0/1.

The ordinary rays cover ramps, stacks, occlusion, both triangle windings, shared edges,
corners, misses, rotated/scaled instances, signed-zero direction components, and source
strict origin/endpoint behavior (`t > 0`, `t < distance`). Nearest surface distance is a
point-ray result, never a body clearance/support certificate.

`bih-oracle.tsv` has 188 independently authored BIH callback traces generated by
[`vmap_bih_fixture.cpp`](../../vmap_bih_fixture.cpp) against the original header. Its root
is always `[-2,2]^3`; tree/object words, ray input and ordered callback IDs are serialized
as hex. Callback mode 0 reports no hit; mode 1 proposes distance 0.25; mode 2 proposes 2.0
for object 1 and 1.5 otherwise. Traces include signed zero, zero-plane multiplication
NaNs, missing-child infinity sentinels, BVH2 clipping, subnormals, all axes, shrink/pruning,
fuzzy-comparison adjacent values and root endpoints. These are constructed typed arrays,
not native parser input. Some rays are deliberately approximate-unit to probe traversal
boundaries directly; they are not a promise that a public endpoint API accepts them.

## Portable trig-profile oracle

The separate `generate_vmap_trig_fixture.py` compiles the exact generic float trig
functions and coefficient arrays from glibc 2.41 commit
`74f59e9271cbb4071671e5a474e7d4f1622b186f`. Supply a directory containing the five original
`sysdeps/ieee754/flt-32` files named in `vmap_glibc_source_hashes.sha256`:

```sh
python3 crates/benilla-native-geometry-tests/tools/generate_vmap_trig_fixture.py /path/to/glibc-flt-32
python3 crates/benilla-native-geometry-tests/tools/generate_vmap_trig_fixture.py /path/to/glibc-flt-32 --check
```

It verifies every whole-file hash before compiling. Only include/declaration plumbing
is supplied; math bodies and coefficient arrays are original source. Compilation uses
C++17, `-O0`, no fast math, no contraction, round-to-nearest and generic
`TOINT_INTRINSICS=0`. No platform trig function supplies these expectations.

`trig-cases.tsv` covers both signs, signed zero, subnormals, tiny-input and `abstop12`
branch thresholds, 13° and all quadrant boundaries through ±360°, with three neighboring
binary32 angles on either side where admitted. It has 202 cases. `trig-grid.sha256`
hashes 12-byte little-endian records `(angle bits, sine bits, cosine bits)`: first every
4093rd nonnegative angle bit pattern through `360.0f32`, then every 1/1024-degree value
from 0 through 360, each with positive then negative sign. All 1,292,314 records are
recomputed by the Rust tests. Finite angles outside ±360 and nonfinite inputs are
separately rejected. This bounded source profile preserves exact transformed containment;
a regression also proves that a bound hiding one ULP of a vertex still rejects.

See the consumer's
[numeric provenance and original license](../../../../benilla-app/src/net/p2p/world/native_geometry/products/vmap/PROVENANCE.md#portable-euler-numeric-profile).
