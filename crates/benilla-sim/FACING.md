# Source-profile current-pose bearing and arc arithmetic

This is a bounded numeric service, not a map/visibility/LOS provider, complete
combat implementation, content producer or gameplay admission. Runtime selection
and receipt ownership are documented in
[the runtime facing contract](../benilla-world-runtime/FACING-AUTHORITY.md).
The corrected numeric policy changes persisted native profile identity.
`NATIVE_FORMAT_VERSION` and `NATIVE_POLICY` in the runtime reject format-90 saves
without migration.


## Source and licensing

CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/game/Entities/Object.cpp:1465–1473`: `WorldObject::GetAngleAt`
  subtracts XY coordinates in binary32, calls the `std::atan2(float,float)`
  overload, and adds binary32 TAU only if the result is negative
- `Object.cpp:1503–1527`: `HasInArcAt`/`HasInArc` preserve the self-identity
  shortcut, normalize arc and relative angle independently, fold angles greater
  than PI, and compare inclusive negative/positive half-arc borders
- `Object.cpp:1546–1559`: `IsFacingTargetsFront` first tests the target's
  arc, then the attacker's arc. Equal coordinates are not equal object identity
- `src/game/Maps/MapManager.h:121–132`: `NormalizeOrientation` uses float
  fmod. Positive whole turns produce zero, negative whole turns produce TAU
- `src/game/Entities/Unit.cpp:4428–4431`: `SetInFront` uses `GetAngle`;
  `Unit.cpp:11033–11075`: `UpdateSplinePosition` uses the old map pose for
  target-facing and the same float subtraction/atan2/TAU sequence for tangent
  fallback. Its explicit `<math.h>` include supplies the float overload for the
  unqualified call; the selected profile still pins the implementation
- `Unit.cpp:3059–3066`: player dodge requires **both** front arcs
- `src/game/Spells/Spell.cpp:6245–6248`: `CheckRange` facing asks the
  player's single `HasInArc(target, PI)` after distance checks

CMaNGOS code is GPL-2.0-or-later, Copyright CMaNGOS contributors. The Rust
translations, boundary and software-operation adapters are GPL-3.0-or-later,
2026 Benilla fork contributors. Existing [upstream notices](licenses/COPYRIGHT.md)
and [GPLv3 text](licenses/GPL-3.0.txt) apply. No game files or database rows are
included.

The selected implementation is GNU libc 2.36
[`e_atan2f.c`](https://github.com/bminor/glibc/blob/glibc-2.36/sysdeps/ieee754/flt-32/e_atan2f.c)
and [`s_atanf.c`](https://github.com/bminor/glibc/blob/glibc-2.36/sysdeps/ieee754/flt-32/s_atanf.c).
Both retain the Sun Microsystems 1993 permission notice in `src/facing/atan2.rs`.
The combined crate remains GPL-covered.

Full-file hashes, normalized only from CRLF to LF, are retained in
[the oracle manifest](tools/facing_source_hashes.sha256) and in
`facing::SOURCE_SHA256`. The algorithm/profile identity is
`cmangos-8ec338a1-facing-glibc-2.36-soft-rne-v2`.

## Numerical profile and boundary

The source names C++/libm operations, not a cross-platform bit identity. This
service explicitly chooses the pinned glibc implementation and emulates every
binary32/binary64 operation with the existing `rustc_apfloat` dependency.
Round-to-nearest/ties-to-even, gradual subnormals, and exact source expression
order are mandatory. No operation fuses. No host atan2, fmod, trig, FMA,
floating-point environment, fast-math, implicit contraction or platform libm is
used by the implementation. Floating-point status flags/errno are not gameplay
outputs and are not reproduced. Nonfinite inputs reject before evaluation.

The raw bit helpers retain signed zero and subnormal inputs/intermediates:

- `atan2_bits(y,x)` accepts every finite binary32 pair, including signed zero
- `bearing_bits(origin,toward)` accepts native XY coordinates inclusive ±32760
- `normalize_bits` and `has_in_arc_bits` accept finite input orientation/arc
  magnitudes up to `0x41c90fdb`, binary32 four times TAU. Internal relative
  angles may exceed that input bound and still use the exact source reducer
- `has_in_arc_bits` validates all inputs before its source self shortcut;
  its `same_object` parameter is identity supplied by the caller, never inferred
  from coordinates. Current runtime player/NPC calls always use false

Source `ang >= 0` retains negative zero. Source normalization also retains
negative zero. The existing runtime `Real` contract separately canonicalizes
stored zero and rejects subnormal outputs. A genuine tiny bearing which cannot
fit that storage contract fails the staged transaction; it is never clamped.
This raw/source versus runtime-storage distinction is intentional and tested.
Unsupported coordinates/angles reject, rather than extrapolating or silently
normalizing arbitrary input into the admitted domain.

## Evidence and limits

The independent oracle extracts unchanged CMaNGOS bodies from hash-checked
files and runs them in the pinned F0 GCC 12.2/glibc 2.36 builder image
`sha256:5397aff8205e0d96686d9a74bd0c9542830d170c167c8d242a3c13e9f269e9a8`.
It records the image and compile command with private evidence. It has no Rust
implementation dependency. Contraction and fast math are disabled, RNE is
required, and FTZ/DAZ must be off. Tests compare exact result bits and booleans.
The 3,351 curated signed-zero, subnormal, axis, ratio-reduction, PI/TAU and
inclusive-border cases retain eight formerly hard inputs and their neighbors.
Another 155,648 deterministic sampled inputs are checked through result digests.
The previous 2.41 corpus differed in 147 cases against this builder, including
131 atan2, two bearing and 14 arc cases. The correction retains the same inputs.
These checks are not exhaustive proof of every binary32 pair or sealed F1
reached-input acceptance.

Regenerate or verify from this crate with
`python3 tools/generate_facing_fixture.py /path/to/mangos-classic /path/to/glibc-float-sources --evidence-dir /main/benilla-config/path --check`.
The source directory must contain the hash-pinned `e_atan2f.c` and `s_atanf.c`.
Normal Rust tests require no checkout, Docker or network.
