# Source creature movement capabilities

`creature_capabilities` evaluates the pinned CMaNGOS predicates from an explicit
validated raw `InhabitType` and the current u32 movement-flags word:

- `CanWalk`: habitat GROUND bit (1)
- `CanSwim`: habitat WATER bit (2)
- `CanFly`: habitat AIR bit (4), or any current LEVITATING (`0x400`),
  HOVER (`0x40000000`) or CAN_FLY (`0x800000`) bit

WALK_MODE, SWIMMING and FLYING do not confer `CanFly`. Current SWIMMING does not
confer `CanSwim`, and current WALK_MODE does not confer `CanWalk`. These are
capability predicates, not the source's distinct `IsFlying` or `IsSwimming`.

## Source, licensing and independent evidence

CMaNGOS Classic revision `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:
`Entities/Creature.h:403–409,643–646`, `Entities/Object.h:701–737,763` and
`Globals/ObjectMgr.cpp:520–524`. Full source hashes are recorded in
`creature_capabilities::SOURCE_SHA256` and the
[exact-source manifest](tools/creature_capabilities_source/source_manifest.json).
The oracle compiles unchanged hash-guarded excerpts and preserves their original
GPL notices. Source is GPL-2.0-or-later, Copyright CMaNGOS contributors; Rust and
authored host code are GPL-3.0-or-later, Copyright 2026 Benilla fork contributors.
Existing [provenance](PROVENANCE.md) and [GPLv3 text](licenses/GPL-3.0.txt) apply.
No game installation, database rows or runtime content producer is included.
The manifest additionally pins full `Creature.cpp` and `Unit.cpp` hashes used
by the separate runtime ownership review. `--upstream` checks those two files,
but the scalar oracle does not compile or execute their lifecycle mechanisms.

[Oracle documentation](tools/creature_capabilities_source/README.md) specifies
the generator, hash guards, numeric fixture schema and 889 source-predicate
comparisons. Separate source-loader probes retain 262 cases showing invalid raw
habitat repair; the Rust tests reject those same invalid raw values.

## General predicate and narrower runtime domain

`InhabitType::try_from_raw(u32)` accepts exactly 1–7. Zero or larger values
reject, including values the source loader would repair to `INHABIT_ANYWHERE`.
Rejecting invalid raw evidence is this boundary's explicit contract, not a claim
that the CMaNGOS loader itself rejects it. No defaults, fallback habitat or
precomputed input booleans are accepted. Capability fields are private, derived
only by `evaluate`, and serializable for evidence without a deserialization path.

The general predicate accepts every u32 movement-flags word, ignoring unrelated
bits exactly as source does. It does not prove where that word came from or that
the habitat is permitted by a particular runtime profile. In particular, source
AIR can make `CanFly` true without a current flight flag, and current hover or
levitation can make it true for a non-AIR habitat.

Runtime initialization and ownership are separate. The initial bounded runtime
profile admits only explicitly supplied GROUND habitat and source spawn flags
within its supported RUN-only subset, with complete-empty external-feature
evidence. Its capability-relevant movement bits are source-derived within that
domain; this pure module does not invent ground/dry defaults or run spawn,
terrain, aura, hover or transport initialization. Broader predicate test coverage
does not admit water, air, flight, effects, navigation or playable-world content.
