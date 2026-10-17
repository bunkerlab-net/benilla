# Bounded source-derived mage initialization

`starter_init::initialize_mage` is a pure, transactional translation of a bounded
fresh level-one mage initialization path. Its policy is
`cmangos-8ec338a1-level1-mage-init-softfloat-v4`. It includes no native character
rows, game assets, assumed race abilities, or class/race stat presets.

## Policy compatibility

Policy v4 adds a separate seed-independent `prepare_starter` contract and rejects
newly executed initial casts during live training, including no-state handlers.
It changes initializer/input/training identities. Completed `initialize_mage`
and `first_entry` calls still reject unresolved random work. Live-training v3
keeps its snapshot layout version2; its mandatory new identity rejects older
snapshots rather than relabeling them. Native receipt/schema compatibility is
specified separately by the runtime.

Policy v3 retains unconditional, permanent, self-applied initial aura156 for the
reputation consumer described below. Policy v2 added the passive
DurationIndex0/resolved-duration0 permanence branch. The initializer policy is
included in the native runtime profile digest and live-training identity.
Profiles bound under v1 or v2 have different identities; their checkpoints and
journals reject under later policies even if a particular input has no reputation aura and
its resulting stats are unchanged. This initializer policy leaves the native checkpoint field layout
unchanged: compatibility is rejected through its mandatory profile/genesis
contracts. There is no automatic migration or rebase. Recreate a compatible
profile and fresh state from validated inputs; never relabel an old digest.

## Input and trust boundary

The caller supplies server-authoritative creation rows, actual class power type,
ordered default skills/spells, complete learning relationships, and the selected
outfit/custom item grants. `Rows::Missing` is an error; `Rows::Complete([])` means
a caller proved the relevant query empty. No constructor supplies missing stats,
items, skills, tier limits, or spell effects. Duplicate row IDs fail. This module
is a rules reducer, not a cryptographic data-origin or capture-completeness
certificate, a spell executor, or a native-world capability token.

A `SpellRow` has all 173 canonical numeric SpellEntry word positions, with SQL
`spell_template` values overlaid by the upstream server precedence. Passing raw
client Spell.dbc words without checking server overrides is invalid. Its duration
comes from the actual dependent SpellDuration row, or the pinned zero result
after proving that lookup has no row. DurationIndex0 alone does not prove the
duration value: a real row zero must be retained. The reducer requires a resolved
duration; `None` remains unknown and is rejected for initial auras. Previous-rank, talent,
spell-learning, and external-hook observations must come from captured source
relationships, not a caller's neutral assumptions. Visual/string words are not
interpreted here. All three effects are preserved. IDs in tests above 900 million
and skill IDs above 60000 are invented synthetic probes, not extracted game data.

Reachability closure must include all SkillLineAbility rows by every reached
skill AND spell, in ascending DBC row-ID order. Default skills can recursively
learn spells; those spells can add more skills and spells. Even a mapping with
`learnOnGetSkill == 0` can need its SpellEntry for profession-repair evaluation.
The caller also supplies every reached SkillLine, matching SkillRaceClassInfo
candidate, required SkillTiers row, spell-chain/talent observation, raw SkillLineAbility.forward_spellid, and complete
`spell_learn_spell` map including derived effect-36 relations. Missing a
critical relationship blocks initialization. An unresolved active executor only
makes that known ability visibly unavailable with a reason; it does not hide its
learning side effects.

## Source ordering and numerical policy

The fresh Unit constructor sets stat modification off and initializes UnitMods
as 0/1/0/1, create resistances to zero, and dodge enabled. Player's constructor
sets parry/block false and proficiency masks zero. These are source initial
conditions, not replacement defaults for absent content.

The admitted sequence is:

1. Create attributes/health/mana from the selected level-one SQL rows
2. Calculate and fill initial health/mana from create stamina/intellect
3. Learn default skills, including recursive skill-trained spells
4. Learn ordered default spells, initial casts, trained skills and dependencies
5. Process outfit grants in source order, then custom SQL grants
6. Perform the second inventory pass, proven inert for the admitted equipment set
7. Enable stat calculation and materialize UpdateAllStats attributes/resources
8. Preserve the created current health/mana, clamping to final maxima

A key source detail is that `Player::Create` does not enable stat modification.
Initial aura and item modifier cells change while effective fields remain at
create values. `LoadFromDB` later enables stats, calls UpdateAllStats, and restores
saved current resources. The trace explicitly records this deferral; refilling
current resources from final maxima would be incorrect.

`initialize_mage` combines fresh creation with final stat materialization.
A native first-entry adapter must additionally call `first_entry_skills` and use
its returned skill map. It translates `_LoadSkills` MONO/range normalization and
tier-based step reconstruction. Source `_SaveSpells` omits dependent spells;
creation defaults and skill-trained spells are dependent. The replay therefore
starts with an empty spellbook, runs `_LoadSpells` training for **every** loaded
skill, then LearnDefaultSkills and learnDefaultSpells. Passive auras are recast,
not saved/reloaded. Executed effect-36 learning produces nondependent saved spells
and currently fails this narrower first-entry gate.

`SkillStatusMap` is an unordered map. The admitted subset proves each skill's
training decisions invariant between loaded and constant-setter states, and
requires all setters for one skill to agree. Only after that proof may the replay
use a canonical iteration order. Recursive learning may not insert/remove skills
while that map is traversed. The replay compares known ability membership/active
state, passive aura membership, proficiency state and final skill values against
the proved creation closure; changed results block rather than disappear.
Equipped items must still meet source skill requirements. `first_entry` also
returns a canonical phase trace; `first_entry_skills` returns just its skill map.

That gate also proves the static stat modifiers independent of reload order:
initial stat auras must be passive (recast rather than persisted), each percent
cell has at most one contributing aura, and bounded flat-cell sums remain exact
integers. Unsupported saved-aura stacking, conflicting step setters, changed
trained-spell closure/removals, and changed equipment eligibility are explicit
errors. The gate is not a full database/login implementation: saved quests,
cooldowns, arbitrary prior character state, runtime scripts, or uncaptured
relationships remain outside this fresh bounded state. No specific race's actual
spell/skill/equipment rows have been assumed or verified.

All admitted stat arithmetic uses LLVM APFloat software binary32, nearest-even
at each source operation, with toward-zero field conversion and ties-away
resource rounding. Constant initial effect amounts convert base points and dice
to binary32 separately before adding, matching CalculateSpellEffectValue.
Cached integer sums, float-to-integer conversions, and skill-aura amounts outside
the signed-16-bit domain fail closed. There is no host-float arithmetic.
Inputs/intermediates outside
the documented bounded valid domain are rejected instead of invoking source
undefined conversion behavior. Create attributes are unsigned byte values;
effective attributes, resources and aggregate modifier cells are bounded by one million. Negative stat fields clamp to zero through the source SetStatInt32Value behavior. Source `-100%`
modifier handling, total-percent nonpositive clamp, stamina/intellect break at
20, integer item-stat base cells, and final current-resource clamps are retained.

## Source-dependent symbolic preparation

`prepare_starter(input, numeric_dependencies, order)` runs the same default-skill,
default-spell and recursive `Init::learn` paths as completed initialization, then
replays actual first-entry learning. The returned `PreparedStarter` is opaque and
has no public constructor or deserializer. It retains both epoch plans, their
order-evidence kinds, the complete input identity, the complete numeric envelope,
and deterministic creation/entry projections. `validate_against` rebuilds the
whole preparation; serialization exposes its identity only. No seed, roll,
caller-authored occurrence list or claimed draw count enters preparation.

The source path uses a private shared phase classifier, separate from
`InitialCastPlan::from_authored` and its synthetic prerequisite assertions. At
an actual cast transition it records the retained raw row and source identity,
resolves the exact cast-time/duration indices, and projects the classified
operations. Default and recursive duplicate mentions are suppressed only by the
actual known-spell transition. Detect3050 remains an occurrence with no scheduled
amount or handler. Targetless operations run before self-holder operations; all
holder aura entries are registered before modifiers are applied.

Only the audited targetless Dodge81 effect20 shape may leave a pending amount:
base points0, base dice0, sides6, no scaling or extra effect consumer. It records
`PendingInitialAmount` and the sealed plan's `irand(0,6)` operation. It emits no
fake `EffectApplied`, calculates no placeholder amount, and samples no RNG. Its
empty handler changes neither state nor control flow; the source Unit constructor
already enables dodge. Each epoch permits at most one pending occurrence. Any
other stochastic, triggered, scripted or unsupported recursive cast rejects.
The completed wrappers cannot consume this projection as completed random work.

`StarterInitialDependencies` requires explicit store and hook-scope identities,
full per-spell source hashes, server coefficient fields, and the exact sorted
union of IDs actually cast in the two traversals. `Unknown`, `Missing { index }`,
and `Found(raw tuple)` remain distinct. Missing requires the exact raw index;
Found requires the matching indexed row. The bounded synchronous policy admits
only matching zero cast-time tuples and duration tuples `(0,0,0)` or
`(-1,0,-1)`. Derived duration must equal the supplied `SpellRow.duration_ms`.
Missing duration21 never establishes racial permanence. Conflicting observations
of one shared index reject even when both would resolve to zero. No numeric row
or `Some(0)` is invented by the bridge.

Creation retains input default-vector order. First entry requires an explicit
exact permutation of normalized loaded-skill keys and uses that vector for the
actual training traversal. Missing, duplicate or extra keys reject. Authored
order is a synthetic contract, not observed SQL/DBC order. Observed order retains
capture and loader identities, without authenticating them. Unknown and reserved
projection-equivalence evidence reject; the legacy canonical deterministic
projection proof does not certify stochastic occurrence order. Both epoch tags
and order kinds bind identities, including when their final stats agree.

`LiveTraining::from_prepared` revalidates and binds the preparation identity in
addition to the complete source input. `source_identity` and
`preparation_identity` remain separately available. Every newly reached live
initial cast rejects before execution, even a deterministic empty handler whose
final skills and modifiers would be unchanged. Existing known-spell no-ops and
ordinary supported active-spell learning retain their previous semantics.

These contracts can be tested with authored generic evidence. Neither opaque
identity nor an `ObservedLoadedOrder` tag verifies a real capture. Native use
must independently retain/revalidate complete hooks, area scope and loaded
source consistency. Actual Human capture/order/DBC gaps and native admission
remain closed. The native `FirstWorldEntryOnly` stream boundary retains creation
as an unsampled prerequisite projection and realizes only the selected entry
plan once, before tick0; preparation itself never claims a completed RNG receipt.

## Supported semantics

- Default language skills 300/300; weapon/level skills 1/5; actual maximized,
  mono/rank, tier and predefined-step branches
- GetSkillInfo race AND class masks and DBC row-ID ordering
- Recursive skill-to-spell and spell-to-skill learning, including first effect
  118, skill-step effect 44's early return, and bounded source skill bonuses
- Constant, direct self initial casts with proven permanent duration for auras
  (duration -1 or the pinned passive DurationIndex0/raw-duration0 branch)
- Stat aura 29, base-percent stat aura 80, total-percent stat aura 137
- Resistance aura 22, base resistance 83, total resistance percent 101,
  base resistance percent 142; all seven schools are retained
- Unconditional permanent self-applied initial aura156, retained by source spell,
  effect, opcode, misc and calculated amount for the reputation consumer; no stat
  cell changes, dynamic application/removal or faction-specific aura190
- Temporary/permanent skill auras 30/98, including retained aura records when
  the skill is not yet learned; SetSkill reapplies these after recursive learning
- Proficiency effect 60, parry/block effects 22/23, learn-spell effect 36,
  and explicit source-empty effects 20/21/25/26/37/39/118
- Ordinary body slots, rings/trinkets, cloak, main-hand weapons, and ordinary
  16-slot backpack stacking, preserving outfit-before-custom order
- Source item class/race/level, required-skill/spell and weapon/armor proficiency
  checks; unapplied carried-item stats stay unapplied
- All ten item stat slots, armor and six nonphysical resistances
- Retained raw weapon damage/delay inputs, without inventing combat damage
- Effective-stat bridges to existing mage mana-regeneration and spell-crit kernels

Source constants such as CLASS_MAGE, item stat types, skill categories, and
Item::GetSkill's weapon/armor lookup are algorithmic enum values. No specific
race's initial spells or gear is hardcoded. Active availability is supplied by a
separate gameplay compiler; this initializer does not grant it by effect ID alone.

## Equipment durability callbacks and snapshots

The initializer retains explicit item-template maximum durability. Fresh equipped
current durability equals that supplied maximum; zero maximum denotes a
nondurable item. `InitializedMage.stat_recipe` retains the admitted modifier cells
and per-slot item modifiers without learning/equipment replay.

`StatRecipe::set_item_broken(slot, broken, live_resources)` applies or removes
that item's modifiers in source item-stat/armor/resistance order. It clamps live
current health/mana after every modifier and never refills them. This ordering
matters when an early removal lowers a maximum and a later removal raises it.
The result contains effective attributes, resistances, max/current resources,
regeneration/crit and disabled broken-weapon slots. The caller owns durability
loss arithmetic, must invoke the stat callback before crossing durability zero,
and must commit the complete combat frame atomically. Current skills and
passives are not reapplied or changed by a break/repair callback.

`StatRecipeSnapshot` has strict serde fields and stores software-float cells as
integer bits. `StatRecipe::restore(fresh, snapshot)` requires the authoritative
fresh recipe from the caller's profile, replays the supplied broken flags and
requires exact item identities, modifiers and cell agreement. Edited cells,
foreign items, missing/duplicate flags and live resource/max mismatches fail.
Structural deserialization is never provenance. The caller must bind that fresh
recipe to its actual content/profile digest and independently validate item
current durability versus these broken flags.

## Explicit exclusions and failure modes

Unresolved or conditional passives cannot become ordinary unavailable active
abilities. Conditional passive eligibility, unknown initial effects/auras,
unknown attribute flags, hooks, scripts, rank/talent transitions (including
nonzero SkillLineAbility spellbook-successor links independently of previous-rank
metadata), random/scaled
initial effects, unsupported equipment conditions, recurring/proc effects,
nonpermanent auras, duplicate overlapping aura keys, and unsupported skill
removals are errors. A failed call returns no partly initialized state.

Equipped on-use item spells do not execute during initialization. Equipped
on-equip/proc spell hooks are rejected; they are not silently ignored. Item sets,
random properties, duration, custom item hooks, nonzero unclassified item flags,
reputation/honor requirements, bags, special bag families, offhands, shields,
ranged slots, and inventory overflow currently block this bounded path. No
manual possession of game data is required by the algorithm: user-local rows
that satisfy the admitted semantics can initialize successfully. No real
Human/Gnome mage profile has been verified by the synthetic tests.

## Remaining passive consumers

Every aura outside the supported list above currently rejects initialization.
The following source opcode/consumer inventory guides the next row-driven
expansion; it does **not** claim any selected race/class actually has these rows.

| Aura opcode | Source state or consumer that must be implemented or explicitly gated |
| --- | --- |
| 17 | HandleModStealthDetect and visibility/stealth detection |
| 190 | Player::CalculateReputationGain faction-specific kill reward modifiers |
| 117, 178 | Unit::MagicSpellHitResult mechanic/debuff resistance |
| 59, 168 | Unit melee/spell damage bonuses against creature types |
| 169 | Crit-versus consumers in DealDamageBySchool, DoAttackDamage, SpellCriticalBonus |
| 47, 49, 51, 52 | Parry, dodge, block and weapon crit percentage updates |
| 54, 55, 57, 71 | Melee/spell hit and general/school spell crit updates |
| 84, 85, 88, 110, 116, 134, 161 | Health/power regeneration and combat/interrupted variants |
| 143 | Exclusive resistance stacking and resistance updates |
| 75 | Chat-language selection |
| 82, 155 | Underwater breathing/timer behavior |
| 44, 45, 151 | Creature/resource/stealth tracking fields and affected visibility/UI |

`HandleNoImmediateEffect` is not a no-op: it retains an aura consumed later by
the named subsystem. Aura156 is retained without modifying stat cells, including
first-entry reconstruction. Its source-order list must match again on first
entry; live skill training and level-up cannot change this fixed initial list.
The [reputation catalog](REPUTATION.md) binds exact amounts, effect identities,
passive lifecycle metadata and `SpellRow::source_hash()`. That domain-tagged
SHA-256 includes all 173 supplied words, duration/rank resolution, talent/hook
observations and ordered learned-spell relationships, distinguishing missing
from explicitly empty relationships. It is a mechanical binding, not proof of
capture completeness. Native quest construction cross-checks it against the
initializer and CombatGenesis; an absent reputation catalog keeps all reached
reputation consumers unavailable.

The bounded aura156 path requires the passive flag, no stance restriction or
caster aura state, no aura-interrupt removal, self target, permanent resolved
duration, constant effect amount and the existing trigger/proc/hook exclusions.
It preserves misc metadata without using it as a faction filter. Other aura
opcodes in the inventory above remain unsupported.

### Actual-row motivation and unresolved client evidence

The public [Classic-DB creation spell source][creation-db] at
`ec4f596146be6467ea93c57397858e329e2db852` includes Human Mage Diplomacy 20599.
The pinned [core Spell.sql][diplomacy-source] row has passive attributes 0x50,
effect 6/aura156, base points 9 and base dice 1: the constant amount is 10.
ProcChance 101 and the nonzero damage/bonus coefficients are not a reason to
change this amount; reputation consumes the retained amount directly.
DurationIndex21, RangeIndex1 and CastingTimeIndex1 are references, not resolved
DBC values. No actual client duration/range/cast-time row or real playable
Human/Northshire profile is established here. Source-shaped tests use invented
IDs and explicitly synthetic resolved durations; they are not copied game rows.
A full compatible installed database also requires its installer updates and
core migrations; a public base dump is not a completed capture or admission.

[creation-db]: https://github.com/cmangos/classic-db/blob/ec4f596146be6467ea93c57397858e329e2db852/Full_DB/ClassicDB_1_12_1_z2815.sql.gz
[diplomacy-source]: https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/sql/base/dbc/original_data/Spell.sql

## Verification

`tests/starter_init.rs` compares 840 synthetic stat cases with extracted original
CMaNGOS bodies for GetTotalStatValue, GetTotalResistanceValue, stamina/intellect
bonuses, UpdateMaxHealth/Power, UpdateArmor, UpdateAllStats, the modifier switch,
HandleModTotalPercentStat, and the pre-weapon _ApplyItemBonuses block. It also
checks the unchanged Player::Create prefix and constructor/deferred-update source
ordering. The source generator rejects mismatched file hashes. It compiles with
no fast math, contraction off, FE_TONEAREST, and undefined/float-cast sanitizers.

A second independent extracted-source oracle covers 108 synthetic default/step
skill cases using original GetSkillInfo, LearnDefaultSkills, SetSkillStep,
GetSkillRangeType, and profession classification. Another 1,296 vectors use the
original `_LoadSkills` body plus SetSkillStep replay. An unchanged `_LoadSpells`
and learnDefaultSpells source harness also checks loaded-skill-before-default
phase order and dependent flags. Forty original effect-60 vectors check zero,
disjoint and overlapping weapon/armor proficiency masks. First entry rejects
unequal overlapping masks because their source grant operations do not commute.
An additional 144 vectors use the original item-bonus block to check break/repair values and every
intermediate resource clamp. Tests also exercise recursive
learning, source row ordering, initial skill bonus reapplication, current-resource
preservation, carried versus equipped stats, missing rows, unsupported initial
passives, item-effect blockers, and storage/source-order failures.

A sibling callback oracle extracts both complete SetSkill overloads and
UpdateSkillTrainedSpells, plus the skill-status enum, from hash-pinned source.
Its 37 authored cases distinguish callback-before-reset, temporary-before-permanent
reapplication, existing/new skills, Defense ordering, threshold and race/class
filters, profession repair, in-world endpoint choice, and removal boundaries.
The addSpell/learnSpell/removeSpell endpoints and aura installation are explicitly
authored adapters, not extracted recursive spell learning. Six initializer-shaped
vectors are compared with the actual Rust learning path in
`tests/starter_skill_callbacks.rs`, including reversed loaded row order and
retained bonuses. Neither harness provides real Human DBC rows or observed order.

Run from this crate's root:

    python3 tools/generate_starter_init_fixture.py /path/to/pinned/upstream
    python3 tools/generate_starter_skills_fixture.py /path/to/pinned/upstream /path/to/ObjectMgr.cpp
    python3 tools/generate_starter_effect_amount_fixture.py /path/to/pinned/upstream
    python3 tools/generate_starter_callbacks_fixture.py /path/to/pinned/core-root --check
    python3 -m unittest discover -s tools -p test_starter_callbacks_oracle.py
    cargo test -p benilla-sim --test starter_init --test starter_skill_callbacks
    cargo clippy -p benilla-sim --all-targets -- -D warnings

Twenty-four additional vectors use the unchanged WorldObject effect-value body
to check binary32 rounding/cancellation before integer aura and skill fields.

These tests establish the stated algorithms and admission boundaries. They do
not establish end-to-end playable Classic, a verified install, an unrestricted database
save/login implementation, or native-content profile parity.

## Provenance and license

Translated from CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, CMaNGOS Project contributors,
GPL-2.0-or-later. Rust translation and original oracle adapters are 2026 Benilla
fork contributors, GPL-3.0-or-later. Existing crate licenses/AUTHORS.md,
licenses/COPYRIGHT.md, upstream GPLv2 and applicable GPLv3 notices apply.

Reviewed source file SHA-256 values:

- Entities/Object.cpp: `410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b`
- Entities/Player.h: `acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb`
- Entities/Player.cpp: `d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e`
- Entities/Unit.cpp: `03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0`
- Entities/StatSystem.cpp: `91aa0eb47c31aea9d97b64a97c1bdc350b4ea7e7fd30f1ce885a40b69112f1bb`
- Spells/SpellAuras.cpp: `5e60510d23b789f8f0fb584da8f05a5be675f1cb4024ce997c989cde00105fde`
- Spells/SpellEffects.cpp: `d4335082b8b3ee60d9bc200fbb47e87887dbdbe8b78a474219b68b452744520d`
- Spells/SpellMgr.cpp: `d1a7064febbe3a7657e85931c4bda89a09adfabc89b7cbed23fdbffd6e37c22d`
- Spells/SpellMgr.h: `91decb42afdb694496271d704c0e8a48c9c6fb4866b6c817e72e4b7cd2d3b12d`
- Globals/ObjectMgr.cpp: `dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b`
- Entities/Item.cpp: `5e401746527531beb521d3eeaa6bc0b55fceaa0d4aef1e780d4f429a545f72e4`
- Entities/ItemPrototype.h: `64768cb942d73c17ce00fe76f953a3d7ca3e5e617a5f4c798cb5d259bf931756`
- Globals/SharedDefines.h: `4832b8e0da45211143acae7c42173fea42732e2acdbf190692725e51b020bfe8`
- Server/DBCStructure.h: `354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50`

Source repository:
https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game
