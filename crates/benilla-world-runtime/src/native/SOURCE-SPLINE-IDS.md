# Bounded source-thread spline IDs and first-life forced stop

Native format 68 composes one **fresh isolated logical source thread**, seeded at
1. It does not import or guess a running server's counter or reproduce upstream
MapManager scheduling. Production native entry remains closed. This is a real
bounded runtime/controller path under authored source fixtures, not proof of a
populated player build or rendered movement.

## Exact selection and source closure

The composed owner exists only with all of:

- source-motion policies for every fixed NPC owner;
- validated fresh source player initialization and first-life death policy;
- the existing full raw spell authority and its validated interaction catalog;
- no selected player-slow subsystem (that composition remains outside this slice).

The interaction source validates ordinary independent player control fields,
including `unit_state == 0`. Its external-control mutation exclusions and the
exhaustive admitted initialization/projectile effects leave no player movement
generator, fear, confusion, knockback, charge, taxi or transport producer.
`GroundMovementFlags` separately accepts only `0x13f`, excluding transport,
forced-spline, root, jump and flight flags. These are different facts:
`UNIT_STAT_MOVING` is the generator-state mask, **not** the directional movement
packet flags. A validated fresh player's server spline is finalized and no
admitted producer launches it in this domain.

Profiles outside this intersection keep the prior explicitly partial local-NPC
projection. They do not receive a shared-thread owner or a fabricated player
allocation. Their uncomposed player StopMoving gap remains. Adding a new player
motion/control producer requires revisiting this closure, not extending a list of
caller-supplied absence booleans.

## Live callbacks and wire projection

Pinned CMaNGOS `MoveSplineInit.cpp` has a `static thread_local uint32` counter
seeded at 1, postincremented by successful Launch and Stop. The native NPC motion
component's retained local ordinals remain unchanged: they still prove each
owner's source spline state. A separate bounded owner allocates shared wire IDs
at each actual launch/interruption callback, including forced NPC death stops.
Every allocating `NpcMotion` event retains its own assignment. Multiple events
from one already-updated/detached NPC consume consecutive local ordinals, not a
fabricated assignment from the final snapshot. Completed-frame validation checks
all owners have caught up. NPC respawn retains its allocation count.

At the actual player `DeathAction::StopMoving`, `StopMoving(true)` clears the
source generator moving bits (already absent under the closed owner), clears
only `FORWARD` from the accepted packet flags (`SPLINE_ENABLED` is already
excluded), allocates from the same counter, and emits a real player MonsterMove
stop at the **current staged callback pose**. Backward, strafe, turn and walk bits
are not erased. This occurs before the later CombatStop actions and next-frame
root order. It is not a substitute for root, observer visibility recomputation,
corpse creation, ghost/release, or graveyard recovery.

The retained first-life stop witness binds the real death tick, pose, flags and
player allocation. Dead movement commands already reject before changing pose or
flags. Restore requires player count 0 while alive and exactly 1 after this
first-life death, together with the existing validated lifecycle. A stop does not
create an active player ride. Rejoin allocates nothing: active NPC creation uses
the retained shared ID for the exact current local ordinal; a stopped player does
not replay an old stop packet. Existing death-body root restoration is separate.

The stock player's stop acknowledgement reaches the actual controller. Pinned
`HandleMoveSplineDoneOpcode` parses then does nothing; the composed route accepts
that command as a no-op without inventing ID, pose, finite-float or liveness
checks. Other controller domains retain their prior unsupported behavior.

## Bounded checkpoint proof

There is one fixed sorted endpoint per NPC plus the player, not an ever-growing
allocation journal. For positive count `c_i`, retain last shared ID `d_i`.
Require exact owner set, zero count iff zero endpoint, distinct positive endpoints
in `1..N`, `N = sum(c_i)`, and `next = N+1`. Sort by endpoint; every prefix must
satisfy `sum(c_i) <= d_i`. This is necessary and sufficient for a reachable
interleaving: reserve each final endpoint, then place each owner's preceding
ordered events under the nested deadlines. The prefix inequality is exactly the
available-slot constraint. It proves reachability, **not the actual historical
inter-owner chronology**. Normal TickRecord replay checks actual callback order,
assignments, events and state transitions from the prior checkpoint.

The host supports IDs through `u32::MAX`, followed by a `u64` exhausted sentinel.
Further allocation fails transactionally. Source wrapping is explicitly unsupported;
no reset, skipped ID or shortened movement lifetime is silently substituted.
Checkpoint/step failures roll back allocation, movement, death and publication
with the existing transaction. Projection replays allocations on a private frame
and checks the final owner, rejecting missing/extra/skipped/mismatched receipts.

## Pinned references

CMaNGOS `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/game/Movement/MoveSplineInit.cpp`: counter, Launch and Stop. SHA-256
  `cba7b5346dba789a118193f14fd0cc3d9d7387b1e10c9171dc2b58468fcc89cb`.
- `src/game/Entities/Unit.cpp`: Unit::StopMoving; constructor and death ordering.
  SHA-256 `03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0`.
- `src/game/Entities/Unit.h`: `UNIT_STAT_MOVING` and IsStopped. SHA-256
  `2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1`.
- `src/game/Maps/TaxiHandler.cpp`: HandleMoveSplineDoneOpcode.

No database data, game assets, private capture or native content admission is
introduced. Classic DB supplies no replacement for these executable movement
semantics.
