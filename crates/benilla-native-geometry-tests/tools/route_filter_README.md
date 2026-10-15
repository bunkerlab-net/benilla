# Ground/dry creature route-filter source oracle

This independently compiled observer executes the unchanged CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` filter methods with authored scalar
callbacks. It is a bounded filter prerequisite, not a route, Detour corridor,
loaded-world, startup, movement, transport, bridge or gameplay-admission proof.
No game installation, world database, Rust producer or network supplies expected
outputs. The native provider separately proves its actual loaded liquid inputs.

## Preserved source and observation boundary

The [manifest](route_filter_source_hashes.sha256) guards six complete original
files, allowing only CRLF-to-LF normalization. Their hashes are also pinned in
`benilla_sim::route_filter::SOURCE_SHA256`. The
[source package](route_filter_source_fragments.test-fixture) retains the original
copyright/license notices and these complete unchanged methods:

- `MotionGenerators/PathFinder.cpp:1030`: `PathFinder::createFilter`
- `MotionGenerators/PathFinder.cpp:1088`: `PathFinder::updateFilter`
- `MotionGenerators/PathFinder.cpp:1103`: `PathFinder::getNavTerrain`
- `Entities/Unit.cpp:4467`: `Unit::IsInWater`
- `Entities/Unit.cpp:4477`: `Unit::IsUnderwater`
- `Maps/GridMap.cpp:797`: `TerrainInfo::CanCheckLiquidLevel`
- `Maps/GridMap.cpp:1130`: `TerrainInfo::IsInWater`
- `Maps/GridMap.cpp:1163`: `TerrainInfo::IsUnderWater`

Original `NavArea`, `NavTerrainFlag`, `TypeID`, `GridMapLiquidStatus`,
`GridMapLiquidData` and liquid-type macros are extracted unchanged. Compile-time
assertions verify the original navigation flags are ground=1, steep=2, water=4,
magma/slime=8 and masks are all-liquids=15 and water/ocean=10. This is the
non-PLAYERBOTS creature domain. The full conditional source body remains in the
package; defining `ENABLE_PLAYERBOTS` is a compile-time error in this observer.

The harness supplies explicit scalar `Creature::CanWalk`/`CanSwim` replies,
VMap height-enabled and grid-presence replies, and ordered liquid status/type
replies. These are declared test inputs, not reconstructed source loaders or
geometric facts. The existing separately tested source creature-capability port
supplies the actual Rust predicates. The observation-only filter storage records
set/get order and values; no Detour query implementation is claimed. Original
methods contain no inserted tracing, branch edits or replacement decision body.
All liquid callbacks require a non-null data argument and bitwise-equal owner
XYZ, including signed zero. The harness initializes all authored liquid data
fields before the original code can inspect them.

## Source decisions and bounded Rust behavior

Original `createFilter` checks walk then swim, sets include and exclude, then
calls `updateFilter`. Non-PLAYERBOTS walking contributes only ground; swimming
contributes water and magma/slime. Exclude is zero. Current `CanFly` is not read
by this source method. The bounded Rust API separately rejects capabilities
outside walking, nonswimming, nonflying; its host must derive and own those
capabilities. Broader source controls do not broaden runtime admission.

`Unit::IsInWater` delegates to `TerrainInfo::IsInWater`. `CanCheckLiquidLevel`
checks VMap height calculation first and short-circuits the actual grid query
when enabled. Otherwise it requires a grid. With a valid check it requests all
liquids, mask 15. **Every nonzero status**, including ABOVE_WATER and WATER_WALK,
makes this predicate true. If true, the outer `||` skips `Unit::IsUnderwater`.

When the first predicate is false, original `Unit::IsUnderwater` delegates to
`TerrainInfo::IsUnderWater`. That method independently checks coverage and uses
water/ocean mask 10. Only its UNDER_WATER bit makes the predicate true. A dry
source run therefore makes exactly two liquid reads with masks `15,10`, no
filter read, no additional include write, and no `getNavTerrain` call.

When either predicate is true, the original `getNavTerrain` executes another
all-liquids query. NO_WATER returns ground. Otherwise its exact single-type
switch maps water/ocean to water, magma/slime to magma/slime, and other values,
including mixed/deep/WMO flags, to ground. These wet observations are source-only
controls. Rust does not implement or invent a wet filter: every nonzero status
is explicitly unsupported, even a second nonzero status without the underwater
bit. Some independently scripted mask responses deliberately cannot describe a
coherent real liquid surface; they witness source branch behavior only.

The sim entry point is `create_ground_filter(capabilities, query)`. The query
receives `LiquidMask::AllLiquids` then `LiquidMask::WaterOrOcean` and returns a
typed `NoWater`, constructed only from status zero. A callback error stops
immediately. `Filter` has private fields and read-only include/exclude/branch
getters. `NoWater` is a scalar marker, not a loaded-product capability. The
public application adapter must obtain each status at the actual owned creature
position from its loaded snapshot and establish `CanCheckLiquidLevel=true` from
actual tile coverage. A missing grid cannot be promoted to dry proof merely
because the original scalar control skipped both reads. No caller-supplied dry
boolean belongs at that adapter boundary.

## Fixtures and ordered traces

The 87 [authored rows](fixtures/route_filter/cases.test-fixture) cover all four
walk/swim pairs, enabled-height/grid short-circuit controls, absent-grid control,
signed-zero owner coordinates, first-predicate nonzero statuses and OR
short-circuit, second-predicate status-mask checks, original wet type-switch
controls, and both NO_WATER returns from `getNavTerrain`.

The TSV header explicitly names every field. Coordinates are eight-digit
binary32 hexadecimal words; all other numeric fields are decimal. Each input
record is 13 little-endian u32 words:

```text
walk swim height grid x y z status0 type0 status1 type1 status2 type2
```

The status/type triples are consumed by actual liquid-call ordinal, not by
predicate name. Unconsumed replies do not imply calls. Status inputs are limited
to 0..15 and type flags to 0..63; these are authored experiment bounds. Nonboolean
callbacks, nonfinite owner coordinates, out-of-domain liquid inputs and partial
records fail closed. Empty input produces no records.

Output fields are:

```text
include exclude initial_include include_calls exclude_calls read_calls
height_calls grid_calls liquid_calls masks trace
```

`masks` is the comma-separated actual query sequence, or `-` for no calls.
Trace letters are `T` type ID, `W` walk predicate, `S` swim predicate, `I` include
write, `E` exclude write, `R` include read, `H` height-enabled callback, `G` grid
callback, `A` all-liquids query, and `O` water/ocean query. For example:

- Ground dry with actual-grid callback: `TWSIEHGAHGO`, masks `15,10`
- Ground dry with enabled-height short-circuit: `TWSIEHAHO`, masks `15,10`
- Missing coverage source-only control: `TWSIEHGHG`, no liquid queries
- First predicate true: `TWSIEHGARAI`, masks `15,15`
- Only second predicate true: `TWSIEHGAHGORAI`, masks `15,10,15`

The five Rust tests compare the generated rows, reject wet/unsupported
capabilities before producing a filter, check callback failures/order, validate
all nonzero u8 status rejections and verify source masks/flags/hash metadata.
The 13 Python guard tests check complete-file hashes and missing files, LF
normalization, unchanged functions/notices, extraction/marker/location drift,
source-package and fixture drift without overwriting, compiler/symbol failures,
byte-identical regeneration, exact source traces and malformed binary inputs.

## Reproduction

From the repository root, with pinned originals already available locally:

```sh
TOOLS=crates/benilla-native-geometry-tests/tools
MANGOS=../upstream-source/mangos-classic-8ec338a1704e7dcb1c0213eb7ed58f9231ade40f
RECEIPTS=../verification/native-route-owner-filter
python3 "$TOOLS/route_filter_generate.py" "$MANGOS" --check --receipt-dir "$RECEIPTS/oracle-normal"
python3 "$TOOLS/route_filter_generate.py" "$MANGOS" --check --sanitize --receipt-dir "$RECEIPTS/oracle-ubsan"
python3 "$TOOLS/test_route_filter_generate.py" "$MANGOS"
```

`--check` never replaces the package or fixture; package drift fails before
compilation. Missing source/compiler is an error, never a skip. The audited
compiler is G++ 14.2.0, `x86_64-linux-gnu`, with little-endian IEEE binary32 and:

```text
-std=c++20 -O2 -fno-inline -march=x86-64 -mno-fma -fno-fast-math
-ffp-contract=off -Wall -Wextra -Werror
```

`--sanitize` adds `-fsanitize=undefined,float-cast-overflow
-fno-sanitize-recover=all`. No transcendental arithmetic is involved. Normal and
UBSan observations must reproduce identical fixture bytes. Receipts retain the
full generated source, original-method symbol table and exact compiler/profile
command. Source hashes are validated before compilation, separately from the
checked-in fragment package.

Original source remains GPL-2.0-or-later; the authored harness, generator, guards,
observations and Rust port are GPL-3.0-or-later. Existing attribution is in the
[geometry notice](../../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt),
[AUTHORS](../../benilla-sim/licenses/AUTHORS.md) and
[COPYRIGHT](../../benilla-sim/licenses/COPYRIGHT.md).
