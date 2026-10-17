# Bounded source binary64 atan

`source_atan::atan_nonnegative_f32_bits` is the numeric prerequisite for the
source near-point selector's binary32 ratio, widened exactly to binary64 for
`atan`, then narrowed to binary32. It returns all three raw bit patterns:
the widened input, the binary64 atan result, and the final binary32 result.
This is a selected numeric profile, not a replacement for general libm and
not a claim of native platform-library equivalence or gameplay admission.

The accepted input set is nonnegative finite binary32, positive infinity,
and both signed zeros. Positive infinity accommodates binary32 ratio overflow.
Negative nonzero inputs and every NaN are rejected before arithmetic; the
source's signed-zero tiny return is deliberately preserved. There is no
general binary64 input API and no host `atan`, `atanf` or `atan2f` call.

## Source identity and licensing

The selected source is GNU libc 2.41, commit
`74f59e9271cbb4071671e5a474e7d4f1622b186f`, under
`sysdeps/ieee754/dbl-64`. These full-file SHA-256 identities are exported,
in this order, as `source_atan::SOURCE_SHA256`:

- `s_atan.c`: `786f7cf3abc44135498c253da678569247f40a13e7158886b9ad065185e7ca5f`
- `dla.h`: `f06aa3a847e7fdbb1e9f0969a8a3d32ce8cb33165612e8434aadb1f3ee3a9f6e`
- `mydefs.h`: `7d8a1e3c8a0769ca4e778704fc32f0517207ec7ce79cc494217927933fdef9c5`
- `uatan.tbl`: `dcc2d24c0e5c83de489cae4143e4753f73702b04ed905b183c0da493647bf4ab`
- `atnat.h`: `a4c3b10f0b81fbf902f636801f871b53763c5f3b0e5df6c68334dd3197c162de`

The originals were acquired from the official sourceware glibc repository at
that revision. Oracle acquisition checks normalize CRLF to LF only, then
hash each complete file against the [manifest](tools/source_atan_source_hashes.sha256).
The [translated core](src/source_atan/core.rs) and
[table](src/source_atan/table.rs) retain the IBM Accurate Mathematical Library
attribution and **Copyright (C) 2001–2025 Free Software Foundation, Inc.**, under
**LGPL-2.1-or-later**. The full [original notice](src/source_atan/GLIBC-NOTICE.txt)
and [GNU Lesser General Public License](src/source_atan/GLIBC-LICENSE.txt)
accompany them. The license is the pinned revision's unmodified `COPYING.LIB`,
SHA-256 `dc626520dcd53a22f727af3ee42c770e56c97a64fe3adb063799d8ab032fe551`.
The bounded Rust translation was modified on 2026-10-15 by Benilla fork
contributors. Authored API and tests are GPL-3.0-or-later; the combined crate
remains GPL-covered. No game assets or database content are included.

## Numeric policy

Policy identifier:
`glibc-2.41-atan-binary64-from-binary32-soft-rne-non-fma-v1`.

Every operation uses the existing `facing::soft::{D,S}` software IEEE
round-to-nearest/ties-to-even implementation with gradual underflow. Source
binary32-to-binary64 widening is exact. The final narrowing is a separate
software rounding, so a binary32 approximation to atan is not interchangeable.

The translated arithmetic retains:

- The exact `A`, `B`, `C`, `D`, `E` strict branch thresholds from `atnat.h`
- The original six-coefficient polynomial and its multiplication/addition order
- All 241 × 7 binary64 table bit patterns, joined directly from original high
  and low words; both original endian sections were checked to agree
- The `TWO52 + 256*u` table-index rounding, followed by C-style integer
  truncation and subtracting 16, including even-index midpoint behavior
- Reciprocal correction using the source **non-FMA** `EMULV` macro from `dla.h`,
  with `CN = 1 + 2^27` and each intermediate operation separately rounded
- The table index computed from the rounded reciprocal **before** its low-part
  correction, and the original `ESUB` correction in the large-input branch
- The original binary64 half-PI result for inputs at or above `E`, including
  positive infinity

The admitted nonnegative domain makes the nonzero `copysign` calls no-ops;
negative zero returns before those branches. NaN propagation, negative
nonzero inputs, exception flags, errno and alternate rounding modes are
outside the API. Source underflow-flag forcing is omitted because it changes
no returned number; even the least binary32 subnormal widens to a normal
binary64 input. The FMA-selected alternative `EMULV` is not this policy.

## Independent evidence and limits

The [oracle tools](tools/source_atan_generate.py) compile the hash-checked,
unchanged original C numerical files with baseline non-FMA binary64 arithmetic,
not this Rust translation and not the host atan implementation. The
[fixtures](tools/fixtures/source_atan/atan.tsv) record each input's original
binary64 result and separately narrowed binary32 result.

The [tests](tests/source_atan.rs) compare both result widths, independently
check exact widening, retain signed-zero and positive-infinity behavior,
reject all negative/NaN classes, and require every A/B/C/D/E branch threshold
plus its adjacent binary32 values. They also require every one of the 240
direct table midpoints and 240 reciprocal table transition centers, each
with adjacent binary32 values. Midpoint input bits are checked independently
using integer rational round-to-nearest/ties-to-even arithmetic.

The 2,224 named original-C vectors include all those boundaries and all 254
normal binary32 exponent transitions with adjacent inputs. A private table
test checks every translated coefficient against the
[coefficient digest](tools/fixtures/source_atan/coefficients.sha256). Its
generator independently parses both original endian sections, rejects any
shape or value disagreement, and hashes the 1,687 agreed u64 words little-endian.

A separate [sample digest](tools/fixtures/source_atan/sample-digests.tsv)
checks both result widths for a stride-4093 sweep of nonnegative binary32
encodings through positive infinity, plus explicit positive infinity and
negative zero, totaling 522,625 observations. Digest records are input `u32`,
atan `u64`, and narrowed `u32`,
each little-endian; the expected digest is produced only by the original C
oracle. The test also ties the public source identities to the oracle manifest
and checks the retained original license bytes.

This exhausts the named branch/table boundaries, not every binary32 input.
It does not establish all native libm profiles, FPU exception equivalence,
cross-platform execution, complete near-point selector behavior, navigation
or collision search, or runtime admission. Those remain separate contracts.
