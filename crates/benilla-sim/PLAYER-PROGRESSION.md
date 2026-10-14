# Bounded Classic player progression

`benilla_sim::player_progression` ports non-kill `Player::GiveXP`, bounded
`GiveLevel`, and `UpdateSkillsForLevel` from CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. Its rules identity is
`cmangos-8ec338a1-solo-mage-progression-unspent-talents-softfloat-v2`.

This module supplies a staged numeric/state reducer and required audited callbacks.
The bounded native ordinary-quest integration already composes real live training,
derived stats, recipe rebasing, client projection and durable publication. Neither
layer is a complete player-stat implementation or evidence of a playable real
starting zone. Production native admission remains closed. There is no production
no-op `Host`; no game rows, assets or database are bundled.

## Inputs and admission

`Catalog::new` takes a native race ID, mage class ID, explicit session rules,
selected actual `player_levelstats` and `player_classlevelstats` rows, resolved
XP thresholds, and complete queried skill dependencies. It admits levels 1–60
and rejects transitions beyond the supplied rows. It never repeats a level-one
row, synthesizes a threshold, fills an SQL gap, or extrapolates player stats.
The native compiler must establish the real race/class creation relationship;
accepting an ID-shaped pair here does not prove that relationship exists.
Attributes are bounded by the source uint8 row fields; class base health/mana
are positive uint16 values. Each table's keys must be unique and agree.
Zero strength is rejected because the source loader treats it as a missing-row
sentinel, failing level one or copying the preceding row at later levels.

`Rules::from_classic_config(max_player_level, always_max_skill_for_level, talent_rate_bits)` uses
World.cpp's configuration bound 1–100 and the pinned World.h global skill-cap
formula. `Catalog` rejects a separately supplied global skill maximum that does
not match this derivation. The rules must identify the actual session authority.
A new P2P world may explicitly select versioned Classic defaults (60 and false);
those are authored P2P rules, not a captured configuration from an old server.
This module does not silently select defaults. The resolved `Rate.Talent` binary32
bits and talent policy are part of the catalog identity.

The XP table contract is the resolved `ObjectMgr::GetXPForLevel` result:

- Below the configured cap, supply the actual positive `player_xp_for_level` row
- At the cap, supply zero. The source vector has exactly `MaxPlayerLevel`
  elements, and the getter returns zero outside it; a positive SQL row at the
  cap is ignored by the loader
- Rows/transitions above the configuration cap are rejected. The source has
  separate class-stat clamping and race-stat extrapolation there, which are not
  implemented by this bounded row-loader contract

A `SkillDependency` is a complete lookup for one live skill. An explicit `None`
SkillLine result means the row was queried and absent, so source iteration skips
it. A missing entire dependency is an error. SkillRaceClassInfo requires
`Rows::Complete`, including a proven empty result. Selection uses the first
native DBC row ID matching both nonzero race/class mask intersections. A zero
mask is not a wildcard for this lookup. All queried dependency rows, including
negative/empty relationships, belong in the compiler's content identity.

`Audit` requires explicit absence evidence for groups/raids, pets/controlled
units, used talents, shapeshifting, level scripts, dynamic/level-scaling
modifiers, resource/stat aura hooks, unsupported item effects, non-mana power
modifiers, and additional spell-crit/regeneration modifiers. `Unknown` and
`Present` both fail. The host must recheck the live dependencies on every call;
a stored assertion is not independent proof of absence.

## State and transaction API

`State::restore(catalog, snapshot)` checks the policy and canonical catalog hash,
selected create values and next XP, session level, source-derived free talents
and zero used talents,
resource values/maxima, software-derived stats, and skill identity/positions.
`NumericState::from_modifiers` adapts the existing public starter
`ModifierSnapshot` without changing the initializer or its equipment recipe.
The modifier cells must come from actual admitted equipment/passives; providing
neutral cells is not proof that a character has no modifiers.

The three reducers are:

- `give_xp(catalog, state, host, xp)` for `GiveXP(xp, nullptr)`
- `give_level(catalog, state, host, level)` for an admitted direct level change
- `update_skills_for_level(catalog, state, host, maximize)` for the source helper,
  including its explicit maximize override

Each returns `Transaction { state, host, events }` only after every callback
succeeds. The original state and host remain unchanged on missing data, unsupported
semantics, numeric rejection, or a callback error after arbitrary staged work.
`Host: Clone` must be a **deep in-memory clone**, including spell knowledge and
all ancillary player fields. Shared interior mutation, storage writes, network
publishing, and recursive quest callbacks are forbidden during staging.

Each `Observation` carries an immutable callback-time `Snapshot`, including
intermediate states that are deliberately not restorable checkpoints. Events
are observations of already applied effects, not commands to award XP again.
The owner must atomically commit progression, quest state, host spell/equipment/
combat state, command/death-event deduplication, and the event outbox before
publishing. The compiler/store must bound encoded inputs and complete relationship lists
before allocation; this typed API is not an untrusted-file parser.
This API supplies no command sequence, filesystem transaction,
rollback of external I/O, source authenticity, or network trust boundary. Bind
its catalog identity, fixed modifiers, traversal order, host policy, and complete
native rules to the owner's durable envelope. A matching untrusted hash alone
is not authorization to invent game state.

## Source order and numeric behavior

Zero XP, dead players, and players already at the configured cap produce no
source effects. Accepted non-kill XP emits the XP-gain notification with victim
GUID zero, given XP, and type byte 1. No kill-only group fields or rested bonus
are added. Current XP plus the grant uses source wrapping uint32 arithmetic
**before** threshold comparison. The reducer consumes thresholds and levels
repeatedly, retaining the old PLAYER_XP during all intermediate callbacks, then
writes the final remainder. On reaching cap, surplus XP is retained; it is not
clamped to the now-zero next threshold or discarded.

For each level, the reducer preserves this order:

1. Emit the twelve uint32 level-up packet words: new level, base HP delta, five
   power deltas (mana followed by four zeros), then five create-stat deltas
2. Set next XP, reset level-played time, set level, and update skills
3. Replace each create attribute, then create health and mana
4. Execute `InitTalentForLevel` / `UpdateFreeTalentPoints`. Below ten, the
   source no-op preserves reachable zero. At ten or above, binary32
   `(level - 9) * Rate.Talent` is truncated to uint32, then zero used points are
   subtracted. At rate one, levels ten and sixty expose one and 51 free points
5. Execute the supported numeric portions and required hooks of `UpdateAllStats`
6. Fill health only if alive, fill mana and energy, retain/clamp rage, and update
   the session's level. Pet synchronization is absent only under admission

The detailed stat order is preserved too: attributes; melee/ranged attack-power
hooks; armor; maximum health; all five maximum powers; melee/ranged crit hook;
spell crit; defense; spell-damage; regeneration; three weapon-hit hooks; armor
again; six other resistances. `SetMaxHealth`/`SetMaxPower` clamp current resources
at their original positions. `SetPower` preserves its pre-clamp equal-value early
return. Rage/focus/energy/happiness create values are the source player's
1000/0/100/0, with non-mana modifier effects excluded. Direct GiveLevel keeps XP
unchanged and equal-level requests emit no source effects.

Static attribute, health, mana and resistance cells retain the source binary32
operation sequence using the existing pinned `rustc_apfloat` dependency. Effective
stats truncate to int32 and clamp negative field values; resources round halfway
away from zero. Mage regeneration and level-dependent spell crit use the existing
source-derived `mage_resource` kernels. Finite non-subnormal modifier cells and
computed effective stats/resources/resistances are bounded to 1,000,000; invalid
conversions reject the complete transaction instead of clipping to a guessed
native value. Class base-row values remain bounded to uint16 independently.

HP/mana packet deltas preserve defined signed subtraction followed by uint32
wrapping. `GetCreateStat` returns float: a negative stat-delta float-to-uint32
conversion is outside C++'s defined domain. Those transitions are rejected before
any staged event; the oracle retains float-cast-overflow checking. Nonnegative
uint8 create-stat deltas are exactly representable, so their packet bits are exact.

`UpdateArmor` temporarily adds the agility contribution into its TOTAL_VALUE
modifier cell and subtracts it after writing armor. That binary32 round trip can
change the cell. The fixed-cell contract rejects any non-bit-exact restoration
at snapshot admission and at each armor update, rolling back the complete
transaction. Supporting those inputs requires a future mutable modifier/recipe
adapter and source comparisons across successive level changes; silently keeping
or rounding the old cell would change later armor values.

`Rate.Talent` zero is allowed by the upstream nonnegative configuration rule.
The fork rejects negative, nonfinite and subnormal nonzero rates and any supplied
level whose binary32 product cannot safely convert to uint32. There is no guessed
rate clamp. Talents remain explicitly unspent; allocation, resets and post-ten
level decreases are unsupported. Full all-class powers, forms, pets and talent
trees require separate closure. Numeric level 60 does not imply every melee
branch is admitted. The separate [defense-gain kernel](NPC-MELEE.md) now covers
logarithmic catch-up for actual levels 1..60 and pure skill maxima 1..300 using
versioned source-derived constants and software arithmetic. With automatic skill
maximization disabled, authored native tests verify late catch-up gains and exact
checkpoint replay. Other unsupported combat dependencies still reject atomically.

## Required host implementation

The numeric module requires a real host. The native ordinary-quest
`ProgressionHost` now composes `LiveTraining`, `DerivedStats` and mandatory
callback-history replay. `preflight` rechecks dependencies and transaction
constraints; `apply(event, view)` executes supported callbacks against the clone
and rejects unsupported effects. This is internal consistency, not captured-data
provenance or production capability.

For every changed existing skill, this reducer writes value/max, preserves step
and bonuses, marks a non-NEW status CHANGED, and invokes these callbacks:

1. `SkillTrainedSpells` at the new skill value and level, before create stats change
2. `SkillDefenseBonuses` for skill 95 after trained-spell processing

The host must implement the complete source `UpdateSkillTrainedSpells` closure:
all SkillLineAbility rows in native source order; race/class filters; each
SpellLearnSkill repair branch; training method and required skill threshold;
source in-world `learnSpell` versus out-of-world `addSpell`; removals; ranks,
dependents, effects, and recursive skill/spell consequences. It must reject any
closure that would change this reducer's fixed modifier cells, add/remove/reorder
skills, or produce other unsupported effects. There is no assumption that an
already-known spell or an empty supplied list makes that closure inert.
`LiveTraining` now admits bounded actual initializer learning and rejects fixed
skill, modifier, passive, proficiency and other unsupported spellbook changes.
Native genesis probes every supplied consecutive level with the actual host. It
also rejects unproved defense-value-sensitive threshold learning, which could
otherwise appear only after combat between level callbacks.

The original mSkillStatus is an unordered map. `Snapshot.skills` preserves the
host-established traversal order; this reducer never silently sorts it. A P2P
adapter needs an explicit deterministic traversal policy and proof that any
order-dependent closure is supported, or must reject it. Single-skill oracle
cases do not establish equivalence across interacting multi-skill callbacks.

`StatHook` callbacks also require actual attack/weapon-damage, crit, defense,
spell-damage and hit updates. They must not merely acknowledge a name. The
numeric mage-crit and mana-regeneration outputs supplied here do not implement
all seven spell-school or every player-field projection. The host and owner must
also propagate the new create rows into the equipment/durability recipe before
subsequent recomputation. Native `commit_quest_progression` already rebases `StatRecipe` from validated new
create rows and updates combat, skills, resources and ability bindings before
field/event projection and durable publication. Historical preparation and aura
bindings retain their original context. Production capability remains closed.

## Evidence and provenance

Source project: [CMaNGOS Classic](https://github.com/cmangos/mangos-classic),
CMaNGOS Project contributors, commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. Original source grants GPL-2.0-or-later;
this modified Rust translation, generator, and original synthetic adapters are
GPL-3.0-or-later, 2026 Benilla fork contributors, without warranty. The existing
[upstream authors](licenses/AUTHORS.md), [copyright](licenses/COPYRIGHT.md),
[GPLv2 source grant](licenses/LICENSE), and [GPLv3](licenses/GPL-3.0.txt)
notices apply. The crate and combined-artifact license boundary remains the one
in [PROVENANCE.md](PROVENANCE.md).

The generator checks full-file SHA-256 for Player.cpp/h, Unit.cpp/h,
StatSystem.cpp, SpellMgr.cpp/h, SharedDefines.h, DBCEnums.h, DBCStructure.h,
Object.cpp, ObjectMgr.cpp, World.cpp, and World.h before extracting unchanged
bodies or checking the source configuration declarations.
Important source entry points are:

- Player.cpp: GiveXP 2668, GiveLevel 2707, UpdateFreeTalentPoints 2771,
  InitTalentForLevel 2803, CalculateTalentsPoints 19284, GetSkillInfo 5327,
  SetSkill 5365/5408, UpdateSkillsForLevel 5595
- ObjectMgr.cpp: XP loading 3663, selected level getters 3946/3959,
  GetXPForLevel 7235
- World.h: GetConfigMaxSkillValue 449; World.cpp: MaxPlayerLevel bounds 548,
  AlwaysMaxSkillForLevel default 653, Rate.Talent 456 and setConfigPos 2420;
  DBCEnums.h: MAX_LEVEL 100
- Unit.cpp: modifier getters/totals 9049–9103, resource setters 9224–9326,
  player create powers 9374
- StatSystem.cpp: UpdateAllStats 83, armor 122, stamina/intellect bonuses 144–171,
  maximum health/powers 174–202

The committed fixture has 2,429 authored cases: 2,426 valid numeric/state/trace
comparisons and three explicitly rejected raw rage inputs.
These include zero/dead/cap
XP, uint32 overflow, cap-surplus retention, all transitions through sixty, 1→60 and multilevel grants across ten/cap,
negative HP/mana packet deltas, skill flags/statuses and explicit maximize,
static modifier rounding/clamping, and exact ordered field/hook traces. Invalid
raw rage-above-maximum rows are asserted to fail snapshot admission rather than
presented as valid character parity. No real game rows or source assets are used.

Talent cases include zero (both signs), minimum normal, fractional, one, greater
than one, adjacent-to-one and near-uint32-boundary rates under both skill policies.
The C++ harness extracts GiveXP/GiveLevel, InitTalentForLevel,
UpdateFreeTalentPoints, CalculateTalentsPoints, both SetSkill overloads,
UpdateSkillsForLevel, source skill range/info classifiers, stat numeric bodies,
Object/Unit setters, GetCreatePowers, GetXPForLevel, and the configuration skill
getter. Unsupported dependent systems are explicit traced adapters. Matching a
trace does not prove behavior inside those adapters, and neither does the Rust
test-only ProbeHost. Failure injection checks rollback after every host callback
of a multilevel transaction; callback snapshots check that new skill values see
the old create stats and that stat hooks precede resource refills.
An additional extracted-source assertion proves that non-exact armor restoration
can change a subsequent integer armor result. The Rust regression rejects that
domain after staging one complete level and proves whole-transaction rollback.

Reproduce without native game data:

```
python3 crates/benilla-sim/tools/generate_player_progression_fixture.py \
  /path/to/pinned/sources /path/to/pinned/ObjectMgr.cpp --check
cargo test -p benilla-sim --test player_progression
cargo test -p benilla-sim --release --test player_progression
cargo clippy -p benilla-sim --lib --test player_progression -- -D warnings
```

Oracle compilation uses C++17, FE_TONEAREST, no fast math or contraction,
warnings-as-errors, undefined-behavior and float-cast-overflow sanitizers.
Regeneration must be byte-identical. Cargo tests read the committed fixture and
need neither a C++ compiler, network, database nor client install. Linux amd64 is
the verified host; this is not a Windows/macOS build, graphical run, full workspace
gate, real-data capture, or end-to-end native starter-loop verification.
