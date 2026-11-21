# Ordinary creature health regeneration

Policy `cmangos-8ec338a1-ordinary-creature-health-v1` ports the health portion of
ordinary `Creature::RegenerateAll` into a deterministic component. The native
runtime uses it after each NPC's movement, AI and CombatManager step, persists
its timer, journals actual health changes, and projects committed health to
stock unit fields. This is synthetic component integration, not admission of a
playable database or proof of a live-client run. `NativeCapability` stays closed.

## Pinned source

CMaNGOS Project contributors, Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, GPL-2.0-or-later. This translation and
fixture host are GPL-3.0-or-later; [provenance](PROVENANCE.md) and
[upstream notices](licenses/AUTHORS.md) apply.

- [Unit.cpp:324](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L324)
  initializes `m_regenTimer` to zero. There is no ordinary-creature assignment
  resetting it on combat, evade, home, death, entry reset or respawn
- [Creature.cpp:706-798](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp#L706)
  invokes regeneration only for a frame entering ALIVE that remains alive
  after Unit::Update. CORPSE/DEAD frames and the DEAD-to-ALIVE respawn frame do
  not advance it. The same Creature object's timer survives those transitions
- [Unit.cpp:Update](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L453)
  runs spell/aura updates, movement, AI and CombatManager before creature regen.
  The runtime samples combat and `is_evade_regeneration` after the AI component;
  a newly acquired target blocks an otherwise-due pulse, while the final five
  seconds of unreachable evade permit healing even with combat still active
- [Unit.h:1006](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.h#L1006)
  defines the NPC interval as 5,000 ms. Unsigned addition wraps in the standalone
  kernel; one update performs at most one pulse and subtracts the interval once,
  including full health, disabled health or combat-blocked health. The native
  runtime's fixed 1..1,000 ms step and zero genesis timer keep its accumulator
  strictly below 5,000 ms; persistence validates that bound and timer grid
- [Creature.h:824](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.h#L824)
  reads the current template: health flag 0x2 must be present and type flag
  ALLOW_INTERACTION_WHILE_IN_COMBAT (0x40000) absent. Placeholder health-in-combat
  flag 0x1 does not authorize in-combat healing. Required immutable policy fields
  preserve these raw source bits; this runtime does not implement entry, flag,
  player-control or regeneration-override mutations
- [Creature.cpp:863-878](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp#L863)
  uses integer `maxHealth/3` for ordinary uncontrolled creatures. The controlled
  branch uses unsuffixed `0.13` (binary64), then truncates to uint32; that branch
  is explicitly rejected here, as are pets and unknown/overridden creature kind
- [Unit.cpp:8062](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L8062)
  accepts an int32 delta, converts current/max health to int32, adds and clamps
  to maximum. The one-million health bound keeps these conversions and addition
  representable. No minimum-one regeneration is invented: max health 1 or 2
  yields zero healing

## Positive evidence and unsupported power

`Policy` has no default and serde requires every field: ordinary/nonpet kind,
actual RegenerateStats, actual creature TypeFlags, effective UNIT_FIELD_FLAGS,
and known active power kind/maximum. Unknown kind/power and player-control flag
0x8 are errors. These typed values are inputs, not an authenticity certificate;
a future private compiler must bind them to captured rows and effective state.
No missing mana field is interpreted as zero.

`RegeneratePower` is not implemented by this slice. It is provably irrelevant
only when the captured active maximum is zero, both combat/out-of-combat power
flags are disabled, or the captured kind is rage/happiness, which the source
switch returns from before modifier processing. Nonzero mana/focus/energy with
either power flag enabled fails explicitly, even if currently full. Max-power
changes, controlled/pet overrides, NPC power spending and other NPC power
mutations require another implementation. A known ordinary mana-less creature
is accepted even when its template enables power regeneration.

## Verification

`tools/generate_creature_regeneration_fixture.py UPSTREAM_ROOT --check` verifies
SHA-256 hashes of five pinned source files, extracts RegenerateAll,
RegenerateHealth, ModifyHealth, health/control predicates and constants, compiles
with undefined-behavior/float-conversion sanitizers, and compares 13,440 authored
cases. The harness counts power calls while modeling only captured zero-maximum
power; it does not claim power arithmetic parity. It also checks source
constructor and update ordering. Fixture SHA-256:
`4831a9886fcf59f1513ca39e9de772467c7e34b9a2e88b30459103dcb04eb04b`.

Rust tests cover the oracle, single-pulse backlog and wrap, unsupported policy,
zero/full health, disabled flags, combat/evade gates, persisted boundary resume,
replay, atomic failure rollback, death/corpse/respawn timer preservation and
committed client unit-health projection. They use authored values only; no game
assets or actual database rows are bundled.

## Next coverage: remaining RegenerateAll branches

Full Classic remains the target; these are implementation dependencies, not
permanent feature exclusions. This health hook does not complete RegenerateAll.
The next power slice must port Creature.cpp:800-861 and IsRegeneratingPower at
1814-1819 with actual active power/current/maximum, combat state, master GUID,
pet/player-control state, last-mana-use timer, effective intellect, the
OCTRegenMPPerSpirit DBC dependency, world mana/energy/focus rates, and complete
ordered flat/percent power-regeneration aura modifiers. Source arithmetic:

- Power flags select 0x4 in combat, 0x8 otherwise; full power returns immediately
- Mana in combat or with a master, outside the last-use effect, computes
  sqrt(intellect) * OCTRegenMPPerSpirit() * manaRate / 5.f * timerMultiplier.
  Nonpet, non-player-controlled zero results fall back to integer maxMana/20,
  then /5.f * timerMultiplier. Out-of-combat masterless mana uses maxPower/3.0f
- Energy uses 20 * energyRate; focus uses 24 * focusRate. Other kinds return
  before aura modifiers. Ordinary timerMultiplier is 5, from 5000/1000
- Matching flat aura amounts add amount/5.f * timerMultiplier in source order;
  matching percentage auras multiply by (amount+100)/100.0f in source order.
  The result truncates to int32 and ModifyPower clamps it. A deterministic port
  must preserve binary32 operation/conversion order, not rationalize the formula

Controlled ordinary-creature health needs the binary64 maxHealth*0.13 conversion,
actual effective player-control flags and supported control/entry transitions.
Pet regeneration is a separate virtual override (Pet.cpp, 4,000-ms health timer)
and must be ported with its own source evidence and dependencies. Dynamic entry,
flag, active-power/maxima changes and relevant stat/aura updates need source-
ordered mutable state rather than assuming the initial policy remains current.

## Timed-evade damage ordering

The original DOT-to-blocked-chase reproducer is retained in runtime tests. The
unreachable timer starts at ten seconds: all of it is damage-evading, but only
its final five seconds permit health regeneration. This distinction comes from
[CombatManager.h:43-46](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Combat/CombatManager.h#L43).

School immunity is checked first. For a due nonimmune tick,
[SpellAuras.cpp:4571-4593](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellAuras.cpp#L4571)
calls DealDamageMods after absorb/resist calculation.
[Unit.cpp:865-889](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L865)
adds the supplied damage to existing absorb, zeros damage, and returns before
AI DamageDeal/DamageTaken callbacks. PeriodicTick then applies its signed-resist
bonus/malus, emits the periodic log, calls DealDamage and finally invokes procs.
The arithmetic helper preserves that exact accumulation/order; it does not
replace existing absorb or resolve shields/resistance itself.

The runtime's existing no-resistance/no-shield domain passes zero prior absorb
and resist, resulting in amount=0, effective=0, absorb=the periodic amount. Aura
clock, ticks and normal expiration continue; it is not a school-immune result.
The original short-DOT regression and a longer DOT exercise checkpoint resume,
replay and client zero-damage/full-absorb logs during timed evade.

[Unit.cpp:959-968](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L959)
returns zero damage before loot ownership, health and threat handling. Earlier
interrupt-aura/channel callbacks and later ProcDamageAndSpell still exist in
source and remain excluded by the bounded spell domain; this change does not
invent support for them. Consequently this admitted zero-damage tick creates
no damage/threat receipt. On actual EnterEvadeMode,
[UnitAI.cpp:113-129](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/BaseAI/UnitAI.cpp#L113)
removes eligible auras before combat stop/home movement. The existing ordinary
AuraRemoved events remain at that transition, not at timer start.

`generate_periodic_evade_fixture.py UPSTREAM_ROOT --check` extracts the unchanged
DealDamageMods method and contiguous periodic mitigation block. Its 120 authored
cases cover zero/nonzero prior absorb, positive/negative/zero resistance and
AI callback order/suppression. SHA-256:
`2c2709d20eac67a43eb1b936994abb7110f89ed4f9f55adc4de9c8e45bb4ec55`.
