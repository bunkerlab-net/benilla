# Selected Home return and default Wander resume

This component follows CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` for the selected dry, non-transport,
same-polygon ordinary Home movement. It composes the existing source-owned
Chase, Home arrival lifecycle and retained default Wander owner. The executable
scene is authored and Aggressive; entry 6's real neutral behavior and actual
Northshire content are not inferred from it. Production native admission remains
closed.

## Source order and ownership

`MotionMaster::Clear(false)` removes Chase while retaining default Wander.
Mutating Home interrupts that retained default again before Home Initialize.
The second `WanderMovementGenerator::Interrupt` delegates to
`AbstractRandomMovementGenerator::Interrupt`, which invokes InterruptMoving, including
its forced spline-position relocation and stop operation. A private source
relocation receipt updates the matching ReturningHome AI position atomically;
it cannot be supplied as an external authority fact or reused for another command.

`HomeMovementGenerator.cpp:40–110` chooses the saved combat-start pose when the
default generator supplies no reset position. The all-zero saved XYZ sentinel
selects respawn instead. The forced calculation constructs a fresh stack-local
PathFinder. Its `HomeLocal` identity equals that single dispatch binding and has
no prior cache or target. Chase and default Wander constructors are never reused
for this operation. Home queries at its source map pose; Launch may correct the
first path vertex from the running spline, and loaded spatial evidence covers
that corrected segment as well.

The selected source call is `calculate(start, destination, true)` with straight
mode false. The native continuation preserves the literal force flag. It applies
the already documented
[BuildPolyPath two-scalar compatibility rule](../../docs/CMANGOS-COLD-DISTANCE-COMPATIBILITY.md)
to the constructor-empty lookup. This is separate from the
[Wander endpoint rule](../../docs/CMANGOS-WANDER-ENDPOINT-COMPATIBILITY.md).
The source NORMAL same-polygon branch returns before the later force-destination
rewrite, so this component does not invent a shortcut to satisfy force=true.
The independent [source observer](../../scripts/native_home_source_oracle/README.md)
retains the exact source intervals, hashes and explicit compatibility patch.

Home arrival comes from completed spline state. Its final orientation, ordinary
addon reload and supported finalization effects precede default Reset. Reset
retains Wander move count, replaces its constructor/cache and expires its timer;
the next ordinary Update can select another random leg around the respawn center.
Combat-start position is not substituted for Wander's respawn center.

## Transactions, replay and limits

The temporary Home terminal is retained through proposal and rollback so an
uncommitted retry can use its exact native allocation. Only a validated outer
commit acknowledgement retires temporary and obsolete constructor ownership.
Current cache-free constructed owners and the suspended default remain live.
Chase retires at Home entry; suspended Wander retains its original loaded owner
until arrival Reset replaces it. Earlier component assertions expected zero
retained owners during Home; that prematurely discarded a still-live default
and has been corrected as an app ownership error. Stable NPC/construction keys
prevent registry compaction from changing pending handles.

A historical Home cache in the spline snapshot is validation evidence, not a
retained native allocation or permission to reconstruct one. Generic loaded
Chase/Wander checkpoint reopening remains closed; the synthetic-only one-current-
Wander exception and its exact native replay proof are described in
[WANDER.md](WANDER.md). Home and suspended Wander are outside that exception.
Replay starts from cache-free genesis and compares the complete transcript; a
saved historical result cannot mint a fresh query lease. Native format 51 and
motion policy `cmangos-8ec338a1-ground-linear-home-wander-v7` reject older
snapshots rather than inventing the required evidence.

The concrete mailbox retains the existing 2 ms invocation slice, original
500 ms absolute lease, cancellation, rollback and commit-ACK rules. No Home-only
time extension or synthetic force flag bypass is introduced. For initialized
owners, source-context width, model selection and height checks have separate
retained phases. Later model/body checks reuse only the original Work's fully
validated immutable initialization receipt, preserving raw API validation and
failure order. The same exact frozen owner proof avoids repeated profile/owner
checks at the original target-water branch; target identity/model/cache checks
and fresh LOS/water observations remain in source order. WanderPrepare uses the
retained live-job continuation rather than restarting raw validation. Current owner
CanSwim is retained in filter 13 for the selected dry habitat, while wet routes,
cross-polygon paths, active allowed-Z normalization, transport/flying branches,
unsupported Home callbacks and complete world-startup loading remain outside
this component. Authored geometry demonstrates composition, not authentic
client assets or production corridor admission.

The generic AI controller also retains its earlier conservative 150-yard guard
on the pre-Home map position. An inward boundary crossing from stale position
outside 150 yards to an interrupted spline position inside can therefore reject
before reaching the corrected motion boundary. Such crossings are excluded from
this runtime composition. The direct motion owner checks distance after the
second interruption, and an outward crossing rejects transactionally. Neither
boundary invents the source far-home despawn lifecycle.
