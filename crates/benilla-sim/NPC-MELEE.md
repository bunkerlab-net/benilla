# Ordinary NPC mainhand melee arithmetic

This GPL-3.0-or-later module translates a **bounded** CMaNGOS Classic path at
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It provides real chance, damage,
health-projection and starter defense-gain calculations. It is not native content
admission, a complete `DealMeleeDamage` implementation, or proof of graphical play.
Nothing here loads or bundles game assets, database rows or character defaults.

## API and boundary

`prepare(MeleeInput)` validates numeric/state inputs before any random request,
returning a private-field `MeleePlan`. Request the plan's inclusive damage range,
then an inclusive `1..=10000` outcome die, even when the damage range has one value.
`resolve(plan, MeleeRolls)` is pure: invalid rolls cannot change health, skills,
RNG, combat state or callback state. It returns rolled, armored, total and clean
damage; health before/after; effective damage; overkill; outcome; required callback
records. `MeleeResolution` has private fields with read-only same-named getters, and
cannot be cloned or copied. `apply_health` consumes that single-use result and
checks the expected live health and projection before changing **only** health.
A runtime must stage all needed callbacks together with its RNG journal before
committing; health alone is not a completed melee action. Bind the prepared action to the
owner, entity/generation, tick, current authority, geometry/world revision and
request digest, and reject duplicate action IDs at the runtime boundary. Equal
health does not prove the rest of the world or callback state is unchanged.
Re-resolving an immutable plan remains pure and valid, so the component's
single-use result cannot substitute for runtime replay checks.

The named input types assert the caller has already audited this domain:

- Alive, uncontrolled, non-pet, non-totem, non-worldboss ordinary creature,
  one physical mainhand damage line, no offhand/ranged/extra/on-next-swing attack
- Player-controlled mage, normal ground combat, not taxi/evade/crowd-controlled,
  no duel, no death prevention, no immunities, absorb, resist, split damage,
  damage shield, aura/item/proc/script modifiers or AI damage overrides
- `CanDodge=true`, `CanParry=false`, `CanBlock=false`: Unit and Player constructor
  values with no later enabling/disabling spell effect; other classes and learned
  parry/block effects do not fit this type
- Resolved current state and fields, not guessed values or raw template defaults

These semantic assertions cannot be established by arithmetic validation. A
native adapter must reject missing dependencies and unhandled native fields
before constructing these types. This module does not open the native compiler's
admission gate. `front_facing=false` is explicitly unsupported pending rear daze.
Both arcs are necessary: `Object::IsFacingTargetsFront` requires the target to
face the attacker **and** the attacker to face the target. A traveling generic
spell does not set `casting_non_melee`; use `IsNonMeleeSpellCasted(false)` semantics.
The explicit `has_channeled_spell` and `has_auto_repeat_spell` fields must be
read from actual current-slot presence; `prepare` rejects either, including
triggered/finished channels. Do not infer their absence from `casting_non_melee`
or supply false defaults: the source casting predicate can exclude a channel
that still requires a damage response. Channel/auto-repeat responses require
separate downstream adapters before this admission can be broadened.

Accepted numeric domain: creature level 1–63, mage level 1–60, live health 1–1,000,000,
effective agility 0–1,000,000, signed armor magnitude at most 1,000,000, current
defense as uint16. Damage/stat float inputs use `SpellFloat`: bounded finite normal
binary32 or signed zero, magnitude at most 1,000,000; damage dependencies must be
nonnegative. Conversion exceptions, out-of-domain values and unavailable branches
return errors rather than clipping or host-cast saturation.

## Exact input derivations

### Attack damage

`minimum_damage` and `maximum_damage` are the **resolved**
`UNIT_FIELD_MINDAMAGE`/`UNIT_FIELD_MAXDAMAGE`, not `creature_template.MinMeleeDmg`.
`CalculateDamage` swaps reversed floats, replaces maximum zero with 5.0, then
truncates both float endpoints to uint32 and performs inclusive `urand`.

The optional `creature_damage_range(CreatureDamageInputs)` ports the no-modifier
`Creature::UpdateAttackPowerAndDamage(false)` and `UpdateDamagePhysical(BASE_ATTACK)`
path. It needs exact effective strength/agility, floating create strength/agility,
post-SelectLevel `m_baseAP`, `m_damageMultiplier`, base weapon min/max, unit class
and weapon usability. Source AP stat contribution is effective strength for mage;
strength-minus-create plus agility-minus-create for rogue; twice strength-minus-
create for warrior/paladin. Add `m_baseAP * m_damageMultiplier` in binary32, truncate
through the source AP integer field, then divide the integer-field value by 14.0f.
Weapon endpoints separately multiply by `m_damageMultiplier`. Add the AP term;
the no-modifier source UnitMods are neutral. An existing unusable weapon contributes
the source conditional 0.5f multiplier. No attack-speed factor belongs here.

Obtaining those dependencies remains the native compiler's responsibility:

- Select a real creature level; range selection has its own source RNG draw
- Select the exact class/level row and template branch; preserve all relevant
  template fields and rank/server configuration
- `damageMulti = template.DamageMultiplier * _GetDamageMod(rank)`
- Current class-stat branch requires nonnegative template DamageMultiplier and
  ArmorMultiplier and a matching class/level row: base endpoints are
  `BaseDamage -/+ BaseDamage * (DamageVariance / 2)`; `m_baseAP` is
  `BaseMeleeAttackPower`; `m_damageMultiplier` is `damageMulti`
- The source legacy branch initially multiplies raw MinMeleeDmg/MaxMeleeDmg by
  `damageMulti`, and `UpdateDamagePhysical` multiplies the base weapon range by
  it again. This surprising behavior must not be algebraically "corrected"
- Create stats come from the selected source multipliers and current stat system;
  float create stats and truncated effective integer stats are not interchangeable
- Source modifier slots, weapon school/line count, dynamic DPS modifiers,
  usability and AP bonuses must be resolved or rejected, not silently neutralized

No SelectLevel branch, model selection, rank rate, source database row or omitted
configuration is invented by this helper.

### Chance table

Ordinary NPC weapon skill and skill maximum are selected level * 5. Worldboss or
other target-relative level adjustments are excluded: the armor path uses actual
`GetLevel`, while weapon/crit paths use `GetLevelForTarget`.

Mage defense is live `GetSkillValue(SKILL_DEFENSE)`, with permanent/temporary
bonuses; it is not maximum skill or automatically level * 5. Existing
`mage_resource::CurrentSkills` implements that source getter. Pure current/max
skill are separate inputs to skill growth.

- Base miss is 5%; effective miss adds `(current defense - attacker skill)*0.04f`,
  clamps to 100 and floors at 1 only when the difference exceeds 10, otherwise 0
- Base dodge uses `3.25f + agility / classrate`, with source operation ordering:
  `classrate = 12.9f*(60-level)/59 + 20.0f*(level-1)/59`. Effective dodge adds the
  defense difference *0.04f, clamps 0–100, and is unavailable during a non-melee
  cast. The UI dodge percentage already includes a defense adjustment and must
  never be reused as the base chance
- Mage parry/block are zero because their eligibility is absent. The shared
  `player_avoidance_chance` helper implements the source player arithmetic for
  dodge/parry/block but does **not** grant parry/block eligibility. Parry also
  requires a usable melee weapon; block requires a usable unbroken shield and
  physical school; both require facing, reaction and non-casting predicates
- NPC base crit is 5%; add `(attacker maximum skill - current defense)*0.04f`,
  then clamp 0–100. A sitting player instead has a 10000 crit bucket after miss
- Crushing requires ordinary NPC control semantics, no `NO_CRUSHING_BLOWS` flag,
  and the target's player-controlled flag. Deficit is attacker skill minus
  `min(current defense, player level*5)`. Deficit >=15 yields `2*deficit-15`%,
  capped at 100. No glancing chance exists for this attacker/defender pairing

Source `chance_u` multiplies in binary32 and rounds ties away from zero to basis
points. One die uses cumulative **miss, dodge, parry, block, glance, crit, crush**
order and an inclusive <= comparison. Probabilities need not sum to 100%; earlier
outcomes can push later outcomes off the table. No separate crit die is consumed.

## Damage and callback order

The physical armor calculation runs before the outcome roll, including its
one-damage minimum for an incoming zero roll. Neutral done/taken bonus arithmetic
is exact for the bounded integer range. Miss sets total/clean damage to zero.
Dodge zeroes total and doubles armored `cleanDamage` in this pinned source.
Crit applies source float 2.0 then 1.0 conversion steps and resets clean damage.
Crush adds integer damage/2 while leaving clean damage at the armored amount.

`CalcDamageInfo`/`DealMeleeDamage`/`DealDamage` must remain distinguished:

1. `AttackerStateUpdate` performs CalculateMeleeDamage, then `DealDamageMods`.
   Our ordinary AI domain has no damage-editing callbacks. Send the combat-log
   projection before the source DealMeleeDamage side effects
2. Emit critical wound emote when appropriate. Mage cannot parry, so the source
   parry-haste branch cannot execute
3. DealDamage stands a sitting player even on miss/dodge. Zero damage returns
   without health loss. Positive damage either calls Kill or HandleDamageDealt
4. For a surviving positive hit, update health; call appropriate combat/timer
   transitions if their predicates allow it. The player's AddThreat call is a
   no-op because a player-controlled player cannot own a threat list; do not add
   damage threat to the NPC's threat list just because it damaged the player
5. The surviving hit always consumes the configured durability `rand_chance`
   draw, **including a zero configured rate**. On success draw equipment slot
   `urand(0,EQUIPMENT_SLOT_END-1)` and apply source item durability behavior.
   Missing durability/configuration support must be surfaced by the adapter
6. HandleDamageDealt then calls InterruptOrDelaySpell on every surviving positive
   hit, even when the casting predicate is false. Inspect the current generic
   slot; an absent, finished or traveling generic spell has no pushback draw.
   Apply the existing source mage cast-pushback/cancel component to the current
   casting slot, not projectiles
   already traveling. Any resistance roll comes after durability and before skill
7. DealMeleeDamage calls AttackedBy/EngageInCombatWithAggressor even after a miss,
   conditioned on both CanEnterCombat predicates evaluated at that time. These
   predicates do not themselves include IsAlive; retain the post-Kill evaluation
8. Rear positive hits require the separate `CanDazeInCombat`/probability/1604 aura
   path. This module rejects all non-front geometry rather than dropping it
9. After DealMeleeDamage, ProcDamageAndSpell invokes skills/reactives only on a
   surviving player. Normal/critical/miss/dodge all qualify against non-player,
   non-critter attackers. A lethal hit consumes no victim skill-gain roll
10. Mage dodge sets AURA_STATE_DEFENSE and starts REACTIVE_DEFENSE for 4000ms.
    This state is a separate required callback even if no current mage spell uses it

Kill requires death state/corpse/combat cleanup, killer AI callbacks and the
source death-durability path (10% for NPC kills outside BG in this pin).
`health_after=0` alone does not implement those effects. No callback is silently
claimed complete by this module. Callback fields identify conditional source
work; the caller still evaluates source combat eligibility and item/cast state.

## Starter defense growth and random semantics

`DefenseSkill` holds pure live current/max defense and an explicitly supplied
uint16-admitted defense gain configuration. `defense_gain_chance` returns None
for capped/absent skill (no source draw). For a positive gap below 7 it ports
`room*room / (7.0 * pure_max)` in software binary64, source clamp and *100, then
narrows to binary32. Gap >=7 is **Unsupported**, because the source then uses
`std::log`; no approximate logarithm or guessed skill cap is substituted.

`resolve_defense_gain` requires precisely the optional source draw. Gain=0 still
consumes the probability draw before UpdateDefense returns; pure current=0 still
consumes it but UpdateSkill cannot increase the value. Successful increments clamp
to pure max. The caller updates persistence and recomputes defense-dependent UI
values; the next attack must read updated live defense.

`PercentRoll` accepts finite binary64 [0,100) and preserves the source's strict
`floatChance > doubleRoll` comparison. `from_fraction53` is an explicit deterministic
semantic distribution: uniformly draw `0..2^53-1`, multiply by 2^-53 then 100 in
software binary64. This deliberately specifies semantic draws rather than claiming
the same MT19937/distribution implementation, entropy-consumption pattern or seed
as CMaNGOS. Record semantic purpose, range, value and order in replay state. Damage
and outcome use separate integer semantic draws. Unsupported callbacks/gaps must
not consume committed RNG or partially mutate the world.

## Verification and provenance

Source: [CMaNGOS Classic](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f),
CMaNGOS Project contributors, GPL-2.0-or-later source grant. Rust translation,
synthetic harness and tests: 2026 Benilla fork contributors, GPL-3.0-or-later.
Upstream authorship/copyright and GPL texts are preserved under `licenses/`.
Software arithmetic uses the existing pinned `rustc_apfloat` dependency and its
Apache-2.0 WITH LLVM-exception grant; see `PROVENANCE.md`.

Reviewed source and full-file SHA-256:

- `src/game/Entities/Unit.cpp`: `03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0`
- `src/game/Entities/Unit.h`: `2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1`
- `src/game/Entities/Player.cpp`: `d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e`
- `src/game/Entities/StatSystem.cpp`: `91aa0eb47c31aea9d97b64a97c1bdc350b4ea7e7fd30f1ce885a40b69112f1bb`
- `src/game/Entities/Creature.cpp`: `27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302`
- `src/game/Entities/Object.cpp`: `410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b`
- `src/game/Spells/UnitAuraProcHandler.cpp`: `04e5059490e9a7a0435c4b94b3f041aa5f26552d0948886f3d2ec4a61712294e`
- `src/shared/Util/Util.h`: `1f53eafa77ab89935c905f331d9ee7e5e2fabf1fca7f812e2e913f19243c5878`
- `src/shared/Util/Util.cpp`: `f408c8a466a183cabdf96b0f7c18907b8cbb2144d73b0b2e8b81fdf421b43133`

`tools/generate_npc_melee_fixture.py /path/to/pinned/source-files` hash-checks its
source dependencies and extracts unchanged complete chance, die, damage, armor,
crit, mage agility and creature AP/damage method bodies. Synthetic adapters supply
only the documented no-modifier source state. The starter skill branch/final float
statement are extracted from UpdateCombatSkills; proc-gate source assertions guard
survival and critter behavior. C++ is compiled without fast math/FMA, with nearest
rounding, and fail-fast undefined/float-cast sanitizers. `tests/fixtures/npc_melee_cmangos.tsv`
is original numeric output, not database content. It has 8,471 cases: 420 mage
base-dodge bit comparisons, 7,022 full melee calculations, 1,000 creature damage
field derivations and 29 starter defense-chance bit comparisons. Of the melee
rows, 5,000 use specified MT19937 output with explicit rejection-sampled bounded
draws; 2,022 explicitly cover cumulative outcome boundaries, every mage level,
crushing thresholds, zero/singleton/reversed ranges and high damage/armor limits.
Tests assert all five outcomes, both crushing-eligibility flags, all mage levels
and all outcome-die residues modulo four are represented.

Rust tests add cumulative die boundaries, sit/cast behavior, current vs max defense,
crushing cap, miss/dodge callbacks, lethal callbacks, strict probability equality,
missing/extra draws, malformed ranges/state and unchanged health on rejection.
Compile-fail tests prohibit resolution tampering and reuse after consumption.
Channel/auto-repeat admission is rejected independently of the casting predicate;
source spell-slot inspection is required on each surviving positive hit.
The actual external callback implementations, graphical combat, full native
compilation and large-gap skill growth are outside these tests and this module.
