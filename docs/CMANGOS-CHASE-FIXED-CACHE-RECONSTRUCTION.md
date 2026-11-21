# Selected fixed-cache Chase reconstruction

This is a native calculation equivalence proof for the selected dry NORMAL
same-polygon Chase lineage. It grants no creature constructor, gameplay route,
owner acceptance, checkpoint restore, or production startup authority.

## Source dependency state

Pinned CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, non-PLAYERBOTS configuration:

- `PathFinder.cpp:35–54` constructs 74 zero backing entries, active length zero,
  and the source query/filter state
- `TargetedMovementGenerator.cpp:487–504` updates unfinished spline position,
  constructs the lazy finder before the straight-selection gate, then calculates
- `PathFinder.cpp:122–179` overwrites endpoints and flags before lookup without
  clearing the retained active path
- `PathFinder.cpp:306–383` searches active polygons first, then ordered tail,
  then nearest queries, with the original distance and append comparisons
- `PathFinder.cpp:508–519,988–1020` writes the selected same-polygon NORMAL
  terminal, including slot zero, active length one and two unnormalized points

The cold calculation uses only the named
[zero-distance compatibility rule](CMANGOS-COLD-DISTANCE-COMPATIBILITY.md).
Subsequent calculations use the defined original warm policy. This is distinct
from [Wander reconstruction](CMANGOS-WANDER-ENDPOINT-COMPATIBILITY.md): Wander
clears active length before each calculation, while Chase retains it.

## Exact selected domain

A single genuine source-profile 1,024-node query, immutable map/instance inventory,
configuration and dry owner filter (1/0 or 13/0) owns one constructor lineage.
Every admitted result is NORMAL, active length one, with two exact points and
normalization disabled. Every calculation after the first must preserve the
entire backing vector and active state exactly, including inactive zero entries,
ordered tail, and active polygon slot zero.

An opaque witness authenticates each actual native predecessor. Equal bytes,
sibling proposals, skipped predecessors, or a matching slot do not establish that
lineage. A genuine changed-cache successor leaves this selected capture domain;
a later return to the old polygon cannot erase the intervening transition.
Invalid foreign proposals do not change the accepted witness.

The inert recipe contains the cold operation and, when distinct, the final
operation, with original ordinals and total count. It contains no polygon refs or
loaded handles. One or two replayed calculations do not replace the original
accepted operation count. Counter overflow rejects without changing the witness.
The native-only version-1 wire encoding is at most 111 bytes: a 47-byte header and
one or two 32-byte operations. Decode bounds size/count before allocation and
rejects trailing data, noncanonical flags, invalid coordinates and ordinal shape.
It is independent of the native world checkpoint schema.

Omitted supported operations are identity transitions on the complete state read
by subsequent lookup. Endpoints, flags, points and type are overwritten by the
next successful calculation. Consequently cold plus final sees the same lookup
state and produces the same final semantics. This claim excludes historical
before-observations and does not reconstruct owner clocks, RNG, spline progress,
query IDs, target state, dispatch seals or ACKs.

## Fresh reconstruction and continuation

Reconstruction loads validated input bytes into a new actual source query, follows
constructor/default-query then selection ordering, and reexecutes the retained
inputs under the existing native budgets and shared deadline. It checks the fixed
cache condition again. The opaque result binds the final terminal, retained query,
resumed witness and a fresh attempt challenge. Serialized refs never seed a query.

A future owner bridge must independently bind immutable geometry and constructor
inputs, the original operation count/revision, exact current checkpoint head, and
all terminal semantics. An inert recipe or successful observer callback alone is
not authority. Runtime and app cached-restore gates remain unchanged.

Live continuation may subsequently change the active polygon or append a tail
entry through the existing native API. Such a change is supported live behavior
but leaves this narrow saveable domain. It must not silently restart a constructor
or truncate its history.

## Explicit exclusions and counterexample

A tail length/set comparison is insufficient. On the authored adjacent A/B mesh,
execute A, append B, return to A without an append, then calculate on the shared
edge. The active-first source lookup chooses A. Omitting the no-append A writer
chooses B even though both histories contain the same ordered tail. This is why
Wander's appenders-plus-last-writers recipe is not reused for Chase.

Chase NOPATH is also excluded. Source missing-polygon lookup can mutate backing
before `BuildShortcut` clears active length and preserves two points. Chase can
then perform a second calculation on that mutated finder
(`TargetedMovementGenerator.cpp:538–543`). Provider errors or unsupported native
branches are not substitutes for those source effects. A later proof must model
both attempts and their complete state before admitting such lineages.

## Independent source comparison

The guarded retained-same-polygon C++ observer executes 4,097 original calculations
and compares them with cold-plus-final on separate live source 1,024-node queries
for filters 1 and 13. It also compares later B-append/A-return/shared-edge
continuation and the non-elidable active-flip counterexample. The original
eleven-row retained fixture and source fragment packages are unchanged; additional
comparison rows are separate. Normal and UBSan executions produce identical
comparison bytes. See the [observer contract](../crates/benilla-native-nav/tools/retained_same_poly_README.md).

## Accepted capture and replay ordering

The test-only component route provider stages a separate Chase witness with the actual
accepted cold/final authority requests and original cache revision. It publishes
that optional evidence only after the complete route ACK validates. Rejected
proposals or ACKs cannot advance it; a successfully accepted transition outside
the fixed-cache domain leaves this constructor unsaveable until replacement.
Suspended Wander retains its independent capture. Inspection of this evidence
does not encode a save or grant loaded restore authority.

The native observer reports constructor creation once, then a read-only
pre-selection stage for every retained operation before selection, tile presence
and terminal calculation. The source straight gate belongs before selection on
every calculate, including the retained final input. A rejected or expired gate
callback cannot reach selection. This hook exposes original inputs and retained
selection state, not a new permission to install an owner.
