# Bounded NPC-owned triggered spells

CMaNGOS Classic revision `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
GPL-2.0-or-later, Copyright CMaNGOS Project contributors. The Rust host is
GPL-3.0-or-later. No game rows or assets are supplied.

## Supported source contract

The internal host models an ordinary living, uncontrolled NPC's exact
`TRIGGERED_OLD_TRIGGERED` invocation on the current solo player. The only
entry is the explicit synthetic fixture feature. There is no player/network
ApplyAura or NPC-cast command, and `NativeCapability` remains uninhabited.

Full effective spell words, linked rows and the existing
[player slow source closure](src/native/PLAYER-SLOW.md) determine the amount,
duration and holder. The additional host requires damage class NONE (word164),
zero cast time and speed (word37), and SpellVisual0 (word115).
Spell.cpp:7097 therefore suppresses SpellGo for this invisible triggered domain;
SendCastResult also returns for an ordinary nonpet NPC. Word38 is the unused modalNextSpell field.
Only authored IDs61004/61005 are executable here. Spell1604 and the actual
rear-melee trigger remain closed; the host does not invent stock values.

Pinned source anchors:

- Spell.cpp:2997–3100: SpellStart allocates and enqueues its SpellEvent on the
  true caster Unit at queue clock+1 before PreCastCheck. Prepare's instant
  triggered branch calls cast(true) synchronously without a current generic
  slot, GCD or cast bar. OLD_TRIGGERED alone neither ignores hit calculation
  nor disables procs
- Spell.cpp:4676–5158: strict target taxi, untargetable, Spirit of Redemption,
  aura stacking, CanAttackSpell and location checks retain source order. The
  source row disables other morphology-dependent checks. Same-ID/same-caster
  amount is immutable; distinct admitted IDs stack, and cross-caster same-ID
  comparisons remain unsupported
- SpellTargets.cpp:39 and Spell.cpp:6936–7093: target6 is HARMFUL/CHAIN. The
  sole explicit target has no range or LOS gate in this triggered path.
  Player GM invisibility, harmful GM and untargetable checks can filter it.
  Empty target selection still finishes successfully, independently of a
  strict CheckCast rejection
- Spell.cpp:895–951 and Unit.cpp:2900: effect immunity changes effectHitMask,
  while SpellHitResult receives the original effectMask. Evade and whole-spell
  immunity remain distinct outcomes. SpellMgr.h:1392 makes reflection false
  for damage class NONE, and its hit dispatch performs no avoidance RNG.
  There is no IGNORE_HIT flag or assumed successful hit
- Spell.cpp:1163–1517: successful/unsuccessful hostile-hit callbacks precede
  holder installation; EVADE skips them. An effect-immune hit can still run
  the successful hostile callback without applying an aura
- Unit.cpp:6006 and CombatManager.cpp:184: actual CanAttack and caster-detected-
  by-target predicates are distinct from CanAttackSpell. If combat callbacks
  are reachable, both actors must already be in the represented combat, with
  the same NPC victim/reference, standing player and no current unsupported
  casts. AddThreat(0)/already-in-combat wrappers are no-ops in that domain.
  TriggerCombatTimer(player) does not refresh NPC pursuit, because an ordinary
  player cannot own a threat list. New combat entry and assisting spells reject
- Spell.cpp:7149–7243: a finished/deletable event is destroyed at its scheduled
  callback. The bounded instant spell has no current or executing reference
  after synchronous return. Both strict failures and successful empty/hit
  casts retain their finished objects until that callback

## Current-source evidence

Every stage request binds the staged state witness, current caster generation,
player identity, source-derived recipe digest, positions, levels and health,
and the NPC's actual queue plus just-enqueued lifetime. Replies must echo it
exactly. Unknown predicates are errors; unknown or present excluded hooks reject
and roll back the entire frame.

The host explicitly requires complete current evidence excluding spell magnets,
targeting overrides, cast/check/start scripts, cast-end and hit proc sources,
NPC SpellHitTarget and player SpellHit hooks, playerbot and target pet callbacks,
hostile-action-interrupted holders, stand-state transitions, map encounter
credit hooks, nested casts, outer proc/delayed holder state, and unsupported
current noncombat spells. These are source-dependency receipts, not proof of a
raw-input producer. The authored fixture supplies explicit assumptions only.
The slow row's empty proc table by itself is insufficient for this closure.

## Ownership and durability

NPC notification and spell events share one EventProcessor. Event IDs are
independent of the per-NPC cast allocator. Starts before that Unit update can
be cleaned up in the same frame; starts after it remain queued at clock+1.
Multiple starts preserve insertion order. Same-spawn recasts after ordinary
respawn compare the persistent source Unit identity for stacking, while new
entries still require the current generation. NPC generation and world/player
identity are bound, and stale-generation triggers reject.

Native format 30 binds both allocators, terminal lifetimes, source recipe,
start map/Unit times, fixture schedule position and actual owning event.
Current-tick successful lifetimes retain the allocated holder ID, including
replaced holders, and validate the final holder/allocator lineage.
Restore rejects pending targeting objects, detached events, forged ownership,
deadlines, regressed event allocation, reordered retained starts, impossible
holder IDs, contradictory terminal/holder results and unmatched queue/lifetime pairs.
Preceding formats are rejected without deleting current or previous saves.
These are consistency checks, not authenticated event history.

Whole-frame failure rolls back the queue, holder, RNG, effects and outputs.
Controller save failure retains the already-committed frame, publishes nothing,
and retries storage without rerunning SpellStart. Reopen preserves the finished
object and cleans it exactly once on its due NPC update. Permanent Unit teardown
runs Abort on finished objects; source cancellation is a no-op for them.
Checkpoint suspension does not destroy the Unit.

Runtime and exact headless controller tests cover source-check ordering,
strict failure versus filtering, effect versus whole-spell immunity, missing
closure, generation/witness rejection, mixed dispatch, replay, reopen, save
failure and output-once retry. They do not establish real content admission,
complete NPC casting, rear daze, asset fidelity, live realm behavior, or complete
Classic/P2P gameplay.
