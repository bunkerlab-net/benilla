# Conservative supported-player liveness

`State::player_alive()` requires positive health and, when the native death
lifecycle is selected, its retained phase to be Alive. The legacy fixture lane
without a death lifecycle retains its existing positive-health predicate.

This is deliberately a **conservative native-host predicate**, not a new port of
upstream `Unit::IsAlive` or `IsDead`. In pinned CMaNGOS
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, `Unit.h` defines IsAlive as ALIVE and
IsDead as DEAD or CORPSE; JUST_DIED is not IsDead. Native lethal staging currently
sets HP zero before installing the enclosing death plan's next phase. Retaining
the health conjunct preserves the former refusal during that callback window.
A lifecycle-only replacement would change existing synchronous behavior.

For every currently admitted first-life state the result is identical to the
previous health predicate. For an inconsistent observation (positive health with
a dead lifecycle, or zero health with an alive lifecycle) the result is false.
Such observations are not new valid states: all existing HP/phase, first-life
clock, killer, aura and checkpoint proofs remain unchanged. No format or policy
identity changes are necessary for this behavior-preserving refactor.

## Consumer/source mapping

Only live eligibility/observation guards use this helper in this change:

| Host consumer | Pinned source role / retained restriction |
| --- | --- |
| `engine::spell_events` threat eligibility | ThreatManager::addThreat rejects dead threat owner/hated unit; surviving projectile damage is not canceled |
| `engine::command` Cast and cast update checks; `engine/spell_authority` | Spell preparation/update caster-liveness operand; source metadata and existing casts still own execution |
| `engine::pose` | Existing bounded living-player movement policy, not ghost movement admission |
| `engine::npc_update`, `engine/motion` observations and post-melee result | Source Unit::UpdateMeleeAttackingState victim liveness and AI current-target observations |
| `engine::enter_player_combat` | Unit::SetInCombatState alive guard, distinct from CanEnterCombat |
| `engine::mana` | Source player resource-update alive operand after spell/aura events |
| `engine/visitors`, `engine/death` visitor observations | GridNotifiersImpl.h source visitor liveness; callback-local scene remains the observed owner |
| `engine/combat_authority` | Sight predicate admission and surviving-damage callback precondition |
| `interactions` current UnitTargetState | Source player/NPC relationship evaluator current liveness; NPC operands unchanged |
| `engine/inventory` | Player::SwapItem explicitly rejects !IsAlive |
| `engine/quests`, `engine/loot`, client `native_quests` | Existing ordinary living-player interaction admission; no spirit-healer/ghost services admitted |
| `player_melee` current ready phase | Existing selected living-player mainhand precondition |

The named branches keep their prior ordering and return/event behavior. A negated
host helper is a refusal in the existing restricted host domain, not a claim that
it equals source IsDead. Historical relationship contexts still retain their
numeric health witnesses; no serialized historical schema is reinterpreted.

Numeric damage/lethality, resource values/clamps, raw witness equality, NPC health,
legacy fixed-outcome damage-loop cutoffs, death/slow-holder consistency checks and
all persistence invariants remain numeric. Other specialized admission/proof
paths are not generalized to future life states by this refactor.

## Verification and still-closed recovery

Tests exercise normal alive/dead first-life states, legacy no-death states, the
HP-zero/old-Alive staging observation, and positive-HP/dead mismatch refusal.
Rehashed positive-HP/dead checkpoints remain rejected. The exact app controller
also exercises actual NPC death, stock cast/movement/refused spirit release,
continued ticks, retained root bookkeeping and reopen without a ghost or HP1.

This prerequisite does not make ghosts representable. Source ghost spell8326,
its speed/water/visibility callbacks, source appearance/display snapshot,
generation-owned corpse registry, graveyard authority, dynamic interaction flags
and repeated-life persistence must all be composed before accepting release.
Actual ghosts remain dead with HP1 in upstream; none is constructed here, and
`ReleaseSpirit` remains explicitly unavailable.
