# Source-owned creature generations

This optional native40 component owns ordinary creature initialization per life.
It does not open production admission, import a creature database, authenticate
captured rows, or resolve client assets. The raw arithmetic port and its
exclusions remain in [CREATURE-INIT.md](../benilla-sim/CREATURE-INIT.md).

## Retained input and lifetime

`CreatureGenerationCatalog` privately retains complete raw initialization input
per stable spawn, effective spawn-template identity, and raw DB respawn delay
endpoints. Missing rows do not become empty queries. Source collections and total
catalog bytes are bounded before expensive preparation; every reachable model,
gender and level must validate before the first RNG request. Identical loaded row
identities must agree across spawns. Existing faction, flags, regeneration, spell,
capability and geometry projections must agree with the same retained source.

A generation owner retains the immutable initialized level, complete numeric
result, model selection and creation/death/removal history. Runtime health caps,
regeneration, AI, damage/hit inputs, movement, rewards and client projection use
that life. The authored `NpcProfile` remains a static identity/behavior scaffold;
its copied level or damage is not a selected-source fallback. Historical event
consumers select the retained life instead of the current respawned creature.
Mainhand-only equipment retains complete `creature_equip_template` and relevant
`item_template` metadata, including explicit empty query results. Source selection
uses the template equipment ID or the exact spawn override. Virtual display/info
fields and `hasWeapon` are recomputed for every life. Static loaded disarm halves
the armed creature's final floating-point damage before the ordinary integer roll;
item damage and delay never replace creature damage or attack intervals. The
creation sheath/misc bytes and selected virtual fields reach client projection.
Shared item identities agree with retained starter, inventory and combat metadata
and with every other creature source. Raw ExtraFlags independently bind initial
AI reaction; unowned controls such as no-melee reject even without the optional
combat-authority catalog.

Offhand/ranged equipment, forced dual wield, dynamic disarm auras, loot-wielding,
and later script/event equipment writers remain unsupported. Explicit spawn zero
can retain zero equipment across respawn only within this immutable-source domain.
Client display binding checks consistency; actual visual proof needs local assets.
Combined player-slow ownership
rejects until cleared aura-slot level residue has its own per-write provenance.

## Source order and RNG

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `Creature.cpp` InitEntry/UpdateEntry/SelectLevel: model, conditional gender,
  conditional level. A single weighted model still draws; equal levels do not
- `Creature.cpp` LoadFromDB and `Creature.h` respawn randomization: initial DB
  respawn-delay selection follows initialization and draws even for equal bounds
- `Unit.cpp` JustKilledCreature/SetDeathState and `Creature.cpp` JUST_DIED: body
  loot precedes the death respawn-delay redraw, which precedes death timer creation
- `Creature.cpp` JUST_ALIVED/ResetEntry: initialization is repeated before the AI
  respawn callback. Generation-zero LoadFromDB caps corpse delay to nine tenths
  of the initial delay; later lives restore entry/config corpse delay. A later
  death redraw does not retroactively recalculate that initial cap

All requests use the existing shared counter sampler, retaining rejected words
and exact semantic spans. There is no independent creature seed or reset cursor.
Player LoadFromDB initial-cast genesis remains the counter-zero prefix. The
component's `SyntheticPlayerThenAuthoredSpawns` schedule appends named creatures
in explicit authored order. That is a deterministic synthetic cold-entry contract,
not evidence of universal source grid iteration or a real Human entry order.

## Persistence and transaction boundary

Native format 40, its policy and all native hash domains reject older envelopes.
The RNG mapping itself is unchanged. Checkpoints require explicit source-owner
option fields; absence cannot deserialize into a source-free fallback. Restore
replays retained semantic spans against the world seed and retained source, then
recomputes initialized values. It checks life order, actual corpse-removal markers,
death-delay deadlines, current AI profile/generation/timestamps, initial ordered
prefix, globally disjoint retained spans and current counter bounds. Historical
corpse removal retains its AI snapshot and complete bounded loot-timer call chain;
the deadline is recomputed from that life's source corpse delay, with each call
bound to the loaded loot policy. A tick marker alone is not enough. AI checkpoint 6 also
binds the exact effective profile and explicit owned-generation mode.

The structural generation history bound is 256 lives per spawn; encoded owner
history is bounded to 256 KiB, with an additional aggregate source-replay work cap.
Those work/byte limits can reject before the structural life ceiling. Exhaustion rejects rather than dropping historical ownership.
The frame and replay batch remain staged transactions. Failed later source work
cannot commit an earlier draw, NPC mutation, event, reply or journal suffix.

A self-contained rehashed history is deterministic consistency evidence, not a
signature or independent proof that historical callbacks happened. Exact journal
replay supplies the recorded transition comparison; imported capture authenticity,
actual map scheduling and complete real-world content admission remain separate.
