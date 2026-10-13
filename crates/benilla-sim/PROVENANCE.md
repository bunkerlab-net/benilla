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
