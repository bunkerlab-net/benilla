# Simulation provenance and license boundary

This crate is a deliberately partial server-simulation port for this fork. It is not
part of upstream Benilla's client-only scope. Its GPL-3.0-or-later package license is
explicit and does not inherit the workspace's MIT OR Apache-2.0 default. Existing
Benilla source retains its original notices. A distributed combined derivative that
links this crate must satisfy the GPL's conditions; the combined artifact cannot be
represented as MIT/Apache-only. Private use is not a guarantee about other rights.

## Source

- Project: CMaNGOS Classic, CMaNGOS Project contributors
- Repository: https://github.com/cmangos/mangos-classic
- Commit: `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`
- File: `src/game/Tools/Formulas.h`, namespace `MaNGOS::XP`
- SHA-256: `ca8b55fd022b0c0191255bc457574ccafce5eeb2f99a447466ba75be4717652c`
- Source grant: GNU GPL version 2, or at the recipient's option any later version
- Rust modifications/translation: 2026, Benilla fork contributors, GPL version 3 or later
- Upstream authorship, copyright notice and GPLv2 text are preserved under `licenses/`
- The applicable GPLv3 text is `licenses/GPL-3.0.txt`

## Implemented surface

`is_trivial`, `quest_green_range`, `gray_level`, `color_code`, `zero_difference`
translate the corresponding integer helpers. `base_gain` reformulates `BaseGain`
using exact integer ratios; `round_ties_even` explicitly defines rounding.

The accepted input domain is level 1..=255. Source uint32 overflow outside that domain
is not reproduced. `color_code` preserves the source's unsigned subtraction at level
1, even though its equal-level result is green. XP eligibility uses `is_trivial`, not
`gray_level` or display color, because those source thresholds are not interchangeable.

Deterministic deviation: rational arithmetic eliminates upstream float32 intermediate
rounding. The fixture `tests/fixtures/rounding_differences.tsv` exhaustively enumerates
integer-rounding differences for level pairs 1..=63, with both results. These are
intentional differences, not claims of exact `Gain` parity. Changing this policy needs
a simulation-rules version change once these primitives are integrated in multiplayer.

## Verification

`tools/generate_xp_fixture.py` checks the pinned source's hash, compiles the original
helper bodies using g++ C++17, no fast math, and FE_TONEAREST, and writes the oracle
fixture. It does not use the Rust implementation. Tests compare all 3,969 level pairs,
assert exact integer helper parity, bound float32 approximation error, and check every
rounded-value deviation against the committed list. Unit tests exercise all nonzero
byte-sized level pairs, zero rejection, caps, and tie rounding. Cross-platform end-to-end
replicated simulation has not been tested by these formula tests.

## Exclusions

No `XP::Gain` creature filtering, elite/damage/rate modifiers, group allocation,
rested XP, level-up tables, full quest lifecycle, production AI/combat, or durable persistence is provided.
`encounter` is an original synthetic headless harness using these XP primitives; its
fixed damage and simple killer-only rewards are not translations of CMaNGOS combat.
See `ENCOUNTER.md` for its deliberately narrow deterministic contract.
No Classic-DB rows, Playerbots code, client assets, FrameXML, or game-install files are
included. This crate includes algorithmic source and numeric test outputs only.

## Classic stat and armor kernels

`src/classic.rs` is a further GPL-3.0-or-later translation/reformulation from the
same pinned CMaNGOS Classic commit and source license grant. The existing
`licenses/AUTHORS.md`, `licenses/COPYRIGHT.md`, upstream GPLv2 text and applicable
GPLv3 text also cover this work. Rust changes and the independent oracle harness
are 2026 Benilla fork contributors. No database rows or Blizzard files are used.

Reviewed upstream files (full-file SHA-256):

- [`src/game/Entities/StatSystem.cpp`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/StatSystem.cpp):
  `91aa0eb47c31aea9d97b64a97c1bdc350b4ea7e7fd30f1ce885a40b69112f1bb`;
  `Unit::GetHealthBonusFromStamina`, `GetManaBonusFromIntellect`,
  `UpdateMaxHealth`, `UpdateMaxPower`, `Creature::UpdateMaxHealth` and
  `Creature::GetHealthBonusFromStamina`
- [`src/game/Entities/Unit.cpp`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp):
  `03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0`;
  `Unit::CalcArmorReducedDamage` and its integer assignment in
  `Unit::CalculateMeleeDamage`
- [`src/game/Entities/Unit.h`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.h):
  `SubDamageInfo::damage` is `uint32`, establishing truncation at that callsite
- [`src/game/Entities/Creature.cpp`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Creature.cpp):
  `Creature::SelectLevel` inspected to document required inputs/fallbacks;
  not ported by this module

The reference helpers preserve source float operation ordering. The rational
armor helper uses an explicitly versioned policy, with every float-rounding
difference enumerated in the declared bounded exhaustive domain. The separate
software-binary32 armor helper preserves the source operation sequence with
integer-limb arithmetic and its own policy identity.
See [CLASSIC.md](CLASSIC.md) for the input domains, numerical contract, source
prerequisites and excluded mechanics. The original `encounter` continues to use
its existing fixed-damage rules; adding these formulas does not relabel that
harness as Classic combat.

`tools/generate_classic_fixture.py` hash-checks the source, extracts original C++
bodies verbatim and compiles `tools/classic_oracle.cpp` with strict float settings.
It generates original numeric test outputs only. No source file or game content
needs downloading during Cargo/CI tests. The seven new tests cover bit-exact armor,
stat outputs, exhaustive documented rational deviations and rejected boundaries;
these tests alone do not prove native world import or graphical playability.

### Software arithmetic dependency

`src/classic/softfloat.rs` translates the CMaNGOS armor sequence using the published
[`rustc_apfloat`](https://github.com/rust-lang/rustc_apfloat) crate, pinned by
`=0.2.3` plus the committed Cargo.lock identity/checksum:

- Published version: `0.2.3+llvm-462a31f5a5ab`
- Registry SHA-256: `486c2179b4796f65bfe2ee33679acf0927ac83ecf583ad6c91c3b4570911b9ad`
- Published crate VCS revision: `eeaacad81247af65d4043cb3e32d023a652d7951`
- LLVM port revision: `462a31f5a5abb905869ea93cc49b096079b11aa4`
- License: `Apache-2.0 WITH LLVM-exception`, as stated by the upstream package;
  its `LICENSE.txt` and `LICENSE-DETAILS.md` accompany the dependency source
- Dependencies: existing workspace `bitflags` and `smallvec`; no native library
  or arbitrary source download/build requirement is introduced

Cargo ignores build metadata in version requirements, so the lockfile checksum
is part of the reproducible source contract. Do not silently update that dependency
or numeric policy in a distributed simulation. Player source vendoring must keep
this crate's license files with its source, as with the other locked dependencies.
The GPL-3.0-or-later CMaNGOS translation and combined artifact notices still apply.

## Native quest reward and eligibility kernels

`src/quest.rs` is a GPL-3.0-or-later translation of numeric reward operations and
three independent eligibility predicates from the same pinned Classic commit.
The CMaNGOS Project authorship/grant and preserved license files above apply;
2026 Rust translation, bounded validation and synthetic oracle adapter are Benilla
fork contributors. No quest rows/text, client assets or native database are used.

Reviewed full-file SHA-256 values:

- [`src/game/Quests/QuestDef.cpp`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Quests/QuestDef.cpp):
  `e4f868e95421c2be5b90bd046178b6f4413dc5203e945cf5735b9bac26104cfb`;
  `Quest::XPValue`, `GetRewOrReqMoney`, `GetRewMoneyMaxLevel`, loader field types
- [`src/game/Quests/QuestDef.h`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Quests/QuestDef.h):
  `a03821312ade7e0e30e620bf37d68e32899d65494e37887c9053eee50ebfa66a`;
  native field/accessor types and `QUEST_FLAGS_NO_MONEY_FROM_XP = 0x0100`
- [`src/game/Entities/Player.cpp`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp):
  `d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e`;
  `SatisfyQuestLevel`, `SatisfyQuestClass`, `SatisfyQuestRace`, and only the
  XP/currency block in `RewardQuest`. `CanTakeQuest`, `CanAddQuest`,
  `CanRewardQuest`, prerequisite/chain/breadcrumb predicates and `GiveXP`
  were audited for dependencies, not ported as complete methods
- [`src/game/Entities/Player.h`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.h):
  `acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb`;
  `ModifyMoney` audited to preserve separate ordered currency adjustments;
  clamping/state mutation is not implemented by the reward quote

`tools/generate_quest_fixture.py` verifies hashes before inserting exact source
bodies and an unchanged RewardQuest statement block in `tools/quest_oracle.cpp`.
The g++ adapter enables FE_TONEAREST, disables fast math/contraction and enables
undefined/float-cast-overflow sanitizers. Numeric fixtures are original arithmetic
probes. `tests/quest_parity.rs` compares 16,114 source-oracle results and checks
invalid levels/identities/rates, conversion overflow, rounding and cap/cost rules.
The Rust implementation uses the same pinned software-binary32 dependency
identified above, and has a distinct quest numeric policy identity.

See [QUESTS.md](QUESTS.md) for accepted domains and the required native
compiler/runtime contract. These callable formulas are not a full quest system;
no existing fixture semantics, native-source gates or persistence rules change.

## Classic player/NPC target relationship kernels

`src/classic_targets.rs` translates the bounded relationship branches described
in [CLASSIC-TARGETS.md](CLASSIC-TARGETS.md), under GPL-3.0-or-later, from the same
pinned CMaNGOS Classic commit and GPL-2.0-or-later source grant. Existing upstream
notices under `licenses/` apply. Rust translation and the original bounded oracle
harness are 2026 Benilla fork contributors. No faction rows, database content,
client assets or Playerbots code are included.

Additional inspected source files (full-file SHA-256):

- `src/game/Entities/Relations.cpp`:
  `06e453c80b33cddf1606004055a5d1407a46007b20fef73c5dc917156256f77b`;
  `GetFactionReaction`, `Unit::GetReactionTo`, `CanAttack`, `CanAttackNow`,
  `CanAssist`, `CanAttackSpell`, `CanAssistSpell`, `CanAttackInCombat`,
  `CanAttackServerside` and `CanAttackOnSight`
- `src/game/Server/DBCStructure.h`:
  `354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50`;
  `FactionEntry::HasReputation` and `FactionTemplateEntry` fields/helpers
- `src/game/Entities/Unit.h`:
  `2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1`;
  native UnitFlags and relationship method contracts
- `src/game/Spells/SpellDefines.h`:
  `07c3fd11a40c993ab8feeb718d21a7a2ae9b5c1700efd2e03ac53e76c6f938bf`;
  AttributesEx2 and AttributesEx3 restriction bits
- `src/game/Server/DBCEnums.h`:
  `cfd4b3d04cc31773173e9d47bb187492f7f144e3cf0f783d8fbf2d7969ebd4ce`;
  native faction-template flags, rejected in this initial capability profile
- `src/game/Spells/Spell.cpp`:
  `ebcabd3073f188541e6a1abcc82a8d0ee6d48d360dc5ab16d9e63b3e87d75201`;
  `Spell::CheckCast` harmful/helpful target dispatch and separate caster checks

The already recorded `Unit.cpp` identity also covers the inspected `Unit::Attack`
and `UpdateMeleeAttackingState` callers. They are not ported by this capability.
The simulation reuses `benilla-formats`' existing faction row/reaction types with
its default features disabled. Raw metadata accessors added to the original
client loader contain no translated gameplay algorithm; its client comparator
and original license remain unchanged.

`tools/generate_target_fixture.py` verifies the source identities and extracts
original C++ constants and full comparator/target method bodies for independent
oracle execution. The committed digest fixtures cover 6,912 comparator vectors
and 24,576 target vectors. Missing/unsupported native state remains explicitly
rejected; this evidence does not remove native world/spell/combat runtime gates.

The source-state extension also extracts exact tracked-reputation relation bodies
and independent initiative/combat-entry predicates with
`tools/generate_relation_reputation_fixture.py`. Its original harness produces
32,768 directional relation vectors and 18,496 lifecycle vectors; digest fixtures
are checked by `tests/reputation_relations.rs`. It uses the source-native reputation
module below, never client FactionInfo matching. Additional pinned source:

- `src/game/Entities/Creature.cpp`:
  `27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302`;
  `Creature::CanInitiateAttack`, lines 2271–2283
- `src/game/Entities/Unit.h` (hash above): `Unit::CanEnterCombat`, line 2386,
  plus UnitState and UnitFlags declarations
- `Reputation/ReputationMgr.{cpp,h}`, `Globals/SharedDefines.h` and
  `Server/DBCStructure.h` use the source-native reputation hashes in
  [REPUTATION.md](REPUTATION.md)

The harness compiles the actual GetReactionTo/reputation/attack bodies and does
not replace tracked faction reactions with predetermined colours. Unsupported
branches remain explicitly excluded. No native runtime witness or admission is
established by these standalone tests.

## Bounded mage projectile and periodic-damage execution

`src/classic_spell/` is a GPL-3.0-or-later stateful translation of selected paths
from the same pinned CMaNGOS Classic commit. CMaNGOS Project attribution,
GPL-2.0-or-later source grant and preserved upstream notices under `licenses/`
apply. The original Rust translation and synthetic oracle adapter are 2026 Benilla
fork contributors. No client assets, native spell rows or Classic-DB content are
included. See [SPELLS.md](SPELLS.md) for exact component domains, source branches,
RNG/rounding policy, required caller contracts and remaining native-entry blockers.

New reviewed source files and full-file SHA-256:

- `src/game/Spells/SpellEffects.cpp`:
  `d4335082b8b3ee60d9bc200fbb47e87887dbdbe8b78a474219b68b452744520d`
- `src/game/Spells/SpellAuras.cpp`:
  `5e60510d23b789f8f0fb584da8f05a5be675f1cb4024ce997c989cde00105fde`
- `src/game/Spells/SpellMgr.cpp`:
  `d1a7064febbe3a7657e85931c4bda89a09adfabc89b7cbed23fdbffd6e37c22d`
- `src/game/Entities/Object.cpp`:
  `410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b`
- `src/shared/Util/Util.h`:
  `1f53eafa77ab89935c905f331d9ee7e5e2fabf1fca7f812e2e913f19243c5878`
- `src/game/Spells/Spell.h`:
  `62ee5ad8a5d5c3dab9a75923daadcf01b59c86ae8bce82752db3b9d3db694dee`
- `src/game/Spells/SpellMgr.h`:
  `91decb42afdb694496271d704c0e8a48c9c6fb4866b6c817e72e4b7cd2d3b12d`
- `src/game/Spells/SpellAuras.h`:
  `98f31e0af6264d68180996850fa74bbac79fe9777680c82f75b07568da109d63`

Previously recorded `Spell.cpp`, `Unit.cpp`, `Player.cpp`, `SpellDefines.h` and
DBC structure identities also apply. `SpellMgr.h`'s
`IsEffectHandledImmediatelySpellLaunch` and `SpellAuras.h`'s `UpdateAura` forwarding
were inspected to establish launch/effect and aura-update ordering. The module
uses the same pinned software-arithmetic dependency identified above. Its
binary32 square root uses an exact software-binary64 midpoint comparison; there
is no new numerical dependency or host-libm authority.

`tools/generate_spell_fixture.py` verifies source hashes and inserts unchanged
function bodies or named contiguous blocks into `tools/spell_oracle.cpp`.
The adapter explicitly supplies the bounded no-modifier context, not a fake
whole-server implementation. With strict floating-point settings and sanitizers,
it generates 8,723 numeric/timer probes; `tests/classic_spell.rs` compares these
and separately tests stateful execution and failure atomicity. No native-capability,
world-runtime, app or distribution mode is enabled by this component.


## Outdoor solo creature AI control

`src/creature_ai/` is a bounded GPL-3.0-or-later translation/reformulation of
ordinary inherited AI/control and numeric branches from the same pinned commit.
CMaNGOS Project authorship/GPL-2.0-or-later grant and existing notices in `licenses/`
apply; the Rust authority boundary, software square-root implementation, validation
and original synthetic C++ adapter are 2026 Benilla fork contributors.
See [CREATURE-AI.md](CREATURE-AI.md) for exact domains, factory selection, source
quirks, geometry requirements and unsupported runtime integration. It does not
activate a native-world gate or relabel the existing original fixture encounters.

Additional reviewed full-file SHA-256 identities (source Git blob identities
also checked against the pinned repository tree):

- `src/game/AI/BaseAI/UnitAI.cpp`:
  `7d8f1ad603a419489759713a5bc0e2bf03ab677f51b64bcb2333807e17048934`
- `src/game/AI/BaseAI/CreatureAI.cpp`:
  `02430dbc589219c4fe78766ab9c2eda303e94349986c947622deee8ee5c0aa89`
- `src/game/AI/CreatureAISelector.cpp`:
  `ea247e160be0af941cb7802b6c47bbd751edae3e58f42b6c898b7203d30c8099`
- `src/game/AI/EventAI/CreatureEventAI.cpp`:
  `70ffaf2f96d95d33d994908acd92431dd694ae8fbf86506f5051320cdf37837f`
- `src/game/Combat/ThreatManager.cpp`:
  `f3619f175b38b1fa7e3ece0cf662dc7804cf1d6f70756e31225242c833002460`
- `src/game/Combat/CombatManager.cpp`:
  `651914417bad072c1ace5b9b11172f3e17856e6ea48bbfba6f6dee5b7ff315e6`
- `src/game/Combat/CombatManager.h`:
  `7b953eb0c24b3b8698717ca0ccb041ca11c2024b1cd89acde07e40c96c22f393`
- `src/game/Entities/Object.cpp`:
  `410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b`
- `src/game/Entities/Creature.cpp`:
  `27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302`
- `src/game/AI/EventAI/CreatureEventAIMgr.cpp` (event applicability audit):
  `18b8da430717f4979586ca28ca8e22c54bf8416b496ba01ae2c6b338b0feea11`
- `src/game/MotionGenerators/TargetedMovementGenerator.cpp`:
  `eaa569e520a7b7a37d3011822e0381d41ab49b9f2dc6ffa1e43d6344bc14ff15`
- `src/game/MotionGenerators/HomeMovementGenerator.cpp`:
  `d225dd76bd02a33fcfdd17dc0d8a2a953f60f99349f59224e20ef678dbb144ac`
- `src/game/MotionGenerators/PathFinder.cpp`:
  `0fc2684275621af341e104d7593c7f66e5baa6251e088548deb5c547625294ef`

Previously recorded Unit.cpp/Unit.h identities cover the reviewed aggro/melee/
combat/death callers. Creature.cpp supplies the audited normal corpse/respawn
branches. Movement source was inspected to establish the external adapter contract,
not ported as a complete pathfinder, spline or motion generator. The existing
pinned software arithmetic dependency/grant above continues to apply.

`tools/generate_creature_ai_fixture.py` extracts source bodies from hash-matched
Unit.cpp, Object.cpp and ThreatManager.cpp into the independent oracle adapter.
Its 52,394-row numeric fixture SHA-256 is
`44aad873e287900435c2c5a91cf36af85a905589cc1c21ccd098a2d0f1d8caa7`.
No native database rows, game assets, EventAI rows or Playerbots code are bundled.

## Mage current skills, crit and resource updates

`src/mage_resource.rs` translates bounded current-skill/rank, mage stat-derived
crit, base magic miss and mana regeneration/update paths from the same pinned
CMaNGOS revision. The existing CMaNGOS Project attribution, GPL-2.0-or-later
source grant and preserved notices apply; 2026 Benilla fork translations and
synthetic oracle adapters are GPL-3.0-or-later. No native DBC/SQL data or assets
are included. [MAGE-RESOURCES.md](MAGE-RESOURCES.md) defines authoritative input
requirements, source branch exclusions, numeric bounds and scheduling contracts.

Already recorded `Player.cpp`, `Player.h`, `Unit.cpp`, `Unit.h`, `StatSystem.cpp`,
`Spell.cpp`, `SpellMgr.cpp` and `SpellMgr.h` hashes cover this work. The oracle
regenerator explicitly verifies every one before using its source. It extracts
unchanged methods/named update blocks and tests 2,082 synthetic numeric/state
vectors with strict floating-point settings and sanitizers. Source `GetStat`
uint32-to-float conversion, `GetSkillValue` bonuses, DBC insertion order and
`Spell::TakePower` exclusions are preserved caller dependencies. The mana spirit
formula's unsuffixed 12.5 uses software binary64 before binary32 assignment;
other source operations use software binary32 through the existing pinned
APFloat dependency. This module does not change native/runtime admission gates.

The upstream spell-crit table explicitly says its vanilla coefficients
`MUST BE CHECKED`. Tests establish pinned-CMaNGOS parity only and must not be
represented as independently verified retail Classic crit coefficients.

## Fresh mage initialization

`starter_init` is a bounded GPL-3.0-or-later source translation from the same
pinned CMaNGOS revision. See [STARTER-INIT.md](STARTER-INIT.md) for source hashes,
explicit raw-row dependencies, deferred stat application, supported effects,
synthetic extracted-source oracles, and the bounded fresh-login replay versus
arbitrary persistence/login boundary. It ships no native character, spell, skill,
or item rows.

## Ordinary creature melee and initialization

`npc_melee` and `creature_init` are bounded GPL-3.0-or-later translations
from the same pinned CMaNGOS revision. Their exact source boundaries, numeric
policies, synthetic differential probes and unsupported branches are recorded in
[NPC-MELEE.md](NPC-MELEE.md) and [CREATURE-INIT.md](CREATURE-INIT.md).
No creature templates, spawn rows or native world data are distributed.
The defense catch-up branch uses a versioned 294-entry source-derived binary64
log table plus software arithmetic. Its explicit platform-math canonicalization,
exhaustive final-f32/strict-RNG verification, exact source excerpts and remaining
host obligations are documented in [NPC-MELEE.md](NPC-MELEE.md). The canonical
log table is not a claim of universal native-libm identity.

## Player death lifecycle

`player_death` translates the bounded ordered death/corpse lifecycle from the
same pinned revision. [PLAYER-DEATH.md](PLAYER-DEATH.md) records provenance,
source-derived callbacks, fixed-step restore constraints and excluded graveyard
and ghost behavior. It does not independently execute runtime callbacks.

## Ordinary quest lifecycle

`src/quest_lifecycle/` is a GPL-3.0-or-later Rust translation from the same pinned
CMaNGOS Classic revision and GPL-2.0-or-later upstream grant. Existing upstream
authorship, copyright and license notices in `licenses/` apply. Rust translation,
adapter and original algorithm-probe changes are by 2026 Benilla fork contributors.
No Blizzard assets, native quest rows, quest text or world database are included.
See [QUEST-LIFECYCLE.md](QUEST-LIFECYCLE.md) for precise admission, transaction-host
obligations, source order, tests and exclusions. This does not enable native quests
in the authored fixture or establish a real-content gameplay result.

Reviewed and oracle-hash-checked files at commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/game/Entities/Player.cpp`: `d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e`;
  CanTakeQuest/CanAddQuest/CanCompleteQuest/CanRewardQuest, AddQuest,
  CompleteQuest/IncompleteQuest/RewardQuest, status/level/class/race and signed
  prerequisite/chain predicates, item adjustment/add/remove, kill/talk credit,
  MoneyChanged, and reviewed future GiveXP/GiveLevel/inventory dependencies
- `src/game/Entities/Player.h`: `acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb`;
  ordered ModifyMoney clamp and money bound
- `src/game/Quests/QuestDef.cpp`: `e4f868e95421c2be5b90bd046178b6f4413dc5203e945cf5735b9bac26104cfb`;
  field loading and numeric reward methods reused through `quest`
- `src/game/Quests/QuestDef.h`: `a03821312ade7e0e30e620bf37d68e32899d65494e37887c9053eee50ebfa66a`;
  status/flags, objective arrays and relation accessors
- `src/game/Globals/ObjectMgr.cpp`: `dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b`;
  signed reverse NextQuestId, PrevQuestId and previous-chain construction,
  combined creature-objective KILL_OR_CAST/SPEAKTO flags

The oracle compiles unchanged extracted bodies/loader fragment with explicit
bounded test adapters. Its synthetic XP recorder is not a GiveXP/GiveLevel port,
and its source/choice/reward item branches are deliberately unreachable.


## Bounded player progression

`src/player_progression/` translates the pinned CMaNGOS Classic GiveXP/GiveLevel
path for the explicit solo mage domain described in [PLAYER-PROGRESSION.md](PLAYER-PROGRESSION.md).
The same upstream GPL-2.0-or-later grant, authors and retained license notices
apply; this derivative is GPL-3.0-or-later. Its extracted-source oracle covers
packet/state/callback order and numeric limits. Missing actual level rows,
unsupported skill/spell hooks, and non-bit-exact armor restoration reject;
no source database rows or client assets are included. Native skill/spell host
and stat/equipment recipe rebasing remain separate runtime integration requirements.

## Bounded solo-creature kill experience

`src/player_progression/kill_xp.rs` adds the pinned source binary32 `XP::Gain`,
victim-aware `GiveXP` and rest arithmetic described in [KILL-XP.md](KILL-XP.md).
It shares the existing level reducer and requires explicit death/tap/side-effect
and fresh-online continuation evidence from the owning runtime. Its 29,995-row
synthetic extracted-source oracle covers numeric and callback order. The module
does not itself implement a native world adapter, resting-area/offline clocks,
honor/reputation rewards, group XP, or production capability admission. The same
upstream GPL-2.0-or-later grant and retained notices apply; this translation and
its synthetic oracle are GPL-3.0-or-later.

## Ordinary backpack inventory

`src/inventory.rs` translates bounded ordinary backpack planning, creation,
cloning, merging and carried requirement removal from the same pinned CMaNGOS
Classic revision. Existing CMaNGOS Project authorship, GPL-2.0-or-later grant and
preserved notices apply; the Rust translation and synthetic source-oracle adapter
are GPL-3.0-or-later, by 2026 Benilla fork contributors. No item/database rows or
client assets are distributed. [INVENTORY.md](INVENTORY.md) describes exact domain,
source order, callback/transaction obligations and independent fixture coverage.

The generator checks full-file SHA-256 identities for the already recorded
`Player.cpp`, `Player.h` and `ObjectMgr.cpp`, plus:

- `src/game/Entities/Item.cpp`:
  `5e401746527531beb521d3eeaa6bc0b55fceaa0d4aef1e780d4f429a545f72e4`;
  `Create`, `CreateItem`, `CloneItem`, `CanBeMergedPartlyWith`
- `src/game/Entities/Item.h`:
  `a884fd9b01da289e3fd556979a6070091f4588400c29b84851d5e88e193bf414`;
  exact native `InventoryResult` constants
- `src/game/Entities/ItemPrototype.h`:
  `64768cb942d73c17ce00fe76f953a3d7ca3e5e617a5f4c798cb5d259bf931756`;
  template field types, stack limits, item classes, binding and flags

Player source supplies full planning/storage methods, unique-count checks and
position membership. Its header's extracted `ItemPosCount` retains the exact
uint8 count and constructor. ObjectMgr's loader resolves Stackable into 1–255;
the bounded interface requires already-normalized values rather than silently
changing native inputs. `tools/generate_inventory_fixture.py` extracts these
unchanged bodies/declarations into `tools/inventory_oracle.cpp`, then executes
1,577 planning and 515 creation/callback/metadata cases under UBSan. The source
oracle uses authored deterministic object adapters and inputs, not a second Rust
translation. No complete server or native-world capability is asserted.

## Positive admin item grants and carried empty Bags

`src/inventory/grants.rs` translates the positive part of
`ChatHandler::HandleAddItemCommand`, carried `Bag::Create` initialization and exact
self-target destination unbinding at the same pinned revision. The existing
CMaNGOS contributors' copyright, GPL-2.0-or-later grant and disclaimer apply;
the Rust translation and authored adapter are GPL-3.0-or-later.

Additional complete-file SHA-256 pins in `generate_inventory_fixture.py`:

- `src/game/Entities/Bag.cpp`:
  `d25b146210b4d13e880be6972cdc574fd72e91eeab5580870f4ee583fcaf6e24`
- `src/game/Entities/Bag.h`:
  `936351212bfa52621d36eb3d1f38e494fc0b4ea3a3e7740fb344c3fb252a6c15`
- `src/game/Chat/Level3.cpp`:
  `2dc12faad03e5b027c4e41566aa7b88f4109c75a8111e14bcecee35633d09bf6`
- `src/game/AI/ScriptDevAI/ScriptDevAIMgr.cpp`:
  `ad6a4113dc1f31aa7059a1167b1409ffd1cfc8c8ebf2b2f73907b8598deb3592`

The oracle extracts unchanged positive-handler code, BagFamily/InventoryType/
EnchantmentSlot enums, NewItemOrBag, Bag::Create, CanStoreNewItem,
GenerateItemRandomPropertyId, AddEnchantmentDurations, AddItemDurations and
OnItemLoot in addition to the existing creation/storage methods. Author-created
fixtures cover 1,024 grants and 216 direct creation boundaries under warnings as
errors and UBSan. Unsupported script, random-property, duration and keyring
routes abort. Bag dispatch is inventory type, not item class, and slots above 36
fail in source creation. The Rust gate checks such a row before starting a grant.

## Ordinary corpse loot, timer callbacks and quest item rewards (reconstruction)

The 2026-10-14 Rust inventory/loot/runtime/client integration is a new reconstruction;
the original patch was unavailable. None of its old test claims are inherited.
CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` remains the source.
CMaNGOS contributors retain their copyright and GPL-2.0-or-later notice, without
warranty. These translations and authored test adapters are GPL-3.0-or-later.
The fixtures contain synthetic inputs and results, no client assets or private DB.

Additional complete-file SHA-256 witnesses:

- `src/game/Loot/LootMgr.cpp`: `7ffc34395011773310f944cfd7f1c59c23ae3043d0c8634d7665c0bbb5752cce`
- `src/game/Loot/LootMgr.h`: `932f10d264919235639bb78ad726988f79f0ab4f3bfa3c144b885e6e40f17a20`
- `src/game/Loot/LootHandler.cpp`: `1bd49276ce292e5991bf7a4f4fd3ac572d00b5230e229c3dd2bcb98c7c686232`
- `src/game/Quests/QuestHandler.cpp`: `00844a75c11d7532475eb8ae2ed9118a249bf51aa07b06ce4ad3979e80c1b768`
- `src/shared/Util/Util.h`: `1f53eafa77ab89935c905f331d9ee7e5e2fabf1fca7f812e2e913f19243c5878`
- `src/shared/Util/Util.cpp`: `f408c8a466a183cabdf96b0f7c18907b8cbb2144d73b0b2e8b81fdf421b43133`
- `src/game/Entities/Creature.cpp`: `27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302`
- `src/game/Entities/Player.cpp`: `d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e`

`tools/generate_loot_fixture.py` extracts unchanged Roll (LootMgr.cpp 374–391),
LootItem construction (397–437), AddItem (930–941), GenerateMoneyLoot (1173–1185),
corpse generation (1711–1731), group and template processing (2397–2477,
2504–2544, 2606–2670), and Util's source float chance comparison. Its 1,536 UBSan
vectors substitute explicit distribution outcomes/permutations. They do not prove
C++ MT engine words or a particular standard library's shuffle implementation.

`tools/generate_loot_timer_fixture.py` extracts Creature's InspectingLoot (2805),
ReduceCorpseDecayTimer (2818) and SetLootStatus (2831) for 800 UBSan vectors.
The runtime additionally preserves JUST_DIED's later full expiry assignment and
source no-op repeated LOOTED transitions, tested at same-tick removal boundaries.

`tools/generate_quest_reward_items_fixture.py` extracts Player's CanRewardQuest
(12378, 12414), RewardQuest (12611–12766), ItemAddedQuestCheck (13444) and removed
item callback, combined with exact inventory planning/creation. Its 1,024 vectors
cover 477 refusals, 1,049 grants and eight later skipped grants in seven cases.
The backpack-only DestroyItemCount traversal adapter is authored; callback bodies
and storage planning/creation are extracted. Do not claim the complete server
DestroyItemCount implementation is executed by that oracle.

Runtime/client packet order is checked against LootMgr's ShowContentTo
(1467–1515), SendItem (1999–2090), SendGold (2257 onward) and Release (1234–1463):
Release first unregisters its caller, so a later SendReleaseForAll cannot emit a
solo response to that caller. Explicit release, missing-slot error then release,
and money-last release therefore differ from item-last automatic SendReleaseForAll.
LootHandler 169–178 resolves an explicit release GUID independently of the current
player loot GUID, so an already queued explicit release after item-last still
performs the first LOOTED transition; repeated LOOTED is a no-op.
An exhausted original nonempty item list fails CanLoot on reopen and receives
SendReleaseFor before registration/InspectingLoot. An originally empty item list
may still open an empty window. QuestHandler 347–381 checks choice and interaction,
then calls CanRewardQuest; any reward refusal reopens OfferReward, after an item
error only when the failed source check sends one. Tests distinguish state changes
from response packets and durable publication from execution.

Source StoreNewItem reserves its original ID before ItemAddedQuestCheck. Ordinary
loot uses this callback boundary directly. Reward reduction stages the quest
callback before invoking its cloned inventory storage host: under the admitted
host the callback cannot observe instance/allocator IDs or creation hooks, and any
allocation failure discards the whole tick, making the ordering observationally
equivalent. Future observable allocation/creation callbacks require explicit
allocator staging before expanding that domain.

## Source-native reputation

`src/reputation/` ports faction-slot matching, initialization, ranks, flags,
ordered spillover, reputation gains and quest reputation rewards from the same
pinned CMaNGOS Classic commit. GPL-2.0-or-later source authorship and the existing
upstream notices apply; this translation and authored harness are GPL-3.0-or-later.
See [REPUTATION.md](REPUTATION.md) for exact files/hashes, typed input closure,
software-binary32 rules, synchronous transactional callbacks and exclusions.
`tools/generate_reputation_fixture.py` extracts the actual pinned C++ methods and
produces 10,496 original source-oracle cases; no database or asset rows are included.
## Ordinary creature health regeneration

`src/creature_regeneration.rs` derives from CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, Creature::RegenerateAll,
Creature::RegenerateHealth, Creature::IsRegeneratingHealth and Unit::ModifyHealth.
CMaNGOS Project contributors; source GPL-2.0-or-later, translation GPL-3.0-or-later.
See [the bounded contract and exact-source oracle](CREATURE-REGENERATION.md).

The timed-evade periodic branch additionally derives from Unit::DealDamageMods
and Aura::PeriodicTick at the same revision. The exact-method/contiguous-block
oracle records prior absorb accumulation, signed resistance ordering and skipped
AI callbacks; see [the damage-order contract](CREATURE-REGENERATION.md#timed-evade-damage-ordering).

## Source NPC spline/chase/home motion

`creature_motion` is a bounded GPL-3.0-or-later translation from the same pinned
CMaNGOS revision and GPL-2.0-or-later source grant. It adds source linear-spline
progression, ordinary melee chase/backpedal, short Home motion, wounded speed and
AI-notify timing. [CREATURE-MOTION.md](CREATURE-MOTION.md) lists exact branches,
query boundaries, numeric restrictions and missing full-Classic mechanisms.

Additional full-file SHA-256 identities:

- `MotionGenerators/TargetedMovementGenerator.cpp`: `eaa569e520a7b7a37d3011822e0381d41ab49b9f2dc6ffa1e43d6344bc14ff15`
- `MotionGenerators/HomeMovementGenerator.cpp`: `d225dd76bd02a33fcfdd17dc0d8a2a953f60f99349f59224e20ef678dbb144ac`
- `MotionGenerators/PathFinder.cpp`: `0fc2684275621af341e104d7593c7f66e5baa6251e088548deb5c547625294ef`
- `Movement/MoveSpline.cpp`: `fcd3a74dc4f8b49a9ff11c268a550815fd4a41c755cfc66c27530e4b73397c08`
- `Movement/MoveSplineInit.cpp`: `cba7b5346dba789a118193f14fd0cc3d9d7387b1e10c9171dc2b58468fcc89cb`
- `Movement/spline.cpp`: `7e8340b710694f360e4582c6c509c133b7a360edbfcb6673e76dcfad71e27c70`
- `Entities/Creature.cpp`: `27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302`
- `dep/g3dlite/G3D/Vector3.h`: `75b8fa263ade1337c4a06d5e6918aa0a7d28bcb107ed405487e62235838125e2`

Other Unit/Object source identities are covered by existing notices. The C++
motion probe is a separately written source-expression harness, not an extracted
full server build. No owner PathFinder, client data, Classic-DB rows or production
admission is supplied by this module. Authored route fixtures identify themselves
as synthetic; actual source-compatible geometry and canonical trig remain required.

## Per-Unit EventProcessor queue

`src/event_processor/` translates the pinned Classic
`src/framework/Utilities/EventProcessor.{cpp,h}` into bounded owned event handles,
source-ordered dispatch and transactional snapshots. Full-file hashes, original
oracle source notices, supported lifetimes, hard resource limits and host-side
callback/teardown obligations are in [EVENT-PROCESSOR.md](EVENT-PROCESSOR.md).
The source GPL-2.0-or-later grant and this crate's GPL-3.0-or-later translation
license apply. Exact unmodified source files are retained under
`tools/event_processor_source/`; the shim, driver, synthetic commands and tests
are authored GPL-3.0-or-later work. This queue alone does not implement the
source SpellEvent scheduler or establish runtime notification/spell integration.

## Portable current-pose facing profile

[`facing`](FACING.md) ports CMaNGOS `WorldObject::GetAngleAt`, `HasInArcAt`
and `MapManager::NormalizeOrientation` from the same pinned Classic source.
Its selected glibc 2.41/CORE-MATH atan2f algorithm has a separately preserved
MIT grant, not the VMap trig port's LGPL grant. Arithmetic uses software
IEEE RNE including explicit source FMA; raw signed zeros/subnormals and the
stricter runtime storage domain are documented separately. Full pinned hashes,
licenses and independent source-extracted boundary/digest evidence are linked
from the contract. This adds no content or production admission.

## Bounded player decrease-speed arithmetic

`player_slow` ports the living, unmounted, unmodified player branch of
`Unit::UpdateSpeed`, `Unit::GetMaxNegativeAuraModifier`, `Unit::GetSpeed`,
`Unit::SetSpeedRate`'s floor and `Aura::HandleAuraModDecreaseSpeed` at the same
pinned CMaNGOS Classic revision. [PLAYER-SLOW.md](PLAYER-SLOW.md) documents the
exact source files/hashes, software-binary32 operation order, required host
exclusions and source-extracted oracle. No spell1604 amount/duration or game rows
are supplied. The runtime's separate source/host catalog and private fixture-only
effect seam do not complete NPC triggered casting or rear-melee admission.
