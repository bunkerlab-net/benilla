# Chase straight-attempt prerequisites

Policy: `cmangos-8ec338a1-chase-straight-gate-soft-rne-v1`.

`chase_gate` implements the numeric predicates and eye-coordinate arithmetic of
the straight-path attempt condition in pinned CMaNGOS Classic
`TargetedMovementGenerator.cpp:502`. It does not execute a route query or grant
movement admission. A passing value only permits the caller to attempt the next
actual loaded-provider operation.

## Exact predicate order

The original condition evaluates, with left-to-right short circuiting:

1. `owner.IsWithinDist3d(goal.x, goal.y, goal.z, 200.f)`
2. `abs(owner.z - goal.z) < 5.f`
3. `owner.IsWithinLOS(goal.x, goal.y, goal.z + target.GetCollisionHeight())`
4. `!owner.IsInWater()`
5. `!target.IsInWater()`

`numeric_gate_bits(owner_xyz, goal_xyz, owner_combat_reach)` returns the first
numeric refusal, `Distance` or `Vertical`, or `Pass`. The coordinate overload of
`WorldObject::IsWithinDist3d` uses only the owner's combat reach, not its bounding
radius or any target size. Its exact comparison is squared distance strictly
less than the square of binary32 `200.f + owner_combat_reach`.

Every subtraction and product rounds individually. Squared distance is the
left-associated binary32 sum `(dx*dx + dy*dy) + dz*dz`. There is no square root,
FMA, wider retained intermediate, inclusive comparison or quantization. The
vertical predicate follows the distance result, using the absolute value of the
rounded binary32 subtraction and a strict comparison against `5.f`.

`los_endpoints_bits(owner_xyz, goal_xyz, owner_height, target_height)` returns
`[owner-eye, goal-plus-target-height]`. The goal's Z addition precedes the owner's
Z addition as in the argument evaluation followed by `IsWithinLOS`. The target's
current position is not the LOS destination. The caller must perform actual LOS
with `ignore_m2=false` and only then query owner water, followed by target water.

For an NPC, source `Unit::IsInWater` delegates at the NPC's actual current
position to `TerrainInfo::IsInWater`, which queries raw liquid status with mask
15 and the `getLiquidStatus` default collision height `2.03128f`, bits
`0x4002007e`. This is not the unit's model collision height. Any nonzero raw
status is wet, including ABOVE_WATER and WATER_WALK. `terrain_is_in_water(u8)`
models that truth conversion for all raw bytes; it does not establish coverage.

For a player, the original override reads only its cached
`ENVIRONMENT_FLAG_IN_WATER` bit. A consumer must use the genuine runtime-owned
player witness; neither the NPC predicate nor a fresh terrain result can replace
it. Source `CanCheckLiquidLevel=false` returns false water without a status
query. Missing loaded coverage must still fail closed in a native provider,
rather than become a manufactured dry observation.

## Numeric and authority boundary

The Rust arithmetic uses the existing pinned `rustc_apfloat::ieee::Single`
nearest/ties-even software profile with gradual underflow. It does not depend
on the host floating environment. Feet coordinates must be finite within
inclusive ±32760. Combat reach must be finite and nonnegative; signed zero and
subnormals are accepted. Collision heights may be any finite signed value,
matching the source's retention of negative nonzero height products. Nonfinite
inputs and overflowing eye-coordinate results are refused. A finite combat reach
may produce an infinite squared distance threshold, which is retained exactly as
in the source comparison. Eye coordinates are not clamped back to feet bounds.

The scalar reach input is not a new authoritative reach receipt. The caller
must bind it to the actual initialized owner and authenticate its existing
source model receipt against owned AI combat reach. This module establishes no
loaded model provenance, entity identity, map/phase/generation match, current
environment state, real LOS/liquid geometry, route, spline or admission.

## Independent source oracle

Run, from the repository root:

```
python3 crates/benilla-sim/tools/test_chase_gate_oracle.py
python3 crates/benilla-sim/tools/generate_chase_gate_fixture.py UPSTREAM_ROOT --check
python3 crates/benilla-sim/tools/generate_chase_gate_fixture.py UPSTREAM_ROOT --check --sanitize
```

The generator verifies eight complete LF-normalized source hashes from
[`chase_gate_source_hashes.sha256`](tools/chase_gate_source_hashes.sha256), then
copies the following source fragments unchanged:

- The complete five-term Chase condition and its two numeric subexpressions
- `WorldObject::GetDistance(float, float, float, DistanceCalculation, bool)`,
  `WorldObject::IsWithinDist3d`, and `WorldObject::IsWithinLOS(float, float, float, bool)`
- `Unit::IsInWater`, `TerrainInfo::IsInWater` and the inline Player override
- Original distance/liquid/environment enums, liquid masks and data structure
- Original declarations, retaining the non-transport distance default,
  `ignoreM2Model=false`, and liquid query default `collisionHeight=2.03128f`

These execute in an explicitly authored host. Position/reach/height getters,
map LOS and terrain coverage/status callbacks are synthetic observations. Player
flags use an authored logging field whose bitwise operation records the cached
read. Unused radius/combined-reach/transport branches abort if called. The host
is not the original translation unit, complete `DispatchSplineToPosition`
method, production geometry implementation or movement lifecycle. The condition
is extracted, not re-created, and no route-building body is executed.

The audited native profile is GCC 14.2.0, x86_64-linux-gnu, C++17, `-O0`,
`-march=x86-64 -mno-fma -fno-fast-math -ffp-contract=off -frounding-math`,
`FLT_EVAL_METHOD=0`, FE_TONEAREST, FTZ=DAZ=0. Each run rejects altered FTZ,
DAZ and rounding modes before any source execution. The sanitizer mode enables
undefined-behavior and float-cast-overflow sanitizers with no recovery. The
compiler is required, never silently skipped or substituted.

The 2,548-case fixture contains strict distance and vertical nextafter boundaries,
both signs, signed zero/subnormal/large finite inputs, distinct owner reach and
radius controls, target reach and current-position controls, unequal eye heights,
two vectors whose gate outcome distinguishes fused/reassociated arithmetic,
2,048 generated bounded numeric inputs, all 16 source-status combinations, all
256 cached flag bytes and every callback short-circuit edge. Four grid-coverage
combinations record source behavior without claiming native loaded coverage. The
callback host asserts actual queried NPC positions, mask 15 and default height;
fixtures retain exact LOS bits, the false ignore-M2 default and ordered trace.

Rust tests compare exact numeric bits/decisions and independently check source
callback observations. Their ordered replay is explicitly test-only; actual
loaded-provider order, identity refusal and error propagation are tested by the
consumer. These fixtures do not establish live gameplay or full pathfinding.

## Source and license

CMaNGOS Classic revision `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, CMaNGOS
Project contributors, GPL-2.0-or-later. The full-file hash manifest identifies
TargetedMovementGenerator.cpp, Object.cpp/.h, Unit.cpp, GridMap.cpp/.h,
GridMapDefines.h and Player.h. The generator reports each unchanged fragment
hash and generated harness hash before execution. CRLF normalization changes
line endings only; any remaining source mismatch aborts before extraction.

Rust translation and authored oracle host: 2026 Benilla fork contributors,
GPL-3.0-or-later. Existing [authors](licenses/AUTHORS.md),
[copyright](licenses/COPYRIGHT.md), [upstream GPLv2](licenses/LICENSE) and
[applicable GPLv3](licenses/GPL-3.0.txt) notices apply. No client assets, captured
world rows or game-install content are included.
