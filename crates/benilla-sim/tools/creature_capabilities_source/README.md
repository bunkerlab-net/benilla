# Exact-source creature capability oracle

The `.inc` files retain unchanged LF-normalized source excerpts, including each
original GPL notice, from CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `src/game/Entities/Creature.h:403–409,643–646`: habitat constants and
  `Creature::CanWalk`, `CanSwim`, `IsSwimming`, `CanFly`
- `src/game/Entities/Object.h:701–737,763`: movement constants and
  `MovementInfo::HasMovementFlag`
- `src/game/Globals/ObjectMgr.cpp:520–524`: separate invalid-habitat repair

Canonical source:
[CMaNGOS Classic](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
Source is GPL-2.0-or-later, Copyright CMaNGOS contributors; the complete original
notice is in every excerpt. Existing [upstream notices](../../PROVENANCE.md)
and [GPLv3 license](../../licenses/GPL-3.0.txt) apply to this combined GPL-covered
crate. The minimal host, generator and Rust translation are GPL-3.0-or-later,
Copyright 2026 Benilla fork contributors. No proprietary data or database rows
are included.

[source_manifest.json](source_manifest.json) pins full-source, excerpt-body and
retained-file SHA-256 hashes. The Python generator independently pins that
manifest's SHA-256. Retained bytes are exact; only full upstream checkout bytes
are normalized from CRLF to LF before verification. With `--upstream`, the
generator also proves every retained body and notice equals its pinned source.
Normal checks require no upstream checkout or network.

The manifest separately pins complete normalized `Entities/Creature.cpp` and
`Entities/Unit.cpp` hashes reviewed for runtime construction, respawn and
movement-flag ownership. With `--upstream`, those two files are checked as well.
They are not retained excerpts, compiled by this oracle, or lifecycle evidence:
the scalar oracle does not execute initialization, respawn or flag transitions.
The runtime's own policy and tests establish its narrower lifecycle contract.

`oracle.cpp` supplies only small typed storage, virtual base declarations and
diagnostic counting so the unchanged excerpts compile. It does not restate
their predicate expressions. `IsSwimming` is retained with its neighboring
source lines but is not a tested capability or output. The `R` operation runs
the unchanged ObjectMgr repair block separately from `P`, whose habitat must
already be valid. This does not emulate the complete database loader or
`Creature::InitEntry`, terrain, effects, runtime flags, or movement admission.

Fixture schema (space-separated decimal integers after a row kind):

`kind raw_habitat current_flags loaded_habitat can_walk can_swim can_fly repair_logs`

- `P`: directly evaluates a valid habitat and explicit current flags
- `R`: evaluates only after source loader repair, with the original raw habitat
  retained. Rust tests separately require rejection of invalid raw inputs

The 889 predicate cases cross every habitat 1–7 with 127 flag words: zero, all
bits, each of 32 individual bits, each bit's complement, all 64 combinations of
WALK_MODE/LEVITATING/FLYING/SWIMMING/CAN_FLY/HOVER, and the complete unrelated-bit
mask combined with each flight-capability subset. The 262 loader cases include
every raw input 0–256 plus unsigned-width boundary values. All values are
authored numeric probes; this finite corpus is not exhaustive over u32 flags.

From the repository root:

```
python3 crates/benilla-sim/tools/generate_creature_capabilities_fixture.py --check --sanitize --upstream /path/to/pinned/mangos-classic
python3 crates/benilla-sim/tools/test_creature_capabilities_oracle.py
```

Drop `--check` only to regenerate the numeric `.test-fixture`. Missing compilers,
changed source hashes and differing output fail; none silently skip.
