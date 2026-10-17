# Empty-list near-point selector arithmetic

`near_point::selector::EmptySelector` reproduces the fresh empty-list selector
used by pinned CMaNGOS Classic `WorldObject::GetNearPointAt`. That caller comments
out its only used-position visitor. This is a source quirk, not evidence that
dynamic colliders or nearby objects are absent.

The source is CMaNGOS commit `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`src/game/Maps/ObjectPosSelector.cpp` and `.h`, with constants from
`src/game/Entities/ObjectDefines.h` and `src/shared/Common.h`. Full-file hashes
are exported in [selector.rs](src/near_point/selector.rs). The translation retains
the CMaNGOS contributors' GPL-2.0-or-later provenance within this GPL-covered crate.

## Explicit numeric profile

The original selector translation unit, with its genuine headers and with and
without the pinned full game PCH header, resolves global `atan` to binary64 under
the checked GCC 14/libstdc++ 14 and Debian Boost 1.83 profile. This observation
does not prove every platform's overload or libm output. The selected
[software atan profile](SOURCE-ATAN.md) fixes the numerical implementation rather
than invoking the host library.

The constructor repairs either signed zero size or distance to source
`DEFAULT_WORLD_OBJECT_SIZE`. It separately rounds `1.8f * size` and division by
distance in binary32, widens the ratio exactly, evaluates pinned binary64 atan,
and narrows its result to binary32. Finite nonnegative constructor inputs include
values whose ratio overflows to positive infinity or underflows to zero.
Nonfinite and negative nonzero constructor inputs are outside this profile.

Initialization performs binary32 `0 - half_angle`. Each side then advances by
binary64 `double(step) + (double(half_angle) + 0.01)` before narrowing to float.
The source literal is double, not `0.01f`. Plus wins ties; each offset is yielded
in source order. An offset exactly equal to binary32 PI is permitted. Empty
used-area iteration immediately exhausts. At most 314 offsets per side, 628 total,
are yielded under this domain; exhaustion is stable.

`candidate_angle_bits` separately performs the source binary32 base-plus-offset
addition, without modulo normalization. It accepts base `[0, TAU]` and signed
offset `[-PI, PI]`. The separately named fallback candidate/trig path accepts the
resulting raw interval `[-PI, round32(TAU+PI)]`. Existing initial-candidate and
VMap trig domains are unchanged.

## Boundaries

These functions supply numerical candidates only. They do not query ground,
liquids or LOS, accept a destination, construct a movement path, or admit native
world startup. A complete empty-selector provider must preserve sequential source
height queries, reset ordinary candidate Z to target Z, and after full exhaustion
restore first XY while carrying the last adjusted Z into one final height query.
That source final return does not perform another vertical or LOS test. Missing
coverage or exhausted work/deadline budgets cannot masquerade as source exhaustion.

The independent C oracle uses original pinned selector bodies with real empty
maps and pinned atan, rather than Rust-generated expected answers. Tests compare
raw ratios, half angles, every yielded offset and terminal states, with additional
input-domain and raw final-angle boundary checks. This does not establish full
`OwnerPathFinder` or populated-world behavior.
