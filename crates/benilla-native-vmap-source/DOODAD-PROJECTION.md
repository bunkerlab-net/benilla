# Explicitly profiled WMO doodad projection

`PreparedDiscovery::project_doodad` resolves one eligible candidate from its
private captured parent, model and MODD data. It produces validated numeric
placement fields and the exact record spans before and after the four-byte
**generated ID hole**. These spans are not a complete `dir_bin`,
a complete map record or collision admission. Generated identities are retained
separately by the [encounter ledger](ENCOUNTER-IDS.md).

## Deliberate numerical compatibility decision

The explicit policy `g3d-portable-libm-0.2.16-v1` preserves G3D operation order,
but selects pinned portable `libm = 0.2.16` double asin/atan2 and float sqrt in
place of the C++ platform's native library. This is a meaningful, versioned
compatibility choice: it does **not** promise every native CMaNGOS installation
would emit identical bits. No tolerance is used to hide differences.

The policy reuses the separately pinned glibc sine/cosine radian evaluator from
`benilla-sim`; it does not reuse the VMap consumer's different degree conversion
or the float facing atan2 implementation. Cargo.lock pins the registry checksum.
The libm dependency retains its upstream MIT/Sun notices in the corresponding
vendored source. Existing feature unification may select architecture-specific
sqrt implementations; all supported platform jobs run the same exact-bit corpus.

Callers must explicitly select the policy ID. Unknown or native-bit-parity
requests fail. The capability is restricted to the four CI targets: little-endian Linux
x86-64/AArch64, Windows x86-64 and macOS AArch64, with dynamic binary32/binary64 round-to-nearest-even and
gradual-underflow checks at selection and each projection. Other environments
fail with `unsupported_numeric_profile`. This avoids a Linux-only native-libm
implementation while making the numeric contract explicit.

Every result binds the source epoch, CMaNGOS pin and numerical policy ID. The
policy-match operation rejects different declarations. Future preprocessing,
artifact exchange and peers must carry and compare this binding and policy;
there is no existing map-admission or network path enabled by these results.
A digest is not a source lease or authority declaration.

## Source arithmetic and serialization

Source pin: `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, `WMOInstance`,
`Doodad::ExtractSet`, G3D `Matrix3`, `Quat` and `g3dmath.h`.

- An emitting parent substitutes `533.33333f * 32` only when original x and z
  compare equal to zero, then permutes position to `[z,x,y]`. Destructible and
  no-collision early returns preserve the original position. Rotation is raw
- Parent degrees multiply by binary32 PI and divide by binary32 180. Matrix
  composition is Z(parent.y) * (Y(parent.x) * X(parent.z)), with unfused,
  left-associated binary32 arithmetic
- Quaternion normalization uses binary32 sum-of-squares, `1/sqrtf`, then component
  multiplication. Its matrix multiplies on the left of the parent matrix
- Euler XYZ conversion uses strict source comparisons, including the two gimbal
  branches, with no epsilon/clamp. Double inverse results narrow before float
  degree conversion. Output fields are `[Y,Z,X]`
- MODD scale is copied unchanged, including finite zero/negative values. It does
  not scale the local position. Later assembly must enforce its own profile
- Source name length is measured before `.mdx`/`.mdl` shortening. Thus the shorter
  `.m2` name includes one trailing NUL when serialized using the original length
- The 18-byte prefix contains map, tile, M2/worldspawn flags and zero nameSet.
  The suffix contains position, rotation, scale, original length and name bytes

All inputs/intermediates must be finite; parent angles are bounded to ±360
because that is the existing pinned sine/cosine domain. Zero/underflowed and
overflowed quaternion norms fail. Positive subnormal norms remain allowed if
all subsequent arithmetic is finite. Overflow norm rejection is deliberately
stricter than the source, which can normalize to zeros and produce identity.
No identity fallback, angle normalization or fabricated generated ID is applied.

## Evidence and remaining assembly work

The [independent authored oracle](../benilla-native-geometry-tests/tools/wmo_doodad_transform_README.md)
executes original WMOInstance, ExtractSet and G3D methods with isolated one-candidate
identities. It records exact input/output bits, intermediate matrices, floating
exceptions, name bytes and original source hashes. The Rust differential test
reports exact field differences, ULP difference and absolute geometric error for
its admitted corpus, separately counting explicit domain refusals. The current
Linux receipt is 72 admitted cases, seven explicit refusals, and zero differing
coordinate fields (zero ULP and absolute geometric error); six source cases emit
no child. This finite corpus is not a proof over every float input. Actual authored
MPQ tests exercise the complete capture-to-projection path and changed-input
rejection. All-platform jobs run these same tests; results remain pending until CI.

A complete map assembler still needs ordered placement closure, duplicate
resolution, transformed bounds, primitive slots and full BIH construction.
Source generated IDs affect `std::map` ordering, which affects BIH leaves and
strict equal-hit/area winners. Arbitrary sorting and ID reassignment therefore
cannot be called source-equivalent. A bijective relabeling is safe only while
preserving primitive order, BIH slots, duplicate classes and first-wins behavior.
The next integration must establish those observables with multi-instance source
queries before publishing complete map products. Selected tiles never imply
missing-map clear space. Held content compilation and production admission remain
outside this interface.

CMaNGOS licensing is in [CMANGOS-NOTICE.txt](CMANGOS-NOTICE.txt); the original G3D
Modified BSD grant is retained in [G3D-NOTICE.cpp.txt](G3D-NOTICE.cpp.txt).
