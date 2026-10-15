# Bounded shared source sine/cosine profile

`source_trig` supplies one private statically generic reducer and polynomial,
with a native arithmetic adapter and a software arithmetic adapter. It is a
selected numeric profile for finite binary32 radians within inclusive ±TAU
(`0x40c90fdb`), not a general libm or a claim that native platform libraries agree.
Both public functions return raw `[sine, cosine]` binary32 bits:

- `sin_cos_radians_bits_native` retains the existing VMap unfused native
  binary64 expression order under native IEEE RNE/gradual-underflow assumptions
- `sin_cos_radians_bits_soft` uses software IEEE round-to-nearest/ties-even,
  gradual underflow and explicitly rounded binary32 conversions throughout

Both accept signed zero and subnormal inputs, reject nonfinite/out-of-range
inputs before evaluation, and never call host sine or cosine. The native
adapter does not set or inspect the host floating-point environment. The
software adapter uses the existing `facing::soft` arithmetic through a narrow
crate-private boundary and is independent of host floating-point arithmetic.
The coefficient arrays, source branch selection and polynomial occur only once
in [the shared core](src/source_trig/core.rs); the adapters contain no polynomial.

## Source and licensing

The selected source is [GNU libc 2.41](https://github.com/bminor/glibc/tree/74f59e9271cbb4071671e5a474e7d4f1622b186f/sysdeps/ieee754/flt-32),
commit `74f59e9271cbb4071671e5a474e7d4f1622b186f`. Full-file SHA-256 identities,
also exported in `source_trig::SOURCE_SHA256`, are:

- `s_cosf.c`: `a6e8c6c2da36b0b859bd83d4c6725667f55ca2d23488dcafb803c23023e55694`
- `s_sincosf.h`: `40c282f82dae4225bccdb6444624cbd2d9030ebfeca98ad5b248dc9d9e168448`
- `s_sincosf_data.c`: `2ad440b58b5baac791c82c70d0d8b61788875f4ecfecb05a5f5e80977c161514`
- `s_sinf.c`: `d2781d7f1f91557d24f342b25c3cc2356fec8f0cf1dfaccce63917e7900ae440`
- `sincosf_poly.h`: `1c42f6f9a22fb814ccb879d25ac6f277440b5f7264bbeec07c1fddcbf0a9542f`

The original copyright is **Copyright (C) 2018–2025 Free Software Foundation,
Inc.**, under **LGPL-2.1-or-later**. The full [original notice](src/source_trig/GLIBC-NOTICE.txt)
and [LGPL license text](src/source_trig/GLIBC-LICENSE.txt) accompany the core,
which retains its original grant. The bounded Rust translation was modified on
2026-10-15 by Benilla fork contributors. Authored adapters and boundaries are
GPL-3.0-or-later; the combined crate remains GPL-covered. The separate facing
atan2 implementation's MIT grant is unchanged. No game data is included.

## Exact bounded arithmetic

The original `abstop12` compares the sign-cleared top twelve bits against
`0x3f4` and `0x398`; these are deliberately not exact PI/4 and 2^-12 numeric
comparisons. The tiny branch returns the original sine bits and cosine 1.
The finite admitted range fits `reduce_fast`, with `TOINT_INTRINSICS=0`:

1. Multiply binary64 x by the original scaled 2/PI coefficient
2. Convert toward zero to signed int32, add `0x800000`, then signed right shift 24
3. Subtract the separately rounded binary64 `n * HPI` product
4. Select the original quadrant sign, negate all cosine coefficients where required,
   and evaluate the original separate binary64 products and sums
5. Round each final result to binary32

The software integer conversion permits the expected INEXACT status and rejects
INVALID_OP. Its signed shift and all polynomial expression ordering match the
source. No implicit or explicit FMA is used. Exception flags, errno, arbitrary
angles and other rounding modes are outside this interface.

## Existing Euler caller and evidence

The app's [VMap wrapper](../benilla-app/src/net/p2p/world/native_geometry/products/vmap/trig.rs)
retains its exact degree validation and `(PI * binary64(angle) / 180) as binary32`
conversion, then calls the shared native adapter. Its existing 202 signed source
boundary cases and 1,292,314-sample digest are unchanged, as are its selected
profile identity and geometric oracle expectations. The [VMap provenance](../benilla-app/src/net/p2p/world/native_geometry/products/vmap/PROVENANCE.md)
and [five-file oracle manifest](../benilla-native-geometry-tests/tools/vmap_glibc_source_hashes.sha256)
remain applicable.

The [near-point oracle contract](../benilla-native-geometry-tests/tools/near_point_candidate_README.md)
adds 258 independent radian cases and a 737,002-sample radian digest, plus
source-candidate vectors and a separate deterministic candidate digest.
It compiles the pinned original glibc bodies rather than this Rust translation.
Tests compare both adapters with those exact bits, separately exercise domain
rejection and integer truncation, and retain signed-zero/subnormal behavior.
Differential samples are not exhaustive proofs over every binary32 angle, and
no host-libm parity, cross-platform execution or gameplay admission follows.
