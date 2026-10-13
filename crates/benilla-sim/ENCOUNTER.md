# Deterministic synthetic encounter

`benilla_sim::encounter` is a headless gameplay harness, not the complete CMaNGOS
server port. Its original fixed-damage combat rules demonstrate replicated simulation
around the source-derived XP primitives. All numeric fixture IDs and stats in the
integration tests are invented; they are not vanilla database rows.

## Contract

- Construct `World::new(Rules, Content, Vec<PlayerSpec>)` with the same complete,
  frozen roster and caller-supplied creature definitions on every peer. Peer IDs and
  creature IDs occupy separate namespaces. Construction validates nonzero stats,
  timing and rates, unique IDs, and a roster of 1..=64 members.
- Agree on `SIMULATION_VERSION`, initial `state_hash()` and the exact roster before
  starting. An existing free-join exploration session needs an explicit agreed
  start transition. The module supplies no discovery, transport or start handshake.
- Supply one `InputFrame { tick, peer, commands }` per roster member, including empty
  frames. Within a frame, command vector order is the sender's sequence. A tick is
  that peer's frame sequence; there is no separate out-of-order queue.
- `submit` accepts only the current tick and known peers/targets, at most 16 commands.
  Identical repeated frames return `Duplicate`; conflicting frames are rejected.
- `step` returns `Waiting { missing }` until every frozen member has submitted.
  Waiting neither advances state nor removes a peer, regardless of elapsed time.
- Each committed tick is atomic. Arithmetic overflow returns an error without
  partially changing health/rewards or consuming the pending inputs.
- Order: respawns by stable ID, commands by ascending peer ID and command index,
  player swings by peer ID, then creature swings by entity ID. Player attacks take
  precedence within a tick. Repeated Attack commands do not reset swing timers.
- Attacks use fixed caller-supplied damage. Creatures retaliate against an attacker.
  A kill awards base XP multiplied by `xp_milli` with rational ties-even rounding,
  and optional guaranteed loot. Only the lethal attacker receives the reward;
  this deterministic tie rule is a fixture policy, not CMaNGOS group/tagging logic.
- A dead spawn ends all attacks on it. Respawn occurs after its specified logical
  ticks, increments its generation, and requires a fresh attack command. Deaths
  and rewards include `(entity, generation)` for identifying one spawn's result.
  Players also respawn after their specified logical delay and stop attacking.

## Snapshot and replay

`state_hash()` is SHA-256 over a version-tagged canonical little-endian encoding of
all committed rules, content, roster and mutable state. Maps are serialized in
sorted key order; lengths and optional-value tags are explicit. Pending network
arrivals are excluded, so different arrival order cannot alter the committed hash.
The golden digest test detects accidental encoding changes.

`snapshot()` creates a typed in-memory snapshot only at a committed boundary; it
rejects pending input rather than silently dropping it. `World::from_snapshot`
restores the state. `Snapshot::canonical_bytes` is diagnostic output, not a promised
save-file format; a durable versioned decoder/migration layer remains future work.

`CommittedTick` includes canonical inputs, ordered events and the resulting state
hash. `World::replay(snapshot, log)` recomputes each tick and rejects missing inputs,
changed hashes, or changed events. Callers own log retention; the world does not grow
an unbounded history internally.

## Deliberate limits

No positions, movement, range, collision, pathfinding, randomness, spells, auras,
classes, level-up tables, quest scripts, XP attribution beyond the lethal attacker,
loot probabilities/drop-rate scaling, inventory limits/trading, durable saves,
late join, roster changes, partition recovery, or cheat-resistant consensus.
`tick_ms` defines the agreed logical duration; no wall-clock scheduler is included.

A shared key authenticates a group; it does not make its members honest. A malicious
member can withhold input or equivocate. This harness assumes a trusted roster and
provides deterministic replay/detection tools, not Byzantine consensus or anti-cheat.
A real transport adapter must stop on observed divergence instead of selecting a
peer's state as an implicit authority.

Tests run two independent worlds with opposite input-arrival order and compare every
tick. They exercise waiting, invalid/repeated inputs, death and respawn, exactly-once
rewards, overflow rollback, snapshots, replay corruption, rule/content fingerprints,
cooldowns, and golden encoding. These are headless tests, not network or renderer QA.
