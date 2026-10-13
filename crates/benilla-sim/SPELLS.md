# Bounded mage projectile and periodic-damage execution

`classic_spell` is a GPL-covered, stateful port of selected CMaNGOS Classic
branches at `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It advances a timed
cast, spends mana, records cooldowns, snapshots launch damage, flies, applies
an aura, deals direct damage, and advances periodic damage against explicit
mutable state. These are executable components, not a replacement spell table.

**This module does not admit a native world, certify a spell ID, or establish a
playable Human Mage starter. Existing native capability gates remain closed.**
No Blizzard files, database rows or learned spell IDs are bundled. The numeric
rows and spell 900001 in tests are original synthetic probes.

## Accepted component domain

- A non-triggered, non-item, timed player cast at one ordinary hostile NPC
- Mage-family magic school damage, plus at most one ordinary periodic-damage
  aura, retaining the three source effect slots
- Positive projectile speed; caster and target are different units
- No spell/aura/gear/script modifiers, combo points, spell power, rank overrides,
  aura-stack modifiers, immunity/reflection charges, absorption, death prevention,
  PvP/duel/pet/GM behavior, proc callbacks, triggered spells, leech or threat mods
- No school resistance or penetration, with caster and NPC levels 1 through 20
  and ordinary level-times-five skills. The upstream resistance calculation's
  100-skill floor makes the additional NPC level-resistance term zero here
- Unit cast speed, zero session latency, no GCD or cooldown modifiers;
  ordinary cooldown categories without `CooldownIsGlobal`
- Caller has already resolved cast time, cost, spell rank, duration, checks,
  direct/periodic polarity and target selection from the pinned server source
  and the user's own client data. Missing evidence must not become a default

`MageProjectileSpec::validate` validates the **typed component domain**. It is
not a raw Spell.dbc validator. It rejects unsupported effect/aura/trigger variants,
multiple direct or periodic effects, missing direct damage, instant casts at
prepare, invalid times and numeric encodings. The raw-native compiler must still
prove that nothing relevant was lost in conversion to this type. Simply setting
`CastChecks::other_failure` to `None` is not that proof.

The no-modifier domain is a precondition of these low-level functions. In
particular, a caller must not pass a resistant NPC to `DamageTarget` and then
interpret the zero-resistance calculation as a full combat implementation.

## Stateful source mapping

- `Spell::SpellStart`, `PreCastCheck`, `Prepare`: reject failed preparation before
  mana/cooldown side effects; GCD begins on successful prepare; record cast timer
- `Spell::update`: skip the creation map tick once; movement and target loss/death
  cancel preparation; subtract map diff, casting when the timer reaches zero
- `Spell::Delayed`, `Spell::GetNextDelayAtDamageMsTime`: explicit damage pushback
  of 1000, 800, 600, 400, then 200 ms, capped by the original cast time
- `Spell::cast`, `FillTargetMap`, `AddUnitTarget`: recheck at release, resolve the
  magic hit die, compute projectile delay, then commit cooldowns and mana
- `Spell::TakePower`: record last mana-use time only for positive mana expenditure
  in the branch that blocks normal mana regeneration. Mana regeneration itself
  is not implemented here
- `Spell::HandleImmediateEffectExecution` and
  `IsEffectHandledImmediatelySpellLaunch`: school-damage value and critical
  outcome are snapshotted on launch; ordinary aura effects wait for impact
- `Spell::DoSpellHitOnUnit`, `EffectSchoolDMG`, `EffectApplyAura`,
  `DoAllEffectOnTarget`: attach periodic aura before accumulated direct damage;
  apply the magic critical multiplier at impact; lethal damage removes the aura
- `Aura::HandlePeriodicDamage`: the no-modifier damage-done amount is snapshotted
  on application. `Aura::PeriodicTick` clamps nonpositive damage, checks immunity
  every tick, does not roll crit, and applies damage against the current target
- `Aura::Update`, `SpellAuraHolder::Update`, `Unit::_UpdateSpells`: skip the
  creation map tick once; tick before reducing duration; tick at timer zero;
  perform **one** tick per update even after a large diff, carrying negative timer
  debt rather than inventing a catch-up loop; remove the expired holder
- `Unit::AddSpellAuraHolder`: same spell/caster, nonstacking ordinary aura replaces
  the holder and restarts its timer. Cross-caster, cross-rank and special rolling
  auras are refused rather than treated as ordinary refreshes
- `Spell::cancel` clears the preparation GCD. `StopCast` release-check failure does
  not. Cancellation after launch never refunds mana or clears a later cast's GCD

Events are ordered component outputs. They are not CMaNGOS packets and do not
perform combat flags, threat/AI reactions, damage callbacks, XP, loot, quest credit,
combat logs, mana regeneration, unit death scripts or world persistence. The
caller must schedule multiple in-flight spells independently of its current
casting slot. A newer cast can begin while an older projectile is traveling.

`CastChecks` carries the independent strict/non-strict source CheckCast result.
This module does not reproduce the precedence among all unported CheckCast
errors. At impact, `allowed`/`immune` are resolved results of the source's
visibility, relationship and immunity rechecks. They must come from the world,
not unconditional success. The component supports the common outcomes; it does
not implement charged immunity, reflection, school switching or partial effects.

All malformed numeric/roll input errors in update/impact/aura-update are
transactional. Unknown aura collisions at impact also leave those objects
unchanged; the caller must reject the unsupported world transaction rather than
keep advancing or fabricate a result. A future native adapter must preflight
its world domain and preserve the enclosing committed state on such errors.

## Numeric and RNG contract

Include `SPELL_EXECUTION_POLICY_ID` in any future world/rules compatibility hash.
All runtime floating calculations use the already pinned `rustc_apfloat` library,
with integer software operations. No host float, libm, fused multiply-add,
reassociation or fast-math authority is used.

- `SpellFloat`: signed normal binary32 or signed zero, absolute value at most
  1,000,000. Reject subnormals, NaN and infinities
- General numeric helpers accept spell rank 1..315; the runtime component accepts
  1..100. `GetSpellRank / 5` is not replaced by an assumed character level
- Effect input/result magnitudes and health/mana: at most 1,000,000. Time values:
  at most 3,600,000 ms. Timestamps are checked u64 simulation milliseconds
- `CalculateSpellEffectValue`: preserve float operation order, level clamps and
  the source's if/else-if; dice scaling runs only when points-per-level is nonzero;
  final return converts toward zero. Per-operation rounding is nearest-even
- `CalculatePowerCost`: flat + per-level + integer percentage of **create** mana;
  preserve its final float conversion in the no-modifier branch and clamp at zero
- `AddUnitTarget`: source binary32 square root, clamp distance to five yards,
  divide by speed, multiply by 1000, floor. Input is the already computed raw
  squared distance, not center distance minus combat reach. The software square
  root uses adjacent binary32 values and an exact software-binary64 midpoint
  square (at most 50 significant bits), rounding nearest-even
- `chance_u`: binary32 multiplication by 100 followed by round-half-away-from-zero
- Hit/resist die: explicit integer in 1..=10000, resisted when die <= threshold
- Critical die: explicit integer in 1..=10000, critical when die < threshold.
  This strict source comparison means a 10000 threshold does not crit on 10000
- Effect die: exact inclusive source irand range, including reversed negative
  ranges; die sides 0 or 1 consume no effect die and add base dice directly
- Pushback: explicit irand draw in 0..=99, even with zero resistance chance;
  in this no-modifier branch the result never resists
- Magic crit: convert damage to binary32, multiply by 1.5, truncate to uint32
- Zero-resistance periodic damage consumes no new hit, crit or resistance die

There is deliberately no hidden RNG generator. Replay must preserve exact
ordered semantic draws and the canonical map-update diff sequence: launch hit,
launch direct-effect die when called, launch crit die only for positive damage
and nonzero threshold, impact aura-effect die when called, and pushback die on
each eligible damage event. Extra/missing/out-of-range draws fail. An outer P2P
RNG adapter must specify its generator and unbiased mapping, then hash/version
that policy; this module does not claim its fixtures reproduce CMaNGOS's global
PRNG stream. Never substitute modulo reduction or consume a dummy crit die when
the source branch would not call it.

## Native dependencies still required

For the proposed Human Mage starter, this port supplies the Fireball-shaped
cast/direct/periodic mechanism without claiming a real Fireball row was loaded.
Frost Armor's armor modifier, reactive chill, aura lifetime/stacking and callbacks
remain unsupported. Supporting an effect shape does not authorize dropping other
starter spells or passive/racial effects.

A production compiler/adapter still needs:

1. Actual initial learned-spell and character rows, pinned spell_template/server
   overrides, all Spell.dbc fields, SpellCastTimes, SpellRange, SpellDuration and
   SpellCategory data and content identity. Preserve every effect/flag rather
   than projecting a row into the supported shape and discarding the remainder
2. Exhaustive raw attribute, target, prevention, interrupt, aura-interrupt,
   stance, reagent/tool/equipment, mechanic, family flag, rank, area/location,
   DB spell-affect/threat/proc/script and server-setting dependency validation
3. Full strict/release CheckCast wiring: caster status and generic cast slot,
   selected target identity, target eligibility, line of sight, exact ranges and
   facing, movement/visibility and target changes. Existing relationship helpers
   alone do not implement these gates
4. Resolved real mana, rank, cast speed, base miss and caster spell-crit chance,
   plus proof there are no excluded damage/cost/crit/periodic modifiers. No
   user-specific stat, spell or creature values are synthesized by this module
5. Nonzero-resistance/full-resist/partial-resist distribution and absorption,
   charged immunities/reflection, modifier coefficients and snapshot rules outside
   the no-modifier branch; other spell/aura effects and procs
6. World runtime scheduling, native unit initialization, combat/threat/AI/death
   callbacks, rewards, deterministic RNG adapter, canonical serialization/hash,
   checkpoint/replay and existing client event/visual/UI integration

Until those checks and the selected content's additional semantics are present,
**native entry must continue refusing**. This module is not wired to the existing
fixed-damage synthetic encounter and does not relabel it as Classic gameplay.

## Verification

`tools/generate_spell_fixture.py` SHA-256 checks pinned source and compiles exact
function bodies or explicitly named contiguous branch blocks in the original
C++ adapter, using nearest-even FP, no fast math/contraction, and undefined/
float-cast-overflow sanitizers. It emits 8,723 original numeric/timer records:
effect scaling, travel/sqrt boundaries, mana cost, cast time, magic miss levels,
crit rounding/chance boundaries, cast timer/dead target branches and periodic
timer debt. No source download or game data is required during Cargo tests.

`tests/classic_spell.rs` compares those records plus end-to-end component state
sequences for cast/cost/cooldown/travel/direct/periodic damage, cancellation,
release recheck, pushback, immunity, death, aura replacement, unknown effects,
roll misuse, cross-caster refusal and atomic errors. These are bounded component
proofs, not whole-server trace parity or a live player/client acceptance test.
