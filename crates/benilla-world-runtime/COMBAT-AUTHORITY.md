# Source-derived NPC and ordinary melee authority

`Profile::initialize_combat_authority` explicitly selects the bounded current-state
host after source player, interaction and motion initialization. Its immutable,
non-deserializable catalog binds captured creature ExtraFlags and all four
StaticFlags words into the canonical profile. StaticFlags1/3/4 are currently
zero-only. StaticFlags2 admits only no-crushing, no-wounded-slowdown and
ignore-feign-death, with the first two crosschecked against existing melee/motion
settings. Zero StaticFlags4 requires backpedal enabled. The source
`IsNeutralToAll` result is checked against the immutable AI initialization.
The v2 catalog admits exactly ExtraFlags NO_AGGRO_ON_SIGHT (0x2) alongside
the existing force-attacking bit. CreatureAI initialization and EventAI respawn
derive Defensive when that bit is set, otherwise Aggressive; a copied reaction
without the matching raw bit rejects. Passive AI, the separate StaticFlags2
SPAWN_DEFENSIVE bit and initial combat-entry overrides remain unsupported.
The catalog validates its policy tag; old-policy contexts reject even if their
outer checkpoint digests are recomputed. The catalog tag changes the bound profile
identity; native48 separately retains source-owned creature generation state while
preserving the shared RNG mapping. Fanning retains its existing separately captured IsWorldBoss-dependent
motion policy and is not inferred from unrelated static flags. Missing rows,
unknown bits, missing motion, unrepresented control/aura/script context and incomplete faction/reputation
inputs reject. Replacing interaction rows preserves this selection and revalidates
it. Source-shaped synthetic fixtures do not certify raw-content completeness.

## Ownership and source phase

The world transaction remains the only mutable owner. NPC observation takes the
borrowed staged NPC after removal from the scene map. Mainhand hooks additionally
borrow the actual in-progress `SoloCreatureAi`, after source movement, victim
selection and synchronous `SetInFront`; they never reinterpret an earlier NPC
reply as the current entity. No second mutable lifecycle/motion owner is persisted.

The existing faction/reputation reducers derive NPC `CanAttackInCombat` and
`CanAttackOnSight` separately. The former preserves the source friendly-NPC/AT_WAR
override after the complete server-side failure; the latter retains the enemy
requirement and feign/home gates. `CanInitiateAttack` combines captured flags with
current `AggroNotify::CanAggro`. Movement/leeway/refresh state comes from the current
source motion and sequenced player flags. Only source sight calls request spatial
visibility/detectability, accessibility and LOS, through distinct typed queries.
Visitor membership and route queries remain separate. Defensive reaction suppresses
proactive player sight without changing the independent CanInitiateAttack or
CanAttack predicates. Direct attacks can still cause retaliation. ExtraFlags 0x2
refuses being an assistant in CanAssistTo; it does not set NO_CALL_ASSIST or
universally remove assistance calls.

Melee front-angle remains external unless the [facing authority](FACING-AUTHORITY.md)
is selected. `CanDodgeInCombat` requires the victim's `HasInArc(pi, attacker)`
followed by the attacker's `HasInArc(pi, victim)`, with source short circuit.
The NPC's preceding `SetInFront(player)` does not replace either check.
Before dice, the query binds both poses,
retained AI target/phase, action, profile, generation, ordered frame witness and
current source motion. `Unit::HandleDamageDealt`'s victim `CanAttack(dealer)` and both
`CanEnterCombat` decisions are derived after surviving health modification, before
durability/pushback callbacks. They are neither NPC sight nor NPC combat predicates.
Lethal `Unit::Kill` and misses do not add that surviving-damage callback.
`DealMeleeDamage` independently rechecks both `CanEnterCombat` values after
`DealDamage` returns, including miss and lethal outcomes. This later typed
`MeleeCombatEntry` callback never substitutes a relationship or liveness test.

Derived replies are recorded in the same order as external observations. The
external host can verify them but cannot replace facts. Replay regenerates them
from the transaction; late missing spatial evidence or receipt verification failure
rolls back state, input sequence, event queues, dice and journal together. The
profile digest prevents restore under different source flags or host selection.
Legacy authored `NpcFacts`/`MeleeFacts` remain only in the unselected synthetic host.

The witness field `creature_ai_target` names the bounded AI's retained target,
not CMaNGOS `Unit::GetVictim`. The inherited mainhand bridge applies current-killer
victim detachment when the callback returns; CMaNGOS `Kill -> CombatStop ->
RemoveAllAttackers -> AttackStop` clears `m_attacking` synchronously during death.
The new post-return entry predicate is unaffected: `CanEnterCombat` reads neither
victim nor health. This batch does not claim a complete source victim snapshot or
redesign that existing death bridge.

The selected [complete visitor lane](src/native/README.md#selected-complete-source-visitors)
adds an ordinary creature/creature prefix and selected player-directed help. It uses both raw directional
faction relations, not a fabricated player-controlled target. At the pre-unit
visitor boundary, Combat phase plus a supported retained target without
`victim_detached_at_ms` represents the active attack victim: AttackStart may
precede a cached ThreatManager choice, while retained AI target alone survives
player death. The visitor cannot execute inside the mainhand kill callback; the
existing detachment bridge has returned before this check. General NPC-to-NPC attack continuations remain unsupported, with whole-frame
rollback. Selected positive help attacks the resolved player victim. A
Defensive peer returns a derived NonAggressive disposition after the active-victim,
neutrality and CheckForHelp prefixes, but before CanInitiateAttack. In particular,
the selected negative CheckForHelp prefix reads live assistance ownership even
for a Defensive observer. Its selected positive continuation uses current cached
player water and creature capability owners before visibility, helper-to-peer
range/LOS and CanAssistInCombatAgainst; see the
[assistance contract](COMBAT-ASSISTANCE.md#visible-peer-help-prefix).

## Provenance and limits

Mechanisms are derived from CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/game/Entities/Relations.cpp`: `CanAttack`, `CanAttackOnSight`,
  `CanAttackInCombat` and source directional reputation relations
- `src/game/AI/BaseAI/CreatureAI.cpp`: initial Defensive reaction (39–44);
  `AI/EventAI/CreatureEventAI.cpp`: JustRespawned reaction restoration (1386–1393);
  `Entities/Creature.h`: NO_AGGRO_ON_SIGHT (50/640);
  `AI/BaseAI/UnitAI.cpp`: peer CheckForHelp precedes non-Aggressive return (83–87)
- `src/game/Entities/Creature.cpp`: `CanInitiateAttack` (2271), ExtraFlags
  initialization (463), `IsIgnoringFeignDeath` (2962)
- `src/game/Entities/Unit.h`: `CanEnterCombat` (2386)
- `src/game/Entities/Unit.cpp`: `IsNeutralToAll` (6228), `CanCrushInCombat`
  (3123); `src/game/Server/DBCStructure.h`: `FactionTemplateEntry::IsNeutralToAll`
  (441); `Creature.cpp`: all four static flag words (631), wounded slowdown (2983)
- `src/game/MotionGenerators/TargetedMovementGenerator.cpp`: backpedal flag (145)
- `src/game/Entities/Unit.cpp`: `HandleDamageDealt` (1203), after
  `victim->ModifyHealth` and before durability callbacks; `DealMeleeDamage`
  (2158/2219) rechecks combat entry after `DealDamage` returns
- `src/game/AI/BaseAI/UnitAI.cpp`: sight/attack predicate callsites

Upstream is GPL-2.0-or-later, copyright CMaNGOS contributors. Derived Rust is
GPL-3.0-or-later. No game assets or third-party database rows are included.

This still has no production admission, loaded map
visitor/visibility/accessibility service, general auras/control/scripts, full
combat parity or multiplayer semantics. `NativeCapability` remains uninhabited.
The tests exercise authored rows through real runtime, replay, store and app
controller code without Bevy linking; they are not playable-client evidence.

## Format transition

This profile/receipt contract uses native format 48 and
`benilla-native-wander-leg-v48`, with separate v48 profile, state,
record, checkpoint and melee-action/chain hash domains. Native37 and all older
checkpoints and records are explicitly rejected by decode, restore and replay;
there is no implicit migration or reinterpretation of authored facts as source
semantics. OriginalFixture format 2 is unchanged.

The [facing authority](FACING-AUTHORITY.md) binds the current numeric policy.
This transition concerns experimental native simulation checkpoints and replay
records only. It does not change the separate profile/character save format.
Opening an incompatible native save fails without replacing or deleting its
current or previous checkpoint files; no automatic migration is performed.
