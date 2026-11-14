# First-candidate acceptance source oracle

This independent test-only oracle executes the complete pinned
`WorldObject::GetNearPointAt` through its first candidate and stops at fallback
entry. Its pin is CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. Every input is authored numeric data.
It does not run a native producer, Rust implementation, database, game install,
full selector search, movement, spell cast, world startup or gameplay admission.

## Original source and bounded execution

The [full-file hash manifest](near_point_primary_source_hashes.sha256) guards
nine originals. The [source package](near_point_primary_source_fragments.cpp.txt)
retains the following complete methods and the original notices:

- `Entities/Object.cpp:2306`: complete `WorldObject::GetNearPointAt`
- `Entities/Object.cpp:1387`: complete `WorldObject::IsWithinLOS`
- `Maps/ObjectPosSelector.cpp:25,81,94`: complete constructor, `CheckAngle` and
  `CheckOriginalAngle`
- `Maps/ObjectPosSelector.h`: complete original header, copied unchanged into
  the compiler's include directory
- `Entities/Unit.cpp:11835`: complete `Unit::UpdateAllowedPositionZ`
- `Maps/Map.cpp:2668`: complete `Map::IsInLineOfSight`
- `Entities/Object.cpp:2297,1379`: complete `GetNearPoint2dAt` and
  `IsWithinLOSInMap` as context only; these two methods are not executed here

The package also retains the exact `Object.h:996` declaration with
`ignoreM2Model = false`, `World.cpp:674` configuration registration with
`DetectPosCollision=true`, and original invalid-height/default-radius/selector
factor constants. World configuration storage is an authored callback with an
explicit per-row flag; the registration line establishes source provenance,
not a claim that the server configuration loader executes in this harness.

The complete first-candidate method is inserted unchanged into the
[authored harness](near_point_primary_oracle.cpp). Its ordering is:

1. Obtain the initial XY and assign `init_z = z = posZ`
2. Read `DetectPosCollision`; the disabled source branch updates allowed Z and
   returns without the distance, selector or LOS checks
3. Form binary32 `(distance2d + searcher_bounding_radius) + target_radius`
4. Construct the original selector with real, fresh, empty `std::multimap` lists
5. Leave the source's commented-out used-position visitor commented out
6. Execute the original `CheckOriginalAngle`; both lists are empty
7. Update the nonflying, nonswimming searcher's allowed ground Z
8. Require strict `abs(init_z - z) < dist` before calling LOS
9. Return only if LOS succeeds; otherwise throw at `InitializeAngle()` entry

`InitializeAngle` is an explicitly authored fallback sentinel. It checks that
both lists remain empty, records `fallback-required`, and throws. `NextAngle`
and `NextUsedAngle` abort if ever reached. The entire original `GetNearPointAt`
remainder stays in the compiled method and the package, but no fallback algorithm
is substituted, executed or claimed. A fallback observation is not a successful
position, even though the candidate coordinates remain available for diagnosis.

The original selector constructor's `atan` executes through host libm. Its
stored angle result is unobserved by this empty-list first-return path and does
not appear in the fixture. No constructor-angle bit parity, host-libm parity,
occupied-list behavior or full-selector parity is claimed. The compatibility
`Common.h` supplies no declarations; the original selector header's standard
multimap construction and the original constructor/check methods are real.

## Authored scalar boundaries

- Initial XY is a recorded authored callback. It verifies exact forwarding of
  the unexpanded distance and angle. Source-exact trig, arithmetic and map
  clamping remain the separate [initial-candidate oracle](near_point_candidate_README.md)
- Ground `Map::GetHeight` records candidate XY, initial Z and `swim=false`, then
  returns the authored scalar. The complete original Unit method does the Z
  comparisons and assignments. CanFly and CanSwim are fixed false. Null
  searcher, swimming, flying and object-ground branches abort if reached
- Both static VMap and dynamic LOS record scalar arguments and return authored
  booleans. The actual geometry is not loaded or traversed here. The complete
  original Map method enforces static-then-dynamic short-circuit evaluation

Source invalid ground does not force rejection: when ground is `-200000` or
at/below the strict `-100000` validity threshold, allowed Z remains initial Z.
The first candidate can still pass its distance and LOS conditions. No support
threshold, extra proximity epsilon or extra ground-validity condition is added.
The signed-zero equality controls also retain the input Z bits rather than
unconditionally assigning an equal ground value.

The exact LOS origin is the target object's actual `(x,y,z+collisionHeight)`;
the destination is raw `(candidateX,candidateY,adjustedZ)`, with no destination
height added and `ignoreM2Model=false`. The separate spell LOS path is not an
equivalent replacement for this query. `IsWithinLOSInMap` context explicitly
shows its destination-height addition, which `GetNearPointAt` does not invoke.
One diagnostic supplies a distinct target pose and source `posXYZ` to prove
where the LOS origin is read; this does not authorize stale or unrelated poses
in a real provider.

Loaded terrain/VMap products, real candidate trig, actual empty dynamic-tree
traversal, configuration authority, deadlines and shared work budgets require
separate integration tests. Authored scalar fixtures establish call arguments,
arithmetic/comparison behavior and control flow only. The production bounded
slice requires collision detection enabled; the `collision-disabled` fixture
observations are separate source controls, not accepted production results.

## Numeric observations and fixture schema

[primary-acceptance.test-fixture](fixtures/near_point_primary/primary-acceptance.test-fixture)
contains 301 rows, including invalid-height boundaries, all static/dynamic LOS
boolean combinations, strict height-difference neighbors on both signs, zero
and subnormal radii/distances, signed-zero Z, disabled configuration controls,
target-eye/candidate-feet asymmetry and a sum-order witness.

The sum-order witness has distance `3f800000`, searcher radius `33800000`,
target radius `33800000` and height difference `3f800000`. The original
left-associated additions round the sum to `3f800000`, so strict equality fails
and LOS is never called. Reassociating the radii first would yield `3f800001`
and change the outcome. Zero total distance likewise fails even with zero Z
difference. No `<=` substitution is permitted.

The tab-separated header names every field. Inputs are:

```text
case label pos_x pos_y pos_z distance searcher_radius target_radius angle
candidate_x candidate_y ground target_x target_y target_z collision_height
detect static_los dynamic_los
```

Outputs are:

```text
outcome result_x result_y result_z xy_calls ground_calls
ground_x ground_y ground_z ground_swim static_calls dynamic_calls
los_src_x los_src_y los_src_z los_dest_x los_dest_y los_dest_z ignore_m2
config_calls radius_calls fallback_calls trace
```

Float values are eight hexadecimal binary32 digits. Flags and counts are
decimal. Uncalled LOS argument slots are zero and are meaningful only when
`static_calls=1`. The dynamic callback verifies all six argument words and the
ignore-M2 flag equal the preceding static callback. The outcomes are
`primary-accepted`, `fallback-required` and `collision-disabled`.

Trace letters: `I` initial XY, `C` configuration, `R` target radius, `H` ground
height, `P` target position, `S` static LOS, `D` dynamic LOS, `F` fallback entry.
The observed traces are `ICRHPSD`, `ICRHF`, `ICRHPSF`, `ICRHPSDF` and `ICH`.

The harness rejects nonfinite inputs, negative distances/radii/collision height,
nonboolean flags and partial input records. Those are bounds on this experiment,
not new rejection branches attributed to CMaNGOS. Negative zero remains allowed.

## Reproduction and fail-closed checks

```sh
python3 near_point_primary_generate.py MANGOS_ROOT --check --receipt-dir RECEIPTS/normal
python3 near_point_primary_generate.py MANGOS_ROOT --check --sanitize --receipt-dir RECEIPTS/ubsan
python3 test_near_point_primary_generate.py MANGOS_ROOT
```

The Linux x86-64 profile uses C++17, IEEE binary32/binary64, `FLT_EVAL_METHOD=0`,
RNE and preserved subnormals. Flags are `-O0 -fno-fast-math -ffp-contract=off
-frounding-math -fno-builtin -fno-strict-aliasing -Wall -Wextra -Werror`.
`--sanitize` adds `-fsanitize=undefined,float-cast-overflow
-fno-sanitize-recover=all`. Normal and UBSan executions must produce identical
fixture bytes. Compiler versions, generated source, selector header, symbol
list and disassembly can be saved with `--receipt-dir`. Symbols for the original
methods are required; fused multiply-add instructions are rejected.

The eleven Python tests verify all nine full-file hash guards, missing files
and compiler, exact source bodies/notices/header, extraction and marker drift,
source package drift before compilation, complete fixture reproduction,
argument forwarding and short-circuit order, strict sum/difference witnesses,
invalid-ground acceptance, signed-zero preservation, LOS endpoint distinctions,
and authored wrapper-domain rejections. A source/compiler mismatch is an error,
never a skipped test. `--check` does not overwrite either fixture or package.

Original CMaNGOS GPL-2.0-or-later notices remain in the package and generated
translation unit. Existing attribution is in the
[geometry source notice](../../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt),
[AUTHORS](../../benilla-sim/licenses/AUTHORS.md) and
[COPYRIGHT](../../benilla-sim/licenses/COPYRIGHT.md). Authored harness, generator,
tests and observations are GPL-3.0-or-later.
