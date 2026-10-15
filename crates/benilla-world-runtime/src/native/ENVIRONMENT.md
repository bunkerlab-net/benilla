# Cached player environment component

`native::environment` is a standalone, opt-in GPL-3.0-or-later port of the
CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` player environment reducer.
Upstream is GPL-2.0-or-later, copyright CMaNGOS contributors. Source anchors:
`Player.cpp` SetEnvironmentFlags:1105–1140, Update:1483–1489,
SetPosition:6069–6075, UpdateTerainEnvironmentFlags:19113–19175;
`Player.h` EnvironmentFlags; `GridMapDefines.h` liquid bits;
`DBCStructure.h` LiquidTypeEntry and `DBCfmt.h` niii.
No client assets, genuine game rows or world database contents ship.

## Exact boundary

The component runs the environment phase **after an accepted relocation**.
It does not implement SetPosition coordinate/orientation acceptance, movement
interrupts, map relocation, group/trade/zone updates, or the subsequent
CheckAreaExploreAndOutdoor call. An `EnvironmentPose` is an already accepted
source-world pose/collision-height bit pattern; finite-value checks protect this
component's arithmetic and do not certify full SetPosition validity.

The immutable `EnvironmentQuery` binds the existing published TerrainInfo
snapshot (map, content, source epoch, product inventory and policy), the SHA-256
of complete LiquidType WDBC bytes, an exact sequence ticket, source binary32
position, all-liquid mask and collision height. `BoundLiquidResult` retains
status and full uint32 entry/type flags plus unchanged level/depth bits. Public
DTOs are inert observations: consistency checks, not authenticated producer
identity, provenance, a native-content compiler, or gameplay admission.

`LiquidCatalog` and the geometry reader share one bounded WDBC parser. All four
niii columns and the complete-file digest survive; an absent row is distinct
from row zero and a present zero-spell row. No selected-row constructor,
default SpellId, Deserialize catalog, NPC capability or InhabitType feed exists.

## Source behavior

The source starts with zero flags, zero timer and a null last-liquid pointer.
Update only saturating-decrements the timer. Expiration while stationary never
queries or changes cached environment flags. Every accepted post-relocation
call checks the gate, including identical pose and rotation-only calls. At zero
it resets to 100 ms and consumes one exactly bound query; calls during the
cached window consume none. The cache retains its original query separately
from the latest post-relocation pose.

Classic liquid type bits are magma 0x01, ocean 0x02, slime 0x04, water 0x08 and
deep water 0x10. The corresponding environment bits are a different enum.
Only matching liquid kinds update WATER/MAGMA/SLIME/UNDERWATER flags, so a
cross-kind transition intentionally retains unrelated prior bits. Entry zero
bypasses lookup even when row zero exists, clearing a prior spell-bearing row
but retaining a remembered zero-spell row. Missing nonzero IDs are real absence.
A differing old row's spell removal precedes the new HasAura query, including
when both rows share the same spell ID.

SetEnvironmentFlags mutates flags before its callbacks. Its no-op guard tests
whether **any requested bit** already agrees; callback dispatch uses the
requested mask, not a changed-bit mask. The dry combined clear therefore calls
threat, interrupt removal, fatigue, breath and environmental scaling in that
order even when only the LIQUID bit was previously set. Water-breathing interval
is queried only on applying a changed UNDERWATER flag. High-liquid depth uses
binary64 multiply/add/comparison because the source's 0.75 literal is double;
there is no intervening binary32 rounding.

## Effects and atomic failure

The exact reducer represents ordered threat updates, interrupt/spell aura
removals, mirror scaling, HasAura/interval reads and self-triggered spell casts.
**No production callback executor is installed in this slice.** Every such
operation returns its typed `CallbacksUnavailable` intent. A staged transition
is then discarded without changing flags, timer, last liquid, cache, sequence,
history tip or journal. Merely recording a callback never applies its effect.
The private callback interface cannot be implemented by external callers; no
generic clone-based host promises rollback for shared external state.

Callback-free initial dry and above-water zero-spell observations, cached calls
and timer phases do commit. A dry exit after LIQUID was set requires the combined
callback sequence and refuses. This intentionally bounded production domain
cannot import arbitrary wet state or spell-bearing remembered rows through a
checkpoint. A private `cfg(test)` fully owned synthetic host exercises the wider
reducer and staged host rollback; it is not the game's effect implementation.

There is no Engine/NativeWorld/Profile default change, live world-feed producer,
NPC accessibility admission, spell/threat authority, damage/drowning/fatigue
implementation, or playable-world claim. Full callback implementations are
follow-on engineering work, not implicitly denied permission. Concrete seams:

- Threat updates need the actual hostile-reference set and source creature
  water/land accessibility, not guessed CanSwim/CanWalk or InhabitType
- Aura interrupts/removal/HasAura need the real ordered holder lifecycle and
  complete interrupt/spell metadata; removal must affect later same-spell queries
- Mirror scaling needs persistent MirrorTimer state and full breathing config,
  multiplier/aura lifecycle and timer ownership
- Triggered self-casts need genuine spell catalog and the source spell/event
  lifecycle; an emitted cast intent cannot substitute for execution

## Independent persistence and evidence

Component checkpoint/journal formats have their own version/policy and do not
change native world saves. Checkpoints retain the exact timer and cached query,
last observed pose, last-liquid ID resolved against the selected complete
catalog, selected world/player/snapshot identity, sequence and history tip.
Restore checks bounded canonical bytes, compatible identity, valid committed
state, state digest and checksum. A trusted filesystem or separately pinned
digest is still needed: an attacker who can rewrite and re-hash an entire
checkpoint can invent self-consistent state. These are integrity checks, not
history authentication or proof of source data provenance.

Create a checkpoint, durably retain it through the caller's storage, then call
`discard_records` with that exact checkpoint. The component provides no storage
writer and cannot certify durable I/O. Replay checks the chain and re-executes
all phases through the same fail-closed production boundary, comparing complete
records. The whole replay batch is staged, so a changed tail cannot partially
commit. Local limits are 2,048 records and 4 MiB canonical journal bytes.

The [source oracle](../../tools/environment_state_README.md) extracts original
methods and timer/gate fragments from hash-guarded pinned files. Its 1,324 rows
cover cached gates, 99/100 ms, same pose, stationary expiry, selective retention,
row zero/missing/full-uint32 IDs, crossing liquids and spells, combined clears,
callback order and binary32 neighbors around the binary64 depth threshold.
Twenty lookup observations come from the original WDBC reader over complete
**authored** stores. Harness geometry facts and HasAura/interval answers are
explicitly authored dependencies; callback traces prove source invocation
order, not actual aura/threat/mirror/spell effects. The private owned-host test
separately exercises same-spell removal feedback and transactional failure.
Raw source-only cases include arbitrary uint8 flags and uint32 timers that
public production checkpoint admission intentionally cannot restore.
