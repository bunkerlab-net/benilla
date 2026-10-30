# Source-owned creature generations

This optional native81 component owns ordinary creature initialization per life.
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

## Retained ordinary addon rows

Initialization retains explicit complete `creature_addon` and
`creature_template_addon` query results, including empty results, alongside the
stable spawn ID and template entry. Each query admits at most one exact-owner
row. Rows retain mount, stand state, sheath state, emote, movement flags and aura
IDs. Duplicate or mismatched rows, unavailable queries and unsupported fields
reject before sampling;
unsupported fields also reject in a template row hidden by a spawn override.
Shared template identities must agree on both query presence and full row content
across spawns.

Pinned `Creature::LoadCreatureAddon` selects the whole spawn row when present,
otherwise the template row; a zero sheath in a spawn row does not fall back to a
nonzero template sheath. Only sheath states 0..2 are supported. The source write
replaces the low sheath byte while preserving initialized `0x1000`; without a
selected row, initialized misc bytes are unchanged. Captured movement flags are
retained but the pinned loader does not apply them. Mounts, nonzero stand/emote,
addon auras and script writers remain unsupported.

The raw rows are bound by the existing initializer and generation source identity,
then recomputed for every selected life and historical restore. Selection adds no
RNG draws. This is bounded source consistency, not proof of source capture
completeness, real client rendering or production admission.

## Capability, Home and visitor composition

Each selected capability source, Home audit and ordinary visitor owner explicitly
classifies its addon closure as `Absent` or `GenerationSource { source_identity }`.
`Absent` requires both retained addon tables to be empty whenever a generation
owner exists; all-zero and shadowed rows still count as present. A generation
reference must resolve to the exact stable spawn and entry in the profile's
validated immutable generation catalog and match its full source identity. A
digest without that selected owner supplies no closure. Changing any retained
row, including an overridden template row, changes the bound identity.

The three consumers share the generation owner's validated sheath-only domain;
they do not copy addon rows or accept a caller assertion that a row is harmless.
Pinned Home completion reloads the same immutable selection, so its supported
sheath write is idempotent. Captured `move_flags` are not applied by the pinned
loader and cannot alter capability movement flags. Scripted Home events, reset
overrides, temporary faction/summon callbacks, visitor addon override writers,
auras, mounts and entry substitutions remain excluded. Profile validation checks
the composed closure before execution and again on restore.

Addon composition alone supplies no habitat or geometry proof. The separately
selected [dry habitat-3 owner](CREATURE-DRY-HABITAT.md) retains `CanSwim=true` and
requires source-ordered initialization and movement observations. Retained sheath
state cannot turn a water-capable source into habitat 1 or certify dry geometry.
Actual entry-6 DBC, movement, EventAI and content-admission dependencies remain open.

## Source order and RNG

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `Creature.cpp` InitEntry/UpdateEntry/SelectLevel: model, conditional gender,
  selected-model initialization liquid query for the dry habitat-3 owner, then
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

Native format 81, policy `benilla-native-creature-string-identities-v81`, and v81
profile/state/record/checkpoint hash domains reject earlier envelopes without migration.
Melee and callback hash domains remain separate.
Generation source policy v4 and capability source policy v3 bind the explicit
initialization closure and required per-life receipt. Ordinary visitor source
policy v3 and Home retain their explicit addon closures. The RNG v2
mapping, original World v2 and character saves are unchanged. Checkpoints require
explicit source-owner option fields; absence cannot deserialize into a source-free
fallback. Restore
replays retained semantic spans against the world seed and retained source, then
recomputes initialized values. It checks life order, actual corpse-removal markers,
death-delay deadlines, current AI profile/generation/timestamps, initial ordered
prefix, globally disjoint retained spans and current counter bounds. Historical
corpse removal retains its AI snapshot and complete bounded loot-timer call chain;
the deadline is recomputed from that life's source corpse delay, with each call
bound to the loaded loot policy. A tick marker alone is not enough. AI checkpoint 7 also
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

## Cold template string identity

The source generation policy v5 and initializer policy v4 retain required raw
`StringID1/2` and the complete ordered `string_id` catalog. The host requires one
identical retained cold catalog across its spawn sources; contradictory catalogs
fail before sealing. This coherence restriction does not claim independently
captured SQL row orders are interchangeable. Raw names, duplicates and row order
remain source identity even if loaded template IDs are equal. Aggregate source
row/byte limits apply before retained-source clones.

The initialized result exposes only the effective template pair. The genuine
fresh generation-zero constructor object begins with an empty set and applies
nonzero IDs, preserving deduplication and ascending iteration. The same object
remains in its constructor cell/GUID/DBGUID stores. This does not rebuild later
live sets, implement string-map registration, or bypass event/inWorld ownership.
Missing catalogs do not default to empty; authored fixture zeros are explicit
synthetic inputs. Old initializer/generation policies and native80 checkpoints
are rejected rather than migrated.
