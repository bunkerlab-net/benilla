# Bounded source-derived mage initialization

`starter_init::initialize_mage` is a pure, transactional translation of a bounded
fresh level-one mage initialization path. Its policy is
`cmangos-8ec338a1-level1-mage-init-softfloat-v2`. It includes no native character
rows, game assets, assumed race abilities, or class/race stat presets.

## Policy compatibility

Policy v2 adds only the admitted passive DurationIndex0/resolved-duration0
permanence branch described below. The initializer policy is included in the
native runtime profile digest. Profiles bound under v1 have a different identity;
their checkpoints and journals must reject as incompatible under v2. There is
no automatic migration or rebase, even for inputs whose resulting stats are
unchanged. Recreate a compatible profile and fresh state from the validated
inputs; never relabel an old digest or checkpoint to bypass this boundary.

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
| 156, 190 | Player::CalculateReputationGain, including faction-specific rewards |
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
the named subsystem. A future deferred-passive mechanism must retain the actual
spell/effect/opcode/misc/amount and relevant attributes, with source provenance,
and prohibit every affected unavailable consumer. For example, reputation-gain
state cannot vanish merely because combat works; reputation rewards must remain
unavailable until that modifier is applied. The current patch provides no such
deferral and grants no neutral substitute for rejected passive state.

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

Run from this crate's root:

    python3 tools/generate_starter_init_fixture.py /path/to/pinned/upstream
    python3 tools/generate_starter_skills_fixture.py /path/to/pinned/upstream /path/to/ObjectMgr.cpp
    python3 tools/generate_starter_effect_amount_fixture.py /path/to/pinned/upstream
    cargo test -p benilla-sim --test starter_init
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
