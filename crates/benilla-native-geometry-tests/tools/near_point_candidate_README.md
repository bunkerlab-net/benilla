# Initial near-point candidate source oracle

This test-only oracle observes `WorldObject::GetNearPoint2dAt` from CMaNGOS
Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, Object.cpp lines 2297–2304.
It does not observe a full `GetNearPointAt` search, Z update, collision/LOS
decision, world startup, motion, combat reach, contact distance, or admission.
All inputs are authored numeric values, with no game-install or database data.

The complete Object.cpp and GridDefines.h bytes are pinned in
`near_point_candidate_source_hashes.sha256`. The exact method, exact
`NormalizeMapCoord` body and its four map macros are extracted unchanged.
Their original notices are retained both in the compiled translation unit and
`near_point_candidate_source_fragments.cpp.txt`.

Trig uses the five complete original glibc 2.41 float-trig files, pinned by the
existing `vmap_glibc_source_hashes.sha256`, at glibc commit
`74f59e9271cbb4071671e5a474e7d4f1622b186f`. All five files, including original
notices, are also retained verbatim in the source package. The generator copies
them unchanged and compiles the original C files separately. Private compatibility
headers provide bit casts, branch hints and the underflow evaluation barrier;
the unreachable nonfinite-error helper aborts. No math implementation is replaced.
`TOINT_INTRINSICS=0` selects the original generic reducer. `SINF=source_sin`
and `COSF=source_cos` bind the original functions, and two preprocessor aliases
bind the unchanged CMaNGOS call sites to those exact symbols. There is no
degrees conversion and no host-libm trig fallback.

## Profile and domains

The source observation profile is x86-64 Linux, C11/C++17, IEEE binary32 and
binary64, `FLT_EVAL_METHOD == 0`, RNE and preserved subnormals. Compile flags are
`-O0 -fno-fast-math -ffp-contract=off -frounding-math -fno-builtin
-fno-strict-aliasing`. The original candidate executes an ordinary binary32
multiply then ordinary binary32 addition on each axis, then source map clamping.
The map macros evaluate to clamp bits `0x46855455`, including their double-literal
`0.5` comparisons. Symbol and disassembly checks require the locally defined
original trig symbols, reject unresolved host trig, and require two distinct
`mulss` and two distinct `addss` instructions in the candidate function.

The authored wrapper domain is finite input XY within inclusive ±32760
(`0x46fff000`), any finite nonnegative distance, and finite normalized angle in
inclusive `[0, float(TAU)]` (`0x40c90fdb`). Negative zero is accepted for distance
and angle. Input XY is not preclamped. The direct-radian trig experiment accepts
both signs through inclusive `|angle| <= float(TAU)`. Rejection fixtures describe
these wrapper domains, not extra behavior of the extracted CMaNGOS function.

## Fixtures

All generated fixtures live in `fixtures/near_point_candidate/` and use the
`.test-fixture` suffix. Numeric case rows are tab-separated hex binary32 words.

- `trig-cases`: 258 signed edges; columns angle radians, sine, cosine
- `candidate-cases`: 3,012 cases; columns target X/Y, distance, angle radians,
  candidate X/Y. Comment lines identify authored case groups
- `trig-rejections`: nine rejected angles
- `candidate-rejections`: 37 rejected target X/Y, distance, angle tuples
- `trig-grid`: count and SHA-256 of little-endian `u32(angle,sine,cosine)` records
- `candidate-grid`: count, xorshift32 seed and SHA-256 of little-endian
  `u32(x,y,distance,angle,candidate_x,candidate_y)` records

The bounded trig sample has 737,002 records, not exhaustive coverage. In order,
emit each unsigned word from zero through TAU with step 4093, positive then
negative; then emit binary32 `step / 16384` for integer steps 0 through 102943,
positive then negative; finally emit positive and negative TAU. Do not deduplicate.
The 32,768 candidate samples use xorshift32 seed `0x4e504341`, operations
`state ^= state << 13; state ^= state >> 17; state ^= state << 5`, truncating
to `u32`. Each row draws four words. X/Y retain the sign and reduce the magnitude
modulo `0x46fff001`; distance magnitude is modulo `0x7f800000`; angle magnitude
is modulo `0x40c90fdc`. Draw order is X, Y, distance, angle.

Cases include signed zero, subnormals, minimum normal, glibc's `2^-12` and
top-12-bit small-branch thresholds, quadrant and TAU neighbors, source clamp
neighbors on both signs, maximum finite distance, and origins beyond the output
clamp. A current-pose bearing source fixture emits minimum subnormal radians
`00000001`; at origin with distance `7f7fffff` this candidate is
`46855455,34ffffff`, proving that direct tiny radians remain observable.

The explicit non-FMA witness is input
`bf0a5141,bf576aa6,3f800001,3f800000`, which the original source maps to
`00000000,00000000`. A fused alternative would produce
`31a51400,b2a25570`. The Python guard test establishes that diagnostic difference
with exact rational arithmetic, without supplying the source oracle's expected
candidate result. Origin `46ea6000,c6ea6000`, distance `46ea6000`, angle `40490fdb`
produces `00000000,c6855455`, distinguishing source arithmetic from preclamping.

## Reproduction

Supply the pinned CMaNGOS checkout and a directory containing the five original
glibc files. No compiler, source, symbol check or failed comparison is skipped.

```sh
python3 near_point_candidate_generate.py MANGOS_ROOT GLIBC_FLT32 --check
python3 near_point_candidate_generate.py MANGOS_ROOT GLIBC_FLT32 --check --sanitize
python3 test_near_point_candidate_generate.py MANGOS_ROOT GLIBC_FLT32
```

`--sanitize` adds `-fsanitize=undefined,float-cast-overflow
-fno-sanitize-recover=all`. Both profiles must produce byte-identical checked
fixtures and digests. `--receipt-dir PATH` optionally saves compiler versions,
the compiled CMaNGOS harness, symbol lists and disassembly. The existing VMap
202-case and 1,292,314-sample degree-trig fixtures and digest are untouched;
the Python tests pin their original fixture-file hashes.

## Separate raw fallback-angle numeric experiment

The same original source bodies also supply separately named `fallback-trig`
and `fallback-candidate` modes. They observe only numeric trig and
`GetNearPoint2dAt`; they do not execute a selector or the `GetNearPointAt` fallback
search. The source method and glibc source package are unchanged, and the six
original fixture files retain their previous byte hashes. The original `trig`
and `candidate` wrapper domains remain unchanged.

Both new modes admit raw finite radians in the asymmetric inclusive interval
`[-binary32 PI, RNE32(binary32 TAU + binary32 PI)]`, numeric endpoints
`0xc0490fdb` and `0x4116cbe4`. This is the final angle domain for a normalized base
plus an offset in ±binary32 PI. No normalization or modulo is applied. The
candidate's XY and distance limits are identical to the initial experiment.
These domain checks are authored wrapper restrictions, not new behavior inserted
into the unchanged source method.

The upper endpoint is exactly 9.424777984619140625, less than 10, and has
`abstop12 == 0x411 < 0x42f == abstop12(120.0f)`. The scaled source 2/PI coefficient
is below `2^24`, making `abs(x * coefficient) < 10 * 2^24`; integer truncation
and the subsequent `+0x800000` are strictly within signed int32 bounds. Thus
the admitted domain is wholly within the original fast-reducer branch. The
Python guard tests check the exact dyadic endpoint sum, source top-bit threshold
and integer bounds. Exception flags and arbitrary libm inputs remain out of scope.

Additional files in `fixtures/near_point_candidate/`:

- `fallback-trig-cases`: 251 vectors, columns raw angle, sine, cosine
- `fallback-candidate-cases`: 7,200 vectors, columns target X/Y, distance, raw
  angle, candidate X/Y
- `fallback-trig-rejections`: 11 rejected raw angles
- `fallback-candidate-rejections`: 38 rejected X/Y, distance, raw-angle tuples
- `fallback-trig-grid`: 84,602 records, count and SHA-256 of little-endian
  `u32(raw_angle,sine,cosine)`
- `fallback-candidate-grid`: 32,768 records, seed and SHA-256 of little-endian
  `u32(x,y,distance,raw_angle,candidate_x,candidate_y)`

For the trig digest, emit the positive domain first: unsigned words from zero
through `0x4116cbe4` with step 65521, then binary32 `step / 4096` for steps 0
through 38603, then the upper endpoint. Next emit the negative domain with sign
bit set: unsigned magnitudes from zero through `0x40490fdb` with step 65521, then
binary32 `step / 4096` for steps 0 through 12867, then the lower endpoint. Do not
deduplicate. For the candidate digest, use the same four-word xorshift32 draw
order and coordinate/distance mapping as the initial experiment, but seed
`0x4e504642`. Preserve the angle draw's sign; its magnitude is modulo
`0x40490fdc` for negative draws and `0x4116cbe5` otherwise.

The upper raw endpoint emits sine `0xb2ccde2e` and cosine `0xbf800000`.
Subtracting binary32 TAU first yields angle `0x40490fda` and different sine bits.
A negative minimum-subnormal raw angle with maximum distance at zero XY emits
`46855455,b4ffffff`. These witnesses prevent silently normalizing or discarding
raw tiny angles. The normal and sanitizer commands above check both original
and fallback numeric fixtures, with no skipped compilation or source checks.
