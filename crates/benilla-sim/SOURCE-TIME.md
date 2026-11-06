# Source-time arithmetic

`source_time` translates pure timer arithmetic from CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It accepts explicit millisecond
observations and diffs. It never reads a host clock. The source licence,
authorship, file hashes, and preserved notices are in
[PROVENANCE.md](PROVENANCE.md#source-timer-arithmetic).

## Integer and error contract

The selected source platform is LP64. Source `time_t` values use Rust `i64`;
source `uint32` values use `u32`, independently of the Rust host's pointer size.
Signed addition or subtraction that would overflow returns
`Error::SignedOverflow` before mutation. This refuses source-undefined
arithmetic rather than wrapping or saturating it. The negative-current clamp
runs only after a defined addition, including when the diff is zero.

`narrow_milliseconds(i64) -> u32` preserves the low 32 bits of an already
observed relative millisecond count, including negative counts. This is the
integer cast in `src/shared/Util/Util.cpp:60-64` and
`src/shared/Util/Timer.h:41-46`, not a new clock sampler or calendar conversion.

## WorldTimer

`world_timer_diff(old, new)` implements `Timer.h:50-60` literally:

- When `old <= new`, return `new - old`.
- Otherwise, return `min((UINT32_MAX - old) + new, old - new)`.

Thus `0xfffffff0 -> 5` returns 20, not wrapping subtraction's 21.
`100 -> 99` returns 1, and `UINT32_MAX -> 0` returns zero. Neither
monotonic-clock subtraction nor a generic wrapping-clock helper is equivalent.

`WorldTimer` starts with both samples zero. `tick(elapsed_ms)` moves current
to previous, narrows the supplied relative observation into current, and
returns the source diff. `tick_time` and `tick_prev_time` expose the samples.
This preserves `Util.cpp:42-57` without its host-clock dependency.

## IntervalTimer and expiry trackers

`IntervalTimer` preserves `Timer.h:77-103`:

- Both interval and current start at zero.
- `update(i64)` adds first, then clamps a negative current to zero.
- `passed()` includes equality, with no special case for a zero interval.
- `reset()` subtracts one interval only when passed. It does not clear current
  or repeatedly subtract until current falls below the interval.
- `set_current(i64)` and `set_interval(i64)` assign directly, including negative
  values. `set_current(0)` is distinct from `reset()`.
- Both getters return `i64` without narrowing.

`TimeTracker` preserves signed `time_t` expiry from `Timer.h:132-143`.
`update(i64)` subtracts without a zero clamp. `passed()` means expiry is
zero or negative. A negative diff increases expiry. `reset(i64)` assigns it.

`ShortTimeTracker` is a separate unsigned timer from `Timer.h:145-162`, beyond
the signed tracker's cited range. Its default expiry is zero. `new`, `update`,
`reset`, and `get_expiry` use `u32`. Update saturates at zero when diff reaches
or exceeds expiry. It cannot represent negative expiry or accept a signed diff.
The unrelated `ShortIntervalTimer` is not part of this module.

## Map interval accumulator

`MapIntervalAccumulator::new(u32)` widens the configured interval into its
LP64 `IntervalTimer`, as in `src/game/Maps/MapManager.cpp:40` and
`MapManager.h:180`. `update(u32)` follows `MapManager.cpp:194-227`:

1. Widen and accumulate the world diff.
2. Return `Ok(None)` below the threshold.
3. At the threshold, narrow the entire current to `u32`, clear current to
   zero, and return `Ok(Some(diff))` once.

For interval 100, diffs `50, 49, 1, 250` return
`None, None, Some(100), Some(250)`. There are no fixed substeps or retained
remainders. With interval `UINT32_MAX`, diffs `UINT32_MAX - 1, UINT32_MAX`
dispatch `4294967293`, preserving the explicit uint32 casts at
`MapManager.cpp:203-205,216` after accumulation in `i64`. A zero interval
passes even for a zero diff and returns `Some(0)`.

The return value is the diff shared by map updates and unload checks. The
caller owns all map work, worker completion, and unload ordering. This
arithmetic-only module clears at return; it does not expose an in-flight
MapManager or claim to execute its barrier and unload loop.

## Evidence and exclusions

The authored `source_time::tests::` tests exercise source wrap/backward cases,
sample narrowing, timer defaults, exact integer interface types, signed diffs,
threshold equality, one-interval reset, explicit current reset, negative expiry,
unsigned saturation, undefined-overflow refusals without mutation, and map
accumulation and dispatch narrowing. They require no installed game data.

This module does not change Profile, native State, RNG, format `NATIVE_FORMAT_VERSION`, or saves.
It adds no serialization or world/map phase scheduler. Authored arithmetic
tests and source inspection are not a sealed F1 world/map timer projection,
whole-prefix parity, or native-world admission.
