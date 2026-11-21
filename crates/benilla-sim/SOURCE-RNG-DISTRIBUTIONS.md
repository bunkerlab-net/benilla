# Source distributions, wrappers, and shuffle

`source_rng::SourceRandom` implements scalar source distributions and
CMaNGOS utility wrappers over a caller-owned raw engine. It does not
replace native RNG or migrate gameplay callers. Engine state and lifetime
rules remain in [SOURCE-RNG.md](SOURCE-RNG.md); source hashes and licences
are in [PROVENANCE.md](PROVENANCE.md#source-rng-distributions-and-shuffle).

## Source and numeric scope

CMaNGOS Classic is pinned to
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. Utility formulas come from
`src/shared/Util/Util.cpp:68-119` and `Util.h:80-137`.
The retained Debian GCC/libstdc++ `12.2.0-14+deb12u1` headers define the
integer, real, and shuffle algorithms. Citations below use those exact
captured headers, not the host compiler's implementation.

- `bits/uniform_int_dist.h:241-362` defines scalar integer sampling.
- `bits/random.tcc:3354-3385` defines canonical real generation.
- `bits/random.h:1864-1873` defines the real affine transform.
- `bits/stl_algo.h:3673-3679,3695-3758` defines paired positions and shuffle.

All real arithmetic uses the existing locked `rustc_apfloat` dependency,
with IEEE round-to-nearest/ties-even and gradual underflow. Subtraction,
multiplication, and addition round separately. No host FPU or fused
multiply-add supplies a result. The retained reference build declares
`-ffp-contract=off -fno-fast-math` in `CMakeCache.txt:140`.

The reached source engine is MT19937 with a `2^32` range. Full-width
64-bit generator arithmetic is supported through `RawGenerator`, including
64-bit Lemire downscaling. This does not add an MT19937-64 engine family.
Canonical real generation supports power-of-two generator ranges,
including `2^32` and `2^64`. Non-power-of-two URBG real generation depends
on pinned binary80 `logl` rounding to choose its draw count. It is unreached
by the pinned utility wrappers and explicitly returns
`Error::UnreachedRealGenerator` before drawing. That numeric path is not
ported. The libstdc++ internal bulk `__generate` interfaces are not source
utility or shuffle callsites and are not exposed here.

## Integer distributions

`uniform_u32`, `uniform_i32`, `uniform_u64`, and `uniform_i64` accept inclusive
bounds. They preserve full-width and equal-bound ranges, unsigned range
arithmetic, and final signed conversion. Equal bounds still consume a raw
word. Inverted bounds refuse before a draw.

Sampling follows the source's range comparison:

1. Equal generator and target ranges return the next normalized raw word.
2. A larger generator range uses the uint32/uint64 Lemire multiply-and-reject
   paths where the source selects them, or its division fallback otherwise.
3. A smaller generator range recursively samples the high part, draws the
   low part, and rejects an out-of-range or wrapped candidate as a whole.

Rejection loops have no invented attempt limit. The 64-bit multiplication
path uses `u128`. Signed extreme bounds never pass through overflowing
signed subtraction. Every source-defined unsigned wrap remains unsigned.

## Real distributions and conversions

`canonical_f32` and `canonical_f64` preserve the source's digit order and
rounding. For MT19937, float generation consumes one word. Double generation
consumes two words, with the first word as the low digit, not a shifted
53-bit fraction. A canonical result rounded to one becomes the predecessor
of one. `uniform_f32` and `uniform_f64` then apply the rounded affine
transform. Its final addition can still round to the upper endpoint; no
extra clamp changes that source result.

Real ranges must satisfy the distribution's finite, ordered, representable
width domain. Unsupported ranges refuse before engine consumption.
`double_to_float_bits` preserves IEEE narrowing, signed zero, subnormals,
and the selected source profile's NaN conversion. It does not use a native
Rust `as f32` result as authority.

## CMaNGOS wrappers

All pinned utility names have a method on `SourceRandom`:

- `irand`, `urand`, `frand`, `irand_default`, and `urand_default`.
- `rand_norm`, `rand_norm_f`, `rand_chance`, and `rand_chance_f`.
- `roll_chance_f`, `roll_chance_i`, `chance_u`, and `roll_chance_combat`.
- `die_roll` and `pick_random_value` for the templated utility mechanisms.

The no-argument `irand` source overload uses the distribution's actual
`0..=INT32_MAX` default, despite its broader header comment. `urand_default`
uses `0..=UINT32_MAX`. `rand_chance_f` draws a double distribution before
narrowing, so it consumes two MT words, unlike a float distribution.
`roll_chance_f` widens its float argument and compares it with a double
roll. NaN compares false but still consumes that roll. `roll_chance_i`
compares with `irand(0,99)`, without a shortcut for zero or full chance.

`chance_u` preserves `std::max(0.0f,chance)` ordering, the binary32 product
by 100, and `roundf` ties away from zero. Thus negative values and NaN
produce zero. A source-undefined float-to-uint32 conversion refuses rather
than saturating. Combat rolling skips only a rounded zero. Otherwise its
comparison is strictly `scaled > urand(1,10000)`, not `>=`. Even scaled
10000 can fail when the source draws 10000.

`Die<SIDES>` retains at most 255 sides and a default side. `set` ignores an
out-of-range uint8 side before evaluating its chance conversion. `die_roll`
uses the supplied value, skips zero weights, accumulates with uint32 wrap,
and tests `supplied <= cumulative`. It never draws another word.
`pick_random_value` borrows an existing slice and returns a reference to the
selected element without copying the whole input. It draws through urand
even for one element. An empty selection refuses.

The ordinary WSE helper now accepts a continuing `DrawCursor` and trace
sink and uses this same integer implementation. Its signed-range refusal
still precedes lazy engine initialization. Local WSE MERSENNE_RANDOM remains
the separate engine helper described in the engine contract.

## Shuffle

`shuffle` operates in place without allocation or copying its elements.
It preserves source forward swaps, including self-swaps and duplicate
values. Empty and one-element ranges consume no words.

When `engine_range / count >= count`, the source handles an even count's
first swap separately, then generates each pair of positions from one
uniform draw over `b0*b1` possibilities. Otherwise it samples each forward
swap separately. For MT19937, counts 65535 and 65536 straddle this branch.
The implementation does not substitute descending Fisher-Yates or cache
unused random bits between distribution calls.

## Trace interface and refusals

The caller owns the engine, stream identity, `DrawCursor`, and infallible
`FnMut(TraceEvent)` sink. This module performs no trace allocation or I/O.
Keep the cursor with the engine across separate borrowed contexts.

Each call records its operation, input words, unique span occurrence,
parent span, raw begin/end positions, and typed output bits or error.
Nested wrappers, distributions, canonical generation, recursive integer
sampling, and paired shuffle draws retain their actual parent links.
Raw events retain every word and position. Rejection events retain the
rejected attempt's raw range and algorithm. Explicit no-draw events cover
rounded-zero combat, supplied-value Die rolls, and trivial shuffle.
Swap events retain output positions without copying generic values. Die
side events retain each reached nonzero weight and cumulative threshold.
The caller supplies any external source-owner identity; this is not an
F3.02 execution-binding implementation.

Unsupported arithmetic refuses without a fake result. Cursor overflow
refuses before consuming the next engine word. Earlier completed draws or
swaps remain consumed if a later operation refuses. There is no rollback,
search-ahead, retry cap, or resynchronisation with expected output.

## Evidence and exclusions

`tools/source_distributions_oracle.cpp` uses the pinned C++ headers with
contraction and fast math disabled. Its checked-in JSONL fixture has 2623
rows, SHA-256
`c18ff371b04f16eddcb0516edeab7d88d3ee658ff2a6f5602b8c031d9cfa606e`.
It records 100 mixed MT rounds, exact raw cursors and complete post-engine
state hashes, 64-bit and fallback branches, large shuffle boundaries,
float conversions, and chance conversion vectors. Scripted generators are
test-only arithmetic inputs, never production replacements for MT19937.
Authored tests separately check rejection records, nested spans, endpoint
repair, supplied-value reduction, no-draw cases, and refusal behavior.

The optional semantic test shares `BENILLA_F1_RNG_TRACE` with the raw-engine
test. With no private default file, it prints an explicit SKIP. An explicit
unreadable path fails. It accepts only the original startup-through-third-
map0 prefix with SHA-256
`574813534352216ffbc4f7ca884db032bb3cfa08e292489438d60e1d9a8a836b`.
It reads wrapper arguments and complete initial MT states as inputs,
recomputes wrapper results and raw spans, and compares recorded results and
raw words only as expected outputs. Unwrapped raw calls are recomputed
independently to preserve each stream between wrapper calls. Missing,
surplus, reordered, cross-engine, or mismatched words refuse.

This projection expects 27865 recorded wrapper calls/results and all 69586
raw MT words from eight states across 15371848 rows. It does not prove
unrecorded inline consumers, direct shuffle chronology, or the caller's
branch decisions. Runtime callsite cutover, creation/model and level branch
integration, failed Wander consumption, and EventAI chronology are not
implemented or certified by these utility tests. No private tape contents,
format change, native state, gameplay-answer adapter, or admission is added.

The recorded semantic owners are `urand`: 17050, `irand`: 7883,
`rand_chance`: 2068, `rand_norm_f`: 576, and `urand_default`: 288.
Their spans contain 29933 raw words. The other utility wrappers have
authored coverage, not recorded semantic coverage in this prefix.
