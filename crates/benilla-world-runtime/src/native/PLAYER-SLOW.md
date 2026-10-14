# Source-bound player decrease-speed holders

This bounded native component is a GPL-3.0-or-later port of CMaNGOS Classic
8ec338a1704e7dcb1c0213eb7ed58f9231ade40f Unit.cpp and SpellAuras.cpp, upstream
GPL-2.0-or-later, copyright CMaNGOS contributors. See the simulation
[numeric contract](../../../benilla-sim/PLAYER-SLOW.md) and
[provenance](../../../benilla-sim/PROVENANCE.md). No game data or database rows ship.

## Admission and effect boundary

`PlayerSlowGenesis::from_sources` requires both a complete `PlayerSlowCatalog`
and a captured `PlayerSlowHostSource`. Effective SpellEntry inputs retain all 173
words after server spell_template overrides, server-only fields, actual linked
cast-time/duration/range records, and explicit complete hook closure. Unknown
never means empty. The audited ID domain includes 1604 without supplying any
stock amount, duration, flags or stacking values; fixture IDs 61004/61005 are
explicitly authored. Unsupported attributes, effects, rank/group conflicts,
periodicity, heartbeat resistance, dispel/proc/script hooks, stacking and random
effect dice reject. The recipe derives NPC-level effect value and the finite
absolute Duration[0], assigning both current and maximum holder duration.

Host inputs bind NPC spawn/entry/level/raw unit flags and control state to the
actual profile. Caster and player spellmod/script/addon sources must be explicitly
complete and absent in the supported branch. Initial player speed/spellmod
holders, mounted/controlled/shapeshift/ghost modifiers and custom ratios are
excluded. Raw max-player-level configuration is required for the source's
absent-caster aura-level field fallback. These supplied closures are inputs,
not proof that a producer inspected authentic content.

There is no public ApplyAura command. The private effect seam takes only a
source spell ID and the current caster identity; it derives amount/duration and
binds profile, world, target, caster generation/level and source digest. At this
milestone only the `synthetic-native-tests` feature can schedule authored effects
before or after the player aura update. That schedule is immutable profile data,
with at most 64 effects per tick; it cannot be supplied as an owner command.
Production admission remains uninhabited. The [separate NPC host](../../NPC-SPELLS.md)
handles bounded invisible instant SpellStart, target/hit checks and event-owned
cleanup. Visible/projectile NPC spells and rear/non-facing melee remain outside
this domain. This is not playable rear daze.

## Holder lifecycle and source order

Active holders have monotone IDs disjoint from initial passive holder IDs,
spell/caster generation and level, raw binding, effect amount/slot, harmful
slots 32–47, application map time, duration/max duration and creation-skip state.
The live state is not deserializable; inert checkpoint/event metadata is checked
against the selected profile and derived values during restoration.

Unit.cpp:4687+ same-caster, zero-stack replacement removes BY_STACK before
creating the replacement. The implementation emits restored speed changes,
clears the old visible slot, adds the new visible slot/duration and reapplies
speed changes, even when final speeds are unchanged. Same-spell/different-source-Unit
stacking is explicitly unsupported; different admitted spells use the checked
empty rank/group/family conflict closure.

Unit.cpp:4086+ updates holders in spell-ID/holder-ID order and only afterward
removes expired holders from the beginning. SpellAuras.cpp:5666+ clears creation
skip on first update and skips elapsed time only on the same map timestamp.
A holder created after player update decrements normally on the next tick.
Death combines passive and active holders in source spell order and runs active
removal at the existing RemoveAuraByDeath action. Modifiers are removed and
speeds restored before the slot clear; no extra death root order is invented.

Caster death does not remove the target's holder. Its captured generation remains
historical after caster respawn. Removal writes the actual persistent source Unit
level across ordinary DEAD/respawn, otherwise the explicit configured fallback
for genuine Unit absence (not currently an admitted permanent-removal lifecycle), even to
an empty slot. That otherwise-unused slot-level byte is durable and reconstructed
on reopen. Restore validates active values, slot uniqueness, source bindings,
creation/elapsed duration, monotone allocation and recomputed rates; hashes do
not establish that an untrusted author witnessed the claimed history.

## Speed, movement and publication

The source handler recomputes RUN, RUN_BACK and SWIM in that order. The software
binary32 kernel chooses the strongest negative modifier, applies the exact source
arithmetic and 0.01f rate floor, and multiplies the source base speed. WALK and
SWIM_BACK remain at their source baseline. Current incoming ground movement flags
select the live budget before flags commit, so WALK/BACKWARD cannot borrow the
previous RUN rate. Geometry receives the runtime-derived speed with the immutable
whole-frame origin/path/time budget. Swimming motion admission remains closed.

The client projects harmful flags 8, spell IDs, caster-level bytes, single-stack
application count and AuraDuration at application/reopen. Callback snapshots
preserve all replacement/expiry/death ordering rather than collapsing to final
state. RUN/RUN_BACK/SWIM forced changes share the actual session counter with root.
Exact GUID/counter/kind/binary32 speed ACKs are bookkeeping only; they cannot move
the player, advance simulation or change aura state. Initial reopening reconstructs
current durations/speeds with fresh session counters.

Every enabled slow-profile tick, including stationary duration progress, uses the
controller's save-before-publication outbox. A failed checkpoint retains committed
effects without replaying input, damage, RNG or speed allocation. Explicit retry
resynchronizes current UI durations because client aura countdowns use wall time.
A crash after saving reconstructs state, not historical notifications.

## Persistence and evidence limits

Experimental native format 18 and the player-slow policy bind this state and the
numeric policy. Prior native formats are rejected without replacing/deleting
current or previous files. Character/profile files and OriginalFixture World 2
are unchanged. Runtime, client, store, geometry and the actual controller source
are exercised by headless authored-fixture tests. Source-extracted numeric oracle
checks do not certify raw content, game assets, live realm behavior, graphics,
general auras, NPC triggered spells or multiplayer scheduling.

The separate [NPC SpellStart host](../../NPC-SPELLS.md) reaches this private effect
seam after source target/hit checks. Its authored triggers do not enable spell1604.
