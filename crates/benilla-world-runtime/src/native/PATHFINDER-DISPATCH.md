# Chase PathFinder constructor dispatch

Pinned reference: CMaNGOS `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`TargetedMovementGenerator.cpp:482–500`, `PathFinder.cpp:37–106`, and
`Map::Create`/`MMapManager::loadMapInstance`. Existing source/license notices apply.
The source map-instance query is loaded before movement dispatch. Chase's lazy
`i_path` construction follows forced spline relocation and precedes both the
straight gate and calculation. Construction does not depend on calculation success.

## Owned modeled boundary

`MotionState::dispatch` emits `PathfinderDispatch` on the existing Route request.
Its construction binding is the first actual dispatch's monotonic `QueryBinding`;
its dispatch binding is the current request. Both bind creature/map/content/geometry
and exact simulation time. `ChaseSnapshot.pathfinder_construction` retains that
identity independently of polygon cache state. Same-generator retargeting and
NoPath retain it; replacement/death discard it. A proximity-skipped initial Chase
emits no constructor. Its first later Replan or Backpedal dispatch constructs it.
Home's separate stack-local PathFinder constructor remains unsupported by this
seam and carries explicit null; it never borrows Chase's retained identity.

The native adapter privately mints `PathfinderDispatchReceipt` from that actual
staged request, actual world/tick, current NPC generation, profile, capability owner and the existing
selected configuration witness. It binds the entire Route request, including
forced-relocation origin and prior cache. The full AuthorityRequest validator also
rejects edits to the outer world/tick/profile/authority/geometry/NPC binding.
The provider sees this event before it
runs the straight gate. No authority success fact or native query allocation is
invented. Profiles without selected configuration retain explicit absent evidence
and synthetic fixture routing remains labeled `SyntheticFixture`.

## Loaded ownership and rollback

The receipt alone neither allocates nor owns a native query. A consumer must have
already loaded the actual source map-instance query and retain it before the gate.
Cloned live receipts share one atomic constructor claim, preventing two providers
from consuming the same event for equal-byte reloaded queries. A retained-use
receipt cannot claim a new query and requires the provider's existing lease.
An identical first event can reuse its provider's existing lease. After an outer
transaction failure a regenerated first event has a fresh seal: the provider must
claim it too while retaining or discarding its original dependency consistently.
`same_live_dispatch` distinguishes repeated use of the same seal from a regenerated
proposal with identical transcript bytes. Neither event nor configuration hashes
can replace this loaded ownership. The narrow initial OwnerPathFinder cache
contract accepts a typed Complete NORMAL claim only with the first dispatch and
revision one; actual source/query/geometry proof remains the app provider's duty.
Production native session admission stays closed.

The mutable simulation identity is staged with motion/world state. An authority
error or later failure inside `step_batch` discards that staged identity. Once
`step_batch` succeeds, the identity is committed in memory; later projection or
checkpoint failures do not roll it back or permit replaying its inputs. Retrying
an uncommitted frame regenerates identical evidence with a fresh live claim. A
provider's dependency-only lease is not itself committed world state or a
successful Route calculation.

## Persistence contract

Native format **33**, `benilla-native-complete-source-visitors-v33`, and
motion policy `cmangos-8ec338a1-ground-linear-motion-first-owned-cache-v3` bind
the first source-cache contract and constructor lifecycle. Profile/state/record/
checkpoint/melee digest domains are v33. Native32 and earlier are rejected;
no migration or serde default invents evidence.
MotionSnapshot also requires its exact policy, including Idle snapshots.
The new simulation snapshot/request options and native Route option are required
fields, even when null. A Chase snapshot with cache or a spline from another
constructor is rejected.

Serialized receipts preserve all evidence for exact replay, but their manual
deserializer restores no live seal. Replay re-executes the source dispatch and
compares every metadata field; it cannot rehydrate native queries from saved
receipts or hashes. First source-cache typed claims may replay from recorded facts;
loaded source-cache checkpoint reopen remains rejected. Saved-session open also
checks the final state after journal replay, so a cache-free base cannot bypass
the reopen guard. Historical Route facts alone do not reject a world whose owner
cache was discarded by the modeled lifecycle. Fixture checkpoint restore remains
explicitly synthetic.
The `OriginalFixture` world and its v2 save format remain unchanged.
