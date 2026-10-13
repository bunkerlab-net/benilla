# Bounded Classic player/NPC target relationships

`classic_targets` is an engine-independent, deterministic **relationship subcheck**
port from CMaNGOS Classic commit `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.
Its capability ID is `cmangos-classic-solo-player-npc-targets-v1`. It performs no
I/O, mutation, random draws, clock access or floating-point operations.
It is not a native world compiler, full spell validator, combat system or AI.
An `Allowed` result never authorizes a cast/swing by itself.

## Source paths and branch selection

The actual relationship implementation is
[`Entities/Relations.cpp`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Relations.cpp):

- `GetFactionReaction(FactionTemplateEntry*, FactionTemplateEntry*)`, lines
  100–143, supplies the deterministic comparator
- `Unit::GetReactionTo`, lines 197–269, supplies the supported two-direction
  fallback after explicit exclusion of control/PvP/reputation/persuasion paths
- `CanAttack`, lines 351–419, rejects the target flag mask and, with exactly one
  player-controlled side, returns `!IsFriend(target)`. Neutral is attackable;
  the target need not have a hostile reaction toward the player
- `CanAttackNow`, lines 428–447, adds player alive, unmounted and target alive
  checks. Upstream identifies this unified helper as backported from TBC+;
  this port claims parity with that pinned helper, not a newly verified 1.12
  client callsite
- `CanAssist`, lines 458–511, rejects unselectable and nonfriendly targets, then
  requires PvP UI for a player to assist an NPC. A friendly ordinary non-PvP
  NPC is not healable through this branch
- `CanAttackSpell`, lines 1065–1120, calls `CanAttackInCombat`, lines 1172–1196,
  which calls `CanAttackServerside`, lines 1206–1276. In the accepted domain
  (direct harmful spell, no exceptions, ordinary non-player-controlled target),
  these reduce to target alive, target mask, and `!IsFriend(target)`

`CanAssistSpell` simply delegates to `CanAssist` in this revision.
[`Spell.cpp`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Spell.cpp)
`Spell::CheckCast`, especially lines 5045–5095, calls these relation checks for
harmful/helpful unit target filters. It performs many independent validations:
caster liveness at line 4694, mount restrictions at lines 5110–5115, target
conditions, effects, resources, items, range, LOS, auras and more. The exported
spell function deliberately does **not** validate caster liveness/mount: that
matches the relation helper, and its name/documentation prohibit treating it as
`CheckCast`. This module does not translate the target table or any spell effect.

[`Unit.cpp`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp)
`Unit::Attack`, lines 6259–6337, and `UpdateMeleeAttackingState`, lines 651–712,
have further world/liveness/evade, range, facing, timing and attack-side effects.
Those routines are not implemented by this module.

[`DBCStructure.h`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStructure.h)
`FactionTemplateEntry::IsFriendlyTo`/`IsHostileTo`, lines 414–438, are **not**
the comparator called by these `Unit` relationship paths. Their list/mask
precedence differs. Substituting them would silently change the result.

## Shared user-local data contract

The module uses existing `benilla_formats::{FactionTemplate, Reaction}` types;
there is no second native faction-ID table. Adding that dependency does not add
runtime file reads or default `dev` features to the simulation.

`FactionCatalog::template_flags(id)` now retains exact FactionTemplate.dbc column
2 in a private catalog sidecar. Existing `FactionTemplate` struct literals and
the client `reaction_toward` behavior remain unchanged. `FactionCatalog::faction`
returns the raw Faction.dbc identity, including negative reputation indices,
so missing metadata is distinguishable from a known untracked faction.
`FactionCatalog::from_rows` remains a client-fixture constructor: it supplies no
native flags or Faction.dbc evidence, and therefore cannot pass this capability.

`FactionRef::from_catalog` resolves the template, sidecar flags and Faction.dbc
identity. A nonzero faction ID requires a real Faction.dbc row. Faction ID zero
explicitly means no Faction.dbc identity; it is not inferred from a lookup failure.
Tracked reputation, missing identities, missing flags, inconsistent metadata and
nonzero template flags return explicit `TargetUnsupported` variants.

The pinned server comparator enables each explicit list only when slot zero is
nonzero, and then scans all four slots. The current client comparator stops at
the first zero. Both remain intact; a regression test covers `[7, 0, match, 0]`
and `[0, match, 0, 0]`. No real DBC rows are bundled or assumed to have gaps.

## Immutable input and output

- `PlayerNpcInput` borrows a `FactionRef` for each side and holds explicit
  `UnitTargetState { unit_flags, alive, mounted }` for player and NPC
- `RelationContext` requires explicit evidence for `control`, `pvp`,
  `reputation_overrides`, `immunity`, and `special_state`. Each is `Absent`,
  `Present`, or `Unknown`; only verified `Absent` enters the supported profile
- Native flags must identify the player as player-controlled and the ordinary
  NPC as not player-controlled. There is no default constructor for this context
- `player_npc_reactions` returns both directions, or an explicit Unsupported
- `check_player_melee_target`, `check_player_harmful_spell_relationship`, and
  `check_player_assist_relationship` return `Allowed`, `Denied(reason)`, or
  `Unsupported(reason)`. No `bool` conflates an unknown domain with a known result
- `HarmfulSpellTargetInput` requires raw AttributesEx2 and AttributesEx3 plus
  `is_aoe`. Version 1 accepts only zero extended attributes and direct targeting;
  dead-target, restriction-ignore, AoE and every other bit are rejected

Domain validation is deliberately before gameplay denials: missing metadata or
unsupported state never becomes an apparently supported friendly/dead rejection.
For supported melee requests, denial precedence is player dead, player mounted,
NPC dead, target flag mask, friendly reaction, matching the upstream path.

The accepted UnitFlags are exactly the five target-blocking bits (spawning,
not-attackable-1, untargetable, taxi, uninteractible), player-controlled identity,
and swimming/in-combat/looting/sheathe bits that do not alter this subcheck.
Possession/PvP/persuasion/native immunity have explicit rejection reasons;
every other bit is rejected rather than silently erased. This conservatively
rejects some states the upstream generic relationship function can classify.
This is an explicit capability boundary, not claimed parity for rejected inputs.

Version 1 does not support pets, charms, guardians, totems, other players, PvP,
duels, FFA, contested guards, sanctuary overrides, reputation/at-war/forced-rank
paths, persuasion, aura/school/mechanic/script immunities, ghosts, feign death,
evade, temporary factions or scripts/conditions. A native compiler must derive
absence from the actual selected data and session state; a zero exported unit
flag value alone proves none of these exclusions.

## Native runtime/compiler integration contract

`FactionRef::from_catalog` is the adapter for actual loaded native faction data.
The future selected-starter compiler must resolve the player's actual
ChrRaces.dbc faction template and every selected creature's native faction,
then preserve all relevant unit/template flags and contextual classifications in
its field-disposition report. It must not select sample faction IDs or turn
`Unknown` into `Absent`. The borrowed snapshots must be rebuilt from current
session state whenever targeting changes; cached native metadata is not evidence
that charm, immunity or PvP state has stayed absent.

This subcheck alone does **not** satisfy
`creature.native_stats_combat_faction_required` or
`spell.native_effect_target_scaling_rules_required`. Existing native-source
rejections must remain until the compiler and the rest of the selected capability
profile are implemented and verified. Do not wire the fixture runtime's friendly/
hostile booleans into native faction metadata. Original authored runtime rules
remain separate and do not acquire Classic parity through this module.

## Evidence and reproducibility

`tools/generate_target_fixture.py /path/to/pinned/source/files` verifies SHA-256
identities before extracting the original C++ constants and method bodies into
`tools/target_oracle.cpp`. It compiles them with g++ C++17 and warnings denied.
The harness replaces only world objects outside the accepted branch domain; it
uses no Rust code, native database rows, game assets or random input. The methods
are extracted at regeneration time, not copied into the repository.

- 6,912 comparator vectors exhaust combinations of zero/nonzero faction identity,
  each relevant group-mask edge, no match, first/last match, a zero gap, and a
  disabled first slot, including conflicting hostility/friendship precedence
- 24,576 target vectors exhaust three reactions, all 32 source and all 32 target
  blocker-mask combinations, player/NPC liveness and player mount state against
  upstream `CanAttackNow`, `CanAttackSpell` and `CanAssist`
- Original ordered numeric outputs are stored as count + SHA-256 fixtures to
  avoid bulky vector files. Rust reconstructs the full ordered input/output
  streams and checks both count and digest
- Additional explicit Rust assertions cover every UnitFlag/template-flag/
  supplied spell-attribute bit, both directions, all missing metadata,
  `Present`/`Unknown` contexts, no-faction identity, denial precedence and the
  unchanged client/server gap distinction

The C++ oracle only supports the stated domain. Its stubs are not a substitute
for upstream execution with reputation/PvP/control state, and no parity is claimed
there. No graphical/native content or cross-peer play test is implied by these
pure integer/branch tests. The broader repository gates remain separate.
