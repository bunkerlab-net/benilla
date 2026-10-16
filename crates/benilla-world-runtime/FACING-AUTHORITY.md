# Current-pose facing authority

`Profile::initialize_facing_authority()` explicitly selects a private immutable
`FacingNumericPolicy` under the existing `InteractionCatalog`. Source motion and
interaction initialization are prerequisites. The selected policy serializes the
exact `benilla_sim::facing::POLICY` and its three normalized-source SHA-256 hashes
into the profile digest. Replacing interaction source rows preserves the selection
and revalidates it; the policy has no deserializer or public constructor.

This mechanism does not create `NativeCapability`, inspect raw database content,
open native startup, or establish playable Classic integration. The compiler
capability remains uninhabited. The tests use authored component fixtures.

## Exact request ownership and derivation

The private executing `Frame::ask` creates each bound request before selecting
these three numeric paths. There is no public resolver that treats an externally
supplied pose, receipt or state hash as a current runtime-state capability.

- `MotionQuery::Spatial(Bearing)`: source `WorldObject::GetAngle`, using exactly
  the request's origin and destination. `Unit::UpdateSplinePosition` deliberately
  supplies the **old map position** before committing its new spline position;
  substituting the new position would be incorrect
- `CombatQuery::MeleeFront`: `Unit::CanDodgeInCombat` calls
  `attacker.IsFacingTargetsFront(victim)`. Source evaluates the player victim's
  `HasInArc(pi, creature)` first, then the creature attacker's
  `HasInArc(pi, player)`, short-circuiting on a failed victim arc. The executing
  `WorkingNpc` supplies its actual post-movement pose even while removed from the
  staged scene. A preceding `SetInFront` is not a substitute for either check
- `SourceSpellQuery::Facing`: the caster player's single `HasInArc(pi, target)`
  at the actual prepare or release CheckRange phase. Release recomputes current
  orientation; an earlier successful prepare does not authorize it

Player and creature identities are distinct even at coincident coordinates, so
all current arc calls pass `same_object=false`. No coordinate equality shortcut
stands in for source object identity. Combat and spell queries retain their
existing complete frame/working-object/history/scheduler witnesses and order.

Derived replies pass through `Authority::observe_derived` and enter the ordinary
ordered receipt stream. The observer may verify or reject, but cannot replace
facts. Replay recomputes the three numeric answers from executing requests and
compares complete replies, so modifying a recorded numeric receipt is rejected.
An error anywhere in the tick rolls back staged state, input sequence, clocks,
RNG and history through the existing whole-frame transaction.

Facing-unselected profiles preserve their external facing behavior. Independent
source spell authority derives [range](SPELL-RANGE.md). LOS, visibility,
accessibility, visitors, NearPoint, routes, colliders and FacingTarget object
lookup retain their existing paths and gain no implicit implementation or default
answer from facing selection.

## Numeric domain and provenance

See [the software numeric mechanism](../benilla-sim/FACING.md) for source hashes,
algorithm, expression boundaries and oracle coverage. Raw input coordinates are
finite binary32 with absolute magnitude at most 32760; orientation and arc are
finite with absolute magnitude at most four binary32 turns. The computation uses
software round-to-nearest-even and the pinned glibc 2.41 CORE-MATH atan2f path,
not host platform trigonometry or a tolerance approximation.

Native `Real` retains its stricter storage domain. Bearing results pass through
`Real::from_bits`, preserving its signed-zero canonicalization and rejecting a
subnormal result without rounding or clamping. Raw numeric support does not
silently enlarge the runtime's coordinate/state domain.

Source mechanisms: CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, `Object.cpp` GetAngle/HasInArc/
IsFacingTargetsFront, `MapManager.h` NormalizeOrientation, `Unit.cpp` melee dodge
and spline update callsites, and `Spell.cpp` CheckRange. CMaNGOS is
GPL-2.0-or-later, copyright CMaNGOS contributors. The pinned CORE-MATH-derived
`e_atan2f.c` has its MIT notice retained in the simulation oracle material.
CMaNGOS-derived Rust and this runtime integration are GPL-3.0-or-later. No game assets or
third-party database rows are included.

## Format boundary

The experimental native simulation checkpoint/record contract is format 28,
`benilla-native-surface-player-environment-v28`, with v28 profile/state/record/checkpoint/
melee-action/melee-chain hash domains. Published native format 11 is rejected by
decode, restore and replay, with no reinterpretation or implicit migration.
Incompatible store current and previous files remain intact on refusal.
OriginalFixture format 2, store envelope format 1, and the separate character and
profile save formats are unchanged.
