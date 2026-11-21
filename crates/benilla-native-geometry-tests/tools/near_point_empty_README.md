# Full empty-list near-point source oracle

This test-only oracle runs the complete original
`WorldObject::GetNearPointAt` from CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, including every free-angle attempt
and terminal first-XY restoration. Height and static/dynamic LOS callbacks
return **authored scalar replies**. These fixtures are not real-world terrain,
loaded VMap/dynamic-tree, world-startup, movement, spell or gameplay-admission
proof. The native provider's separately loaded-product tests establish their
own evidence.

## Unchanged source boundary

The [manifest](near_point_empty_source_hashes.sha256) guards 23 complete original
files, allowing only CRLF-to-LF normalization. The
[source package](near_point_empty_source_fragments.test-fixture) preserves the
original copyright/license notices, the complete selector CPP/header, original
glibc numeric files and these complete methods:

- `Entities/Object.cpp:2297`: `WorldObject::GetNearPoint2dAt`
- `Entities/Object.cpp:2306–2412`: `WorldObject::GetNearPointAt`
- `Entities/Object.cpp:1387`: `WorldObject::IsWithinLOS`
- `Entities/Unit.cpp:11835`: `Unit::UpdateAllowedPositionZ`
- `Maps/Map.cpp:2668`: `Map::IsInLineOfSight`
- `Maps/GridDefines.h:169`: `MaNGOS::NormalizeMapCoord`

The complete original `ObjectPosSelector.cpp` is inserted unchanged into the
translation unit. Its original header is copied unchanged to the include path.
The original real `std::multimap` members are fresh and empty. Its constructor,
checks, both `InitializeAngle` overloads, `NextAngle`, `NextSideAngle` and
`NextUsedAngle` all execute as called by the original outer method. There is no
replacement selector algorithm, fallback sentinel, manual angle sequence or
handwritten C replacement body. `AddUsedArea` is also compiled from the complete
original file; the outer method's used-position population remains commented out
exactly as in the source.

The ground branch of the complete Unit method executes with a non-null,
nonflying, nonswimming searcher. Its strict invalid-height comparison and
conditional Z assignments are original. Other authored capability callbacks
abort if reached. The configuration callback records an explicit row flag;
`DetectPosCollision=true` registration is preserved as provenance, without
claiming a running world configuration loader. `detect=0` is a source-only
control, outside the collision-enabled production slice.

The entire original XY body computes both binary32 multiply/add pairs and
normalizes coordinates. `cosf`/`sinf` names bind to tracing wrappers that record
only the raw argument and then forward to unchanged pinned glibc
`source_cos`/`source_sin`; they never calculate an expected coordinate. The
selector's original global `atan(float-expression)` is checked at compile time
to have binary64 return type and binds the unchanged original glibc `s_atan.c`.
No host atan/trig, Rust implementation, native fixture producer, game install,
network or world database participates in expected-value generation.

Height observations finalize the previous adjusted Z by reading the actual
outer call's Z variable before the next XY computation or next height callback.
The terminal observation is finalized after the unchanged outer method returns.
This instrumentation adds no branch, coordinate, height or LOS decision to any
original source body.

## Source control-flow witnesses

The 42 [case rows](fixtures/near_point_empty/cases.test-fixture) and 1,467
[ordered height rows](fixtures/near_point_empty/heights.test-fixture) include:

- Primary acceptance; first-plus, first-minus and later acceptance
- Primary/fallback strict vertical rejection, static short-circuit and dynamic
  rejection; all three complete-exhaustion paths
- Exact `+PI/-PI` emission and neighbors, original zero-radius/distance repairs,
  a zero half-angle with all 628 emitted offsets, subnormal radius and overflowing
  ratio diagnostics
- Raw negative fallback angles and the unnormalized upper bound `4116cbe4`,
  rather than reducing raw angles to a different trig domain
- Coordinate clamping on every candidate, distinct target pose LOS origin,
  target-eye/candidate-adjusted-feet asymmetry, `ignoreM2Model=false`
- Left-associated distance sum, strict vertical threshold neighbors, invalid
  ground preservation and signed-zero Z preservation
- Full exhaustion restoring first XY while passing the last adjusted Z into
  the final height query, without resetting it to initial Z and without any
  final vertical test or static/dynamic LOS

For `exhausted_final_invalid_preserves_last_z`, the last free candidate has Z=13.
The terminal query uses first XY with Z=13 and receives invalid ground -200000;
the original Unit branch retains Z=13. The final authored LOS booleans are both
true but never called. The result is the source's exhausted return, not proof
that the result has support or line of sight.

Five named `loaded_*_scalar_companion` rows use the coordinates and expected
scalar replies of separately tested loaded-product cases. Their inputs remain
authored here. In particular, the carried-Z companion queries Z=11 for seven
actual candidates, then queries Z=7 at restored first XY; the final query is
observed with no LOS. The wall companion scripts five static rejections and a
sixth clear result. A loaded test must independently establish those replies;
the scalar fixture cannot establish geometry or dynamic-tree behavior. The
blocked-exhaustion companion scripts all seven candidate static LOS calls as
blocked, then observes the final first-XY height query and exhausted return
without any eighth LOS call.

Extreme-size, subnormal and disabled-configuration controls characterize this
source observation domain. They do not expand any bounded runtime admission
policy or authorize a producer to accept an unsupported request.

## Fixture schema

Both files are tab-separated with exact named headers. Binary32 words use eight
hex digits. Counts/flags are decimal. `case` joins the files; labels are descriptive
and may repeat across input variants.

Case inputs:

```text
case label pos_x pos_y pos_z distance searcher_radius target_radius angle
 target_x target_y target_z collision_height detect
 ground_script static_script dynamic_script
```

The three script fields are comma-separated lists of equal length. Ground values
are binary32 hex; LOS values are 0/1. The height-call ordinal selects the reply,
then repeats the last entry after the list is exhausted. LOS entries are indexed
by height-call ordinal, not by LOS-call count. A supplied reply does not imply a
call occurred.

Case outputs:

```text
outcome result_x result_y result_z xy_calls height_calls static_calls
 dynamic_calls config_calls radius_calls trace
```

Outcomes are `primary-accepted`, `fallback-accepted`, `exhausted` and
`collision-disabled`. Trace letters are `I` actual XY call, `C` configuration,
`R` target radius, `H` ground height, `P` target position, `S` static LOS,
`D` dynamic LOS. Source selector operations are not replaced by trace events.

Height rows:

```text
case height_index xy_index raw_angle query_x query_y query_z ground_reply
 adjusted_z static_called dynamic_called static_reply dynamic_reply
 los_src_x los_src_y los_src_z los_dest_x los_dest_y los_dest_z swim ignore_m2
```

Every ordinary candidate has a zero-based `xy_index` and its exact raw angle.
`xy_index=-1` marks the final height query with no new XY computation. Its
`raw_angle=00000000` is an unused slot; it does not claim that the last selector
angle or a new XY call was zero. Uncalled LOS argument words are zero and must
be interpreted only with `static_called`. Dynamic callbacks verify bitwise
identity with all preceding static-call arguments. `swim` and `ignore_m2` are
verified false.

The binary harness input is little-endian u32: the eleven scalar input words,
`detect`, script length, then each `(ground,static,dynamic)` triple. It rejects
nonfinite scalar inputs, negative distances/radii/collision height, out-of-domain
initial coordinates/angles, nonboolean flags, empty/oversized scripts and partial
records. Those are authored experiment bounds, not attributed source branches.

## Reproduction and numeric profile

From the repository root, with the pinned originals already available locally:

```sh
TOOLS=crates/benilla-native-geometry-tests/tools
MANGOS=../upstream-source/mangos-classic-8ec338a1704e7dcb1c0213eb7ed58f9231ade40f
ATAN=/tmp/benilla-atan64-glibc
TRIG=/tmp/vmap-glibc-source
RECEIPTS=../verification/native-near-point-empty-selector
python3 "$TOOLS/near_point_empty_generate.py" "$MANGOS" "$ATAN" "$TRIG" --check --receipt-dir "$RECEIPTS/oracle-normal"
python3 "$TOOLS/near_point_empty_generate.py" "$MANGOS" "$ATAN" "$TRIG" --check --sanitize --receipt-dir "$RECEIPTS/oracle-ubsan"
python3 "$TOOLS/test_near_point_empty_generate.py" "$MANGOS" "$ATAN" "$TRIG"
```

`--check` never replaces the checked-in source package or fixtures. Package drift
fails before compilation. Missing/mismatched source or compiler is an error,
never a skipped test.

The compiler is audited GCC/G++ 14.2.0, `x86_64-linux-gnu`, little-endian IEEE
binary32/binary64, `FLT_EVAL_METHOD=0`, FE_TONEAREST, FTZ=DAZ=0. The unchanged
[source_atan generator](../../benilla-sim/tools/source_atan_generate.py) compiles
original `s_atan.c` at O2 with its original non-FMA EMULV branch. The unchanged
[candidate generator](near_point_candidate_generate.py) compiles the original
float trig objects at O0 under its established unfused profile. Their numeric
objects are linked into the full-path C++20 observer, compiled at O2 with:

```text
-march=x86-64 -msse2 -mfpmath=sse -mno-fma -fno-fast-math
-ffp-contract=off -frounding-math -fno-builtin -fno-strict-aliasing
-Wall -Wextra -Werror
```

The atan C compilation additionally uses `-fexcess-precision=standard`.
`--sanitize` adds `-fsanitize=undefined,float-cast-overflow
-fno-sanitize-recover=all` to all three components. Normal and UBSan observations
must reproduce identical fixture bytes.

Receipts retain generated full source, unchanged selector header, compatibility
headers, numeric original files, compiler version/profile, exact link command,
symbol tables and disassembly. The atan subreceipt retains exact C/compiler
commands; the top-level reproduction log prints the float-trig and combined
commands. Symbol checks forbid host atan/trig imports, require original source
methods and both forwarding bindings, verify two separate binary32 multiply/add
pairs in the XY method and reject fused arithmetic anywhere in the final binary.

The 13 Python guard tests verify all 23 source hashes and missing files, complete
unchanged methods/CPP/header/notices, marker/extraction/package drift, compiler
failure, symbol/binding/FMA checks, byte-identical fixture reproduction, ordered
callbacks and conditional LOS, exact PI/zero/domain witnesses, final carried-Z
restoration and fail-closed binary input bounds.

Original CMaNGOS GPL-2.0-or-later and glibc LGPL-2.1-or-later notices remain with
original source. Existing attribution is in the
[geometry notice](../../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt),
[AUTHORS](../../benilla-sim/licenses/AUTHORS.md) and
[COPYRIGHT](../../benilla-sim/licenses/COPYRIGHT.md). Authored harness, generator,
guards and observations are GPL-3.0-or-later.
