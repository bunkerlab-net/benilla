# Native schema 3: bounded player death

This is a synthetic integration-tested fixed-step policy, not complete Classic map
scheduling or native admission. OriginalFixture state remains separate. Native
schema 2 is rejected rather than reinterpreted. `configure_player_death` requires
actual map/config values and checked callback absence evidence; the raw compiler
must certify those inputs. Runtime-owned disabled subsystems can establish their
own absence when captured rows request none. Missing data cannot become false.

## Exact transaction boundaries

The current loop is commands, player casts/released flights, target periodic
updates, player reactive/death update, NPC receipt-only prelude, then each NPC's
unit timers/movement/AI/melee/CombatManager in spawn order, player combat update,
and mana frame. This is an explicitly versioned deterministic policy, not a claim
that all upstream MapManager container scheduling orders have been reproduced.
The receipt prelude applies already-resolved damage/threat at its event instant;
it advances no NPC clock, attack/pursuit/combat timer, movement, AI, or corpse
lifecycle. A private, non-deserializable single-use token binds the later finish.
A same-frame NPC death remains a corpse until the following creature update.

Unit::CombatStop calls RemoveAllAttackers and deletes outgoing and incoming threat
references. An attacking NPC's AttackStop(false) clears its logical victim while
retaining visible TargetGuid and chase state. The runtime therefore detaches all
NPCs synchronously, including the current melee callback, and emits AttackStop at
that callback boundary. It does not force immediate evade. An NPC updated later
in the same frame selects NoTarget and evades; one already updated waits until
its next AI update. Existing spline progress may precede that selection. No
navigation outcome is invented and no teleport is used. Missing later geometry
rolls back death, equipment, spells, all NPC states, RNG and journal together.

The lethal draw consumes damage then outcome only; no surviving-hit durability,
pushback or defense-skill draws are added. Source ordered death actions set HP
zero, run the existing source death-durability helper, apply each broken item's
stat/resource changes, clear reactives/combat and cancel preparing spells through
execution.cancel. Mana is retained except actual item-stat clamping. Released
flights survive. Initial passive holders are grouped from authoritative per-effect
metadata and preserved; nonzero CasterAuraState or active modifier removal remains
unsupported. Full player owner/character and killer spawn/generation are bound
outside the death module's solo actor tokens. SetInCombatState's alive guard is
preserved after the source post-DealMeleeDamage CanEnterCombat/AttackedBy calls.

## Checkpoints and projection

The checked death snapshot proves first-life fixed-step ordering: JUST_DIED only
on the lethal tick; CORPSE on the next player update; exact whole-second reclaim
expiration and inclusive same-frame subtraction from the 360000 ms timer. Restore
also checks health/phase, killer generation, cleared reactives/combat and detached
NPC state. Released casts retain their checked component state. Journal chaining
is still required to authenticate the claimed death event; a checksum alone is
not proof of history.

The client projection emits durability notification, attack stops, zero HP,
body release-timer/dynamic fields and a root order. Root uses an explicitly supplied
local session order counter, never a world tick or fabricated source counter.
`NativeMovementOrders` must be owned with the controller's other forced movement
orders; failed projection does not consume it. The app/controller must still wire
that shared counter and its acknowledgement handling. Death-body reload projects
root again in the new session's counter domain.

The six-minute boundary commits an explicit release-unavailable event and latches
it once, then later world ticks continue. Manual release has a precise unavailable
rejection. No ghost aura, corpse object, graveyard selection/traversal, reclaim or
resurrection has been implemented. These are the next bounded lifecycle tasks;
this state is not advertised as a fully playable native world. Actual assets,
compiler admission, authoritative geometry and graphical gameplay remain gated.

Pinned source: CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
Unit.cpp Kill/SetDeathState, AttackStop/CombatStop/RemoveAllAttackers/DeleteThreatList,
SetInCombatState (alive guard), and Player.cpp Update/KillPlayer. See the component
PLAYER-DEATH.md for extracted-source oracle provenance and dependency exclusions.

Live player consumers use the [conservative supported-player liveness predicate](PLAYER-LIVENESS.md)
where audited. It preserves first-life and lethal-staging behavior and does not
relax the HP/phase restore relation or enable ghost/release states.
