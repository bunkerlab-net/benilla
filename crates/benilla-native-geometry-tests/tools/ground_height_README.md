# Exact-source ground-height observations

This oracle executes the complete pinned `Map::GetHeight` and
`Unit::UpdateAllowedPositionZ` bodies. It isolates their call arguments, operand
selection and non-flying/non-swimming Z clamp. Static-height returns are authored
callbacks, not loaded terrain or VMap product queries. These observations alone
are not a real-provider, support, locomotion, admission or gameplay proof.

The pin is CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/game/Maps/Map.cpp:2758`: complete `Map::GetHeight`
- `src/game/Entities/Unit.cpp:11835`: complete `Unit::UpdateAllowedPositionZ`
- `src/game/Maps/GridDefines.h:60–64`: invalid-height and search constants
- `src/game/vmap/DynamicTree.cpp:141,267`: original intersection callback and
  complete `DynamicMapTree::getHeight`
- `src/game/vmap/RegularGrid.h`: complete original header, including empty-node
  construction, `Cell::ComputeCell`, validity and `intersectZAllignedRay`
- `dep/g3dlite/g3dmath.cpp:39`: original `G3D::inf()` used by the empty-tree miss

## Source and boundaries

[`ground_height_generate.py`](ground_height_generate.py) checks every original
file in [`ground_height_source_hashes.sha256`](ground_height_source_hashes.sha256)
and retains the entire existing [`vmap_source_hashes.sha256`](vmap_source_hashes.sha256)
guard set, including G3D headers. It extracts each complete Map, Unit and dynamic
method verbatim and checks unique signatures and the pinned method line numbers.
The checked-in [source package](ground_height_source_fragments.cpp.txt) contains
those exact bodies, constants and the complete RegularGrid header, with original
notices. `--check` refuses a changed package before compilation.

The authored [harness](ground_height_oracle.cpp) supplies only declaration and
observation plumbing around Map and Unit. `TerrainInfo::GetHeightStatic` records
its arguments and returns the explicitly supplied scalar. It does not implement
or simulate terrain loading, static-height selection, VMap ray traversal or
configuration. The source always passes `useVmaps=true` here; actual configuration
enablement and loaded-product behavior must be checked by the provider's separate
tests. The existing [static-height selection oracle](generate_static_height_fixture.py)
and [loaded VMap height oracle](scopedheight_README.md) retain their own scopes
and guards unchanged.

There are two dynamic modes:

- `scripted`: the callback records its arguments and returns an authored scalar,
  solely to observe source operand ordering and dynamic-wins/loses cases
- `empty`: the original `DynamicMapTree::getHeight` executes against the complete
  original `RegularGrid2D` template and original G3D `Ray.cpp`, `Vector3.cpp` and
  `g3dmath.cpp`. The source constructor leaves every node null. Cell computation,
  validity, the vertical grid traversal and the callback's false initialization
  are real source code. Unreachable model/node intersection callbacks abort if
  invoked; there is no invented nonempty dynamic intersection algorithm. The
  miss is source negative infinity, not the static -200,000 sentinel

Only unrelated assertion/log plumbing is substituted for `Util/Errors.h`.
No geometry or math operation in the executed empty-grid route is replaced.
Finite x/y coordinates keep the original cell's float-to-integer conversion
well-defined, including out-of-grid coordinates within ±100,000. Nonfinite x/y
cases use scripted dynamics only; they never enter the source grid cast.

Unit is fixed to `CanFly=false` and `CanSwim=false`. The entire source function is
compiled, but swimming/water-walk and flying paths are unreachable and explicitly
abort if their dependencies are requested. Most ground rows pass an unrelated,
nonnull `atMap`; counters fail if it is used. Four null-`atMap` controls also run.
This observes that the chosen source ground branch queries the owner's `GetMap()`.
It does not model entity capability selection or ownership/lifetime authority.

## Numeric contract

Map first obtains the scripted static height using search 10 or 50 for the source
`swim` argument. It then computes binary32 `2 + (z < staticHeight ? staticHeight : z)`
and only afterward subtracts `staticHeight` for the dynamic range. No validity
threshold, range clamp or early return is added to this stage.

`std::max<float>(staticHeight, dynamicHeight)` preserves its first operand on
equality, including opposite signed zero. The original ground Unit branch checks
`maxZ > -100000` strictly, then `z > maxZ`, then `z < groundZ`. Equality leaves
input Z untouched, so assigning the equal height unconditionally changes signed
zero and is not source-exact. The static return sentinel is -200,000; neither
threshold nor sentinel is substituted for the empty dynamic -infinity result.

[`ground-height.test-fixture`](fixtures/ground-height.test-fixture) has 1,422
finite-input/callback observations. It includes validity-threshold neighbors,
all relative height selections, signed-zero equality, original null/non-null
`atMap`, both static search flags, actual empty grid misses and source float
addition/subtraction ordering. Eighteen bounded-coordinate rows surround a
rounding transition at 4096. In the central row, static `449cfd56` and z `457feb55`
produce dynamic origin `458005aa` and range `45318ca9`; reassociation would yield
`45318caa`. Source-only finite controls around ±2²⁴ and ±2²⁵ also expose addition
rounding. Their `source_only_finite` domain is not a public-provider admission
claim. Even the ordinary `finite` tag only describes callback inputs, not product
coverage or the provider's coordinate/range acceptance.

[`ground-height-nonfinite.test-fixture`](fixtures/ground-height-nonfinite.test-fixture)
has 641 separate `source_only_nonfinite` observations, including quiet NaNs with
explicit payloads and infinities. These diagnose source operand/comparison
behavior only. They must not be used to admit nonfinite product or query inputs.
An `empty` row's source-generated -infinity miss is recorded even when all its
inputs are finite; that is an output sentinel, not admission of nonfinite input.
NaN result payload/sign details are observations of the stated toolchain profile,
not a cross-platform portability promise.

Each tab-separated row has these 23 columns:

```text
case label op domain dynamic_mode x_bits y_bits z_bits swim
static_reply_bits dynamic_reply_bits static_call_count
static_x_bits static_y_bits static_z_bits static_use_vmap static_search_bits
dynamic_call_count dynamic_x_bits dynamic_y_bits dynamic_z_bits dynamic_range_bits
result_bits
```

All `*_bits` are eight hexadecimal binary32 digits. Counts and booleans are
decimal. `op=map` returns the Map height; `op=ground` returns final Unit Z after
calling the same complete Map body. Ground rows always use `swim=0`.
`dynamic_reply_bits` is the actual source -infinity in empty mode. Every row
requires exactly one static callback and one dynamic callback. All numeric
fixture filenames deliberately use `.test-fixture`; no native/game assets are
included.

## Reproduction and guards

Run offline with the pinned source checkout:

```sh
python3 crates/benilla-native-geometry-tests/tools/ground_height_generate.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/ground_height_generate.py /path/to/mangos-classic --check
python3 crates/benilla-native-geometry-tests/tools/ground_height_generate.py /path/to/mangos-classic --check --sanitize
python3 crates/benilla-native-geometry-tests/tools/test_ground_height_generate.py /path/to/mangos-classic
```

`--check` compares both complete fixture byte streams and the source package
without changing them. The compiler and executed command lines are printed.
The profile is Linux x86-64, little-endian IEEE binary32, C++17, `-O0`, no fast
math, no contraction, no strict aliasing and `FE_TONEAREST`. `--sanitize` adds
`-fsanitize=undefined,float-cast-overflow -fno-sanitize-recover=all`; it checks both
the finite and source-only nonfinite runs against the same normal fixtures.
The compiler is `g++`; absent source/compiler and wrong source hashes are errors,
never skips. No Rust compiler, formatter, package installation or network is used.

The ten Python tests check all five new full-file hash guards plus inherited
source/header guards, missing files/compiler, exact packaged bodies/notices,
package/marker drift, callback argument forwarding, domain separation, empty
misses, strict invalid-height boundaries, signed-zero preservation and explicit
source-order controls. They do not replace the compiled-source reproduction or
the consumer's loaded-product tests.

The original CMaNGOS GPL-2.0-or-later notices are preserved in the source package
and injected into the generated compilation unit, with project attribution in
[the existing source notice](../../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt),
[AUTHORS](../../benilla-sim/licenses/AUTHORS.md) and
[COPYRIGHT](../../benilla-sim/licenses/COPYRIGHT.md). The G3D original notice
and Modified BSD grant remain in
[`vmap_G3D_LICENSE.cpp.txt`](vmap_G3D_LICENSE.cpp.txt). The authored harness,
generator, tests and observations are GPL-3.0-or-later.
