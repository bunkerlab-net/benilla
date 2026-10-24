# Current-state source spell range

`initialize_spell_authority` selects the private immutable range numeric policy
alongside its existing source spell catalog. There is no additional opt-in and no
public API that can turn an external request, observed reply or saved pose into
current-state authority. Unselected legacy `Query::Spell` behavior is unchanged.

The executing `Frame` already assembles `SourceSpellQuery::Range` from the
validated raw `SpellRangeSource`, player/NPC combat reaches, and both current
staged native XYZ poses. `Frame::ask` derives its answer through the
[portable binary32 mechanism](../benilla-sim/SPELL-RANGE.md), before any external
`Authority::resolve`. `Prepare` is strict; `ReleaseCheck` adds the source
non-strict tolerance. Other phases reject rather than borrowing either rule.
Delayed impact, release-hit and periodic aura execution do not run range checks.

The host preserves source readiness → LOS → target semantics → range → facing →
power order and source `TooFar` (0x59) / `TooClose` (0x76) failure results.
Both reaches inflate maximum range; strictly positive minimum range also gains
both reaches. Release adds ten percent of the inflated maximum, capped at 3.f.
Distance is 3D squared distance with each source binary32 operation preserved;
both endpoints are inclusive. Coincident coordinates do not make distinct
player/NPC objects self-targets, and minimum range is still checked.

## Domain and source side effects

The existing complete-source guards remain mandatory: active audited ID 133,
nonzero cast time, zero range flags, explicit ordinary enemy NPC target, no
controlled/summoned unit, ranged-slot attribute, next-melee-swing behavior,
spell-range modifier, unknown hooks, destination/source-only target or channel.
Only CANT_CRIT is an admitted attribute bit. Raw `words[7] == 0` excludes both
channel attribute flags. The target is an NPC and the range flags are zero, so
the source moving-unit leeway predicate cannot apply even when both units move.
Target/caster motion never freezes the request's positions at preparation.

Source `CheckRange` writes `m_maxRange` only on success. Its sole read at
`Spell.cpp:3645–3649` is the channel-distance check. The admitted non-channeled
projectile and periodic holder have no consumer for it; no invented persistent
spell field or unsupported channel lifecycle is added. The extracted source
oracle checks that failures preserve a sentinel and successes write effective
maximum range.

Finite `SpellFloat` row bounds (≤1e6), `Real` reach bounds (≤binary32 1e12), and
`Point` coordinate bounds (±32760) remain unchanged. Source arithmetic now keeps
subnormal intermediate values and gradual underflow, including squared-distance
and release-tolerance results; converting these intermediates through `Real`
would incorrectly reject defined source results. These admitted bounds preclude
overflow. Nonfinite/over-bound input is rejected explicitly and outside-domain
source observations are not claimed as runtime parity.

## Transactions and persistence

Derived range replies pass through `observe_derived` and enter the ordinary
ordered journal. The observer can reject the transaction, never replace the
answer. Replay rebuilds the current request and numeric result and checks exact
receipt equality, including phase, scheduler/history, witness ordinal, target
generation, profile/geometry binding, source row, reaches and both poses.
A mismatch or observer failure rolls back the entire staged frame, including
inputs, clocks, casts, scheduler, RNG and journal; a retry executes once.

The source spell policy is v5 and binds a private range-policy algorithm and
normalized `Spell.cpp` / `Object.cpp` hashes into the profile digest. Native
checkpoint/record format 51 (`benilla-native-home-wander-v51`) and v51 digest
domains refuse format 19 external-range history without migration. Character
profile saves and OriginalFixture format 2 are unchanged. `NativeCapability`
remains uninhabited, production startup remains closed, and no native producer,
content, Engine, entry or participant path is unlocked.

Runtime tests exercise source-derived failure precedence, phase isolation,
current moving-target poses, both-phase observer rollback, receipt corruption,
source-policy binding and incompatible history. The numeric corpus is generated
from pinned unchanged C++ bodies, with authored inputs only. Controller tests
separately exercise current player movement, boundary failures, durable
publication retry and reopen. None of these headless results establishes a
playable world or complete source spell coverage.
