# Binary64 atan and empty-selector source oracle

These authored numeric inputs exercise one explicitly selected Linux numeric
profile. Expected bits are produced only by independently compiled original
glibc and CMaNGOS source. The generator reads no Rust code, native producer,
world rows, database, game installation, held compiler patch, or network source.
It does not substitute `atanf`, `atan2`, a host transcendental, or an authored
selector algorithm.

## Pinned original sources and licenses

- CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:
  complete `ObjectPosSelector.cpp`, `ObjectPosSelector.h`, `ObjectDefines.h`,
  and `Common.h` are verified by full-file hashes. These sources are
  GPL-2.0-or-later, copyright CMaNGOS contributors
- GNU libc 2.41 `74f59e9271cbb4071671e5a474e7d4f1622b186f`:
  `sysdeps/ieee754/dbl-64/s_atan.c`, `dla.h`, `mydefs.h`, `uatan.tbl`, and
  `atnat.h` are the complete numeric dependency set. These original IBM
  Accurate Mathematical Library files retain the FSF 2001–2025 copyright
  and LGPL-2.1-or-later notices
- The original `math/math-underflow.h`, `sysdeps/generic/math-barriers.h`,
  and `COPYING.LIB` are also hash-checked. Underflow/barrier bodies compile
  unchanged. The checked license is the complete original LGPL text

All glibc files were obtained from the [official GNU libc source repository](https://sourceware.org/git/?p=glibc.git;a=tree;hb=74f59e9271cbb4071671e5a474e7d4f1622b186f).
For example, the authoritative [binary64 atan source](https://sourceware.org/git/?p=glibc.git;a=blob;f=sysdeps/ieee754/dbl-64/s_atan.c;hb=74f59e9271cbb4071671e5a474e7d4f1622b186f)
and [double-length arithmetic macros](https://sourceware.org/git/?p=glibc.git;a=blob;f=sysdeps/ieee754/dbl-64/dla.h;hb=74f59e9271cbb4071671e5a474e7d4f1622b186f)
can be inspected directly. The [manifest](../../source_atan_source_hashes.sha256)
pins full bytes after CRLF-to-LF normalization only. Generation neither downloads
sources nor refreshes pins. Existing [CMaNGOS attribution](../../../licenses/COPYRIGHT.md)
applies; the Rust numeric port carries its own original glibc notices and license.
The authored generator, harness, tests and observations are GPL-3.0-or-later.

The five numeric dependency identities, in manifest order, are:

- s_atan.c: `786f7cf3abc44135498c253da678569247f40a13e7158886b9ad065185e7ca5f`
- dla.h: `f06aa3a847e7fdbb1e9f0969a8a3d32ce8cb33165612e8434aadb1f3ee3a9f6e`
- mydefs.h: `7d8a1e3c8a0769ca4e778704fc32f0517207ec7ce79cc494217927933fdef9c5`
- uatan.tbl: `dcc2d24c0e5c83de489cae4143e4753f73702b04ed905b183c0da493647bf4ab`
- atnat.h: `a4c3b10f0b81fbf902f636801f871b53763c5f3b0e5df6c68334dd3197c162de`

## Numeric and compiler profile

The executable requires GCC and G++ 14.2.0 targeting `x86_64-linux-gnu`.
The observed distribution compiler is Debian 14.2.0-19. The profile is GNU C11
for unchanged glibc source and C++20 for unchanged selector methods, with:

```text
-O2 -march=x86-64 -msse2 -mfpmath=sse -mno-fma
-fno-fast-math -ffp-contract=off -frounding-math -fno-builtin
-fno-strict-aliasing -Wall -Wextra -Werror
```

C additionally uses `-fexcess-precision=standard`. Static assertions require
IEEE binary32/binary64, 32-bit int and `FLT_EVAL_METHOD=0`. Runtime checks
require `FE_TONEAREST`, gradual underflow and MXCSR FTZ/DAZ disabled. The
sanitized profile adds `-fsanitize=undefined,float-cast-overflow
-fno-sanitize-recover=all` and must reproduce identical fixture bytes.

The selected original `dla.h::EMULV` is its non-FMA Veltkamp/Dekker split,
using `CN=134217729`. This branch choice is distinct from disabling implicit
contraction: `__FP_FAST_FMA` could otherwise select explicit builtin FMA.
The generator rejects FMA feature macros at compilation and fused instructions
in disassembly. This establishes the baseline SSE2 source algorithm, not the
machine-dependent IFUNC/FMA selection of an arbitrary installed glibc.

Only three private integration seams are adapted:

1. `endian.h` defines the checked little-endian word indices
2. The unused glibc alias header is empty, while `-D__atan=atan` binds the
   original C body directly to a local strong `atan` symbol
3. `SET_RESTORE_ROUND(FE_TONEAREST)` checks the already-required rounding mode
   rather than changing/restoring it. Under this admitted environment both
   numerical behavior and preservation of the rounding mode are identical

The source expressions, table entries, constants, `EMULV`, `ESUB`, underflow
forcing and barrier macros are unchanged. Symbol inspection requires local
`T atan` and rejects unresolved host `atan`, `atanf`, `atan2`, or `atan2f`.
Standard bit-sign/absolute-value and floating-environment support may remain
host calls. Exception flags, errno, other rounding modes, x87 excess precision,
and host-library equivalence are outside this bit-result contract.

The authentic original CMaNGOS translation unit, with both its full pinned PCH
and no PCH, was separately observed using GCC14/libstdc++14 and Debian Boost
1.83 headers. It performs binary32 multiply/divide, widens the rounded ratio,
calls global `atan(double)`, then narrows. This oracle independently asserts
the same global overload result type. It does not claim every CMaNGOS platform
selects that overload or that a full server build ran.

## Exact selector source and arithmetic

The complete original selector header is copied unchanged and real fresh
`std::multimap` instances execute. Complete original methods are extracted
without expression rewrites: constructor, both `InitializeAngle` overloads,
`NextAngle`, `NextSideAngle`, and `NextUsedAngle`. The original CMaNGOS notice
is retained in the generated translation unit. Neither used-area insertion
nor an occupied-list case is substituted or claimed.

The complete original constant lines are extracted from their pinned files:
factor `1.8f`, `DEFAULT_WORLD_OBJECT_SIZE=0.388999998569489f`, and
`M_PI_F=float(M_PI)`. The numeric path preserves:

1. Separate zero-size and zero-distance repairs, including negative zero
2. Binary32 `1.8f * size`, then a separately rounded binary32 division
3. Exact widening to binary64, original binary64 `atan`, narrowing to binary32
4. Binary32 initialization `0.0f - half_angle` on each empty side
5. Binary64 `(double(half_angle) + 0.01)`, then binary64 addition of the current
   step, then one binary32 narrowing at compound assignment
6. Original plus/minus ordering, strict outer `<PI`, inner `>PI`, signed
   multiplication, and exhausted-list behavior

The wrapper accepts finite nonnegative binary32 size and distance, including
signed zeros. An overflowing intermediate ratio is positive infinity and is
evaluated by the original source. A tiny ratio can round to zero. These are
numeric experiment boundaries, not new CMaNGOS rejection rules. The source
loops terminate naturally; an authored `>628` guard fails the experiment and
never supplies a replacement successful return. `NextUsedAngle` must be false.

Named witnesses include:

- `exact_pi_0`: size `42142315`, distance `3f800000`, half `3fc72456`, six
  offsets ending at `40490fdb`/`c0490fdb`; both terminal steps equal PI
- `exact_pi_runtime_bounded_0`: size `40142315`, distance `3d800000` produces
  that same source sequence with size below four
- `mixed_double_increment`: size=distance=`3f800000` yields first offset
  `3c23d70a`; erroneously evaluating both increment operations in binary32
  would yield `3c23d700`
- The minimum-subnormal size and maximum-finite distance yield zero half-angle
  and the maximum 628 signed offsets; maximum-finite size produces an infinite
  ratio and correctly narrows the source HPI result

## Fixture schemas and coverage

`atan.tsv` has 2,224 rows: `name input_binary32 atan_binary64 narrowed_binary32`.
Every expected result is from unchanged C. Cases include signed zero, smallest
and largest subnormals, infinity, all finite exponent boundaries, neighboring
binary32 values at all five source magnitude thresholds, and both direct and
reciprocal neighborhoods of all 240 table-rounding midpoints. Word fields are
hexadecimal. Names `boundary_A_-1` through `boundary_E_1` identify magnitude
boundaries; `table_direct_n_delta` and `table_reciprocal_n_delta` use n=16..255
and delta=-1,0,1. Python floating operations generate only input vectors.

`selector.tsv` has 155 complete source sequences:
`name size dist ratio half final_plus final_minus count angles...`.
Count is decimal; every other numeric field is a hexadecimal binary32 word.
The matrix includes signed-zero repairs, subnormals, overflow and tiny ratios.
Exact-PI and increment-rounding cases supplement the size-neighbor cases.

`sample-digests.tsv` covers:

- 522,625 atan inputs: unsigned raw words from Python
  `range(0, 0x7f800001, 4093)`, then `0x7f800000` and `0x80000000`. SHA-256
  consumes each input u32, result u64 and narrowed u32, all little endian
- 4,096 selector inputs: restart xorshift32 at `0xa7a64105`, with shifts
  13/17/5 and 32-bit wrapping. Take two consecutive words modulo `0x7f800000`
  as size then distance. SHA-256 consumes size, distance, ratio, half,
  final-plus, final-minus, count and every returned angle, all little-endian u32

`coefficients.sha256` covers all 1,687 `cij[241][7]` u64 coefficient words in
row-major little-endian order. The generator separately decodes both original
endian tables, verifies their complete shape and equality, then hashes them.
No expected coefficient or output comes from the Rust port.

## Reproduce and inspect

From the repository root, with local source trees at the exact revisions:

```sh
python3 crates/benilla-sim/tools/source_atan_generate.py MANGOS_ROOT GLIBC_ROOT --check --receipt-dir /tmp/atan-normal
python3 crates/benilla-sim/tools/source_atan_generate.py MANGOS_ROOT GLIBC_ROOT --check --sanitize --receipt-dir /tmp/atan-ubsan
python3 crates/benilla-sim/tools/test_source_atan_generate.py MANGOS_ROOT GLIBC_ROOT
```

Omit `--check` only to regenerate reviewed fixture changes. Receipts include
every generated source/header, unchanged numeric source/table, commands,
compiler/profile details, full symbol lists and disassembly. The 11 Python
tests exercise all source hash guards, missing source, extraction/marker drift,
compiler-profile rejection, both original endian tables, complete fixture and
digest reproduction, unsupported wrapper inputs, zero/default/overflow cases,
and exact-PI/double-increment regressions. Missing or mismatched sources and
compiler are errors, never skipped tests. `--check` never overwrites fixtures.

Finite samples are not exhaustive proof over every binary32 input. This oracle
establishes the selected atan/empty-selector arithmetic prerequisite only. It
does not prove occupied-list behavior, complete `GetNearPointAt`, geometry,
runtime work budgets, dynamic exclusions, world startup, or gameplay admission.
