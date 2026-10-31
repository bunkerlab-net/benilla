# Bounded native equipment durability

This module ports the equipped-item callbacks from CMaNGOS Classic commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, attributed to the CMaNGOS Project
contributors. The original code is GPL-2.0-or-later; this Rust translation is
GPL-3.0-or-later. It carries no game assets, database rows or invented durability.

## Source contract

- [Player.cpp, DurabilityLossAll / DurabilityLoss / DurabilityPointsLoss /
  DurabilityPointLossForEquipSlot](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L4514-L4624)
- [Unit.cpp, qualifying death and selected-hit callbacks](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L1148-L1246)
- [Player.h, equipment slots 0 through 18](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.h#L558-L581)

Only worn equipment is represented. The caller supplies explicit source item ID,
maximum and current durability for each occupied slot. Missing fields, zero item
IDs, duplicate or noncanonical slots, more than 19 items and current values above
their maximum fail. A maximum of zero is explicitly non-durable equipment, never
an inferred missing-field fallback. Source int32 conversion limits maxima to
`i32::MAX`; wider source uint32 values are unavailable instead of wrapping.
Identical item templates in two slots are legal. Required genesis slot plus item ID identifies an
item in this bounded domain; equipment moving, replacing and stacking are absent.

The runtime owns chance and slot RNG consumption and records those draws even if
the selected slot is empty. Once selected, a hit subtracts one point with a zero
floor. An NPC-death callback applies the source percentage to maximum durability,
not the remaining durability, with a minimum loss of one for durable items. Slots
are processed in ascending order. Already broken and non-durable items do not
produce changes or duplicate break transitions.

The caller must establish every source death gate: durability loss is enabled,
there is no responsible player, the victim is not in a battleground, and the
killing spell does not suppress durability loss. This module does not classify
killers or invent defaults for those gates. Player-bot overrides, spirit-healer
loss, general percentages, repairs, negative point losses and inventory loss are
not supplied.

## Exact numeric contract

The source call passes `0.10f` to a double parameter. Its binary32 encoding is
`0x3dcccccd`; promotion to binary64 produces `0x3fb99999a0000000`, exactly
`13421773 / 2^27`. Every admitted maximum is exactly representable as binary64.
The integer product of maximum and 13421773 has at most 55 significant bits, so
binary64 multiplication is implemented by discarding at most two low bits and
rounding the mantissa to 53 bits, nearest with ties to even. Then a right shift by
27 reproduces source uint32 truncation, followed by the minimum-one branch. No
host floating-point arithmetic or new numeric dependency is used.

Rounding before truncation matters. Maximum 738197499 yields loss 73819751;
unrounded rational truncation incorrectly yields 73819750. Maximum 2147483638
yields 214748367 rather than 214748366. These are covered by independent
source-order C++ oracle vectors, along with zero, minimum loss, ordinary maxima
and the signed source-domain boundary.

The test also checks the digest of 100,000 deterministic full-domain samples
against an independent C++ oracle. Reproduce the fixture by compiling this program
with `g++ -std=c++17 -O2` and piping its binary output to `sha256sum`:

```cpp
#include <algorithm>
#include <cstdint>
#include <iostream>
int main() {
    double percent = 0.10f;
    uint64_t seed = 0x65abc139ull;
    for (uint32_t i = 0; i < 100000; ++i) {
        seed = seed * 6364136223846793005ull + 1442695040888963407ull;
        uint32_t max = uint32_t(seed >> 33);
        uint32_t loss = max ? std::max(uint32_t(1), uint32_t(max * percent)) : 0;
        for (uint32_t value : {max, loss})
            for (unsigned byte = 0; byte < 4; ++byte)
                std::cout.put(char(value >> (byte * 8)));
    }
}
```

Expected SHA-256: `97f56c5d2cc10fcf16708a35975aca0030b55ec87a47e7c3c97f2813a82cac7f`.
The C++ oracle requires IEEE-754 binary32/binary64 with ordinary nearest-even
evaluation and no fast-math transformation. The Rust production helper and tests
execute no host floating arithmetic.

## Transaction and restore obligations

`EquipmentState` is live validated state and does not implement Deserialize.
`EquipmentSnapshot` is a versioned public inert representation, with required
fields, unknown-field rejection and bounded list decoding. `from_snapshot`
checks structure. Saved state must instead use `restore` with the authoritative
loadout, which additionally binds exact occupied slots, item IDs and maxima.
Source rows and native-content capability validation remain outside this helper.

Both mutation methods stage the complete next equipment state. If anything
changes, they call the required recomputation closure with before/after snapshots
and ordered durability changes plus explicit Broken transitions. Only a successful
closure replaces the live state. Empty and already-broken slots are no-ops.

The closure must remove each newly broken item's source stat/armor/resistance
effects in slot order, preserve source intermediate health/mana clamps, and update
weapon availability before equipment commits. Old equipment remains live during
the callback, matching source removal before durability becomes zero. The helper
does not store or silently preserve an effective-stat cache. The surrounding world
transaction must stage all state touched by recomputation so callback failure can
roll back resources and stats as well as equipment. On restore, validate/recompute
effective stats against saved broken flags; a structurally valid snapshot alone
does not prove effective-stat consistency.

Focused helper tests establish these callbacks only. They do not establish complete
native combat, multiplayer, inventory/equipment management, or real-client parity.

## Immutable equipped origin (equipment format2, current native)

Source `Item.cpp::CreateItem` creates an item object once; `CloneItem` creates a
new object. `Player.cpp::RemoveItem`, `StoreItem` and `EquipItem` move the existing
object rather than deriving identity from its new position. This phase records
that distinction in the **existing equipped owner and actual projection**, without
admitting any movement yet.

Every equipped snapshot now requires `EquipmentOrigin { genesis_slot, item_id }`,
anchored to the existing validated fresh source initializer's equipment placement.
This is an explicit local origin token, **not** an invented CMaNGOS global low-GUID
allocation or a new item-creation callback. There is no serde default/migration.
Structural validation still requires origin slot/template equal current slot and
item; authoritative restore additionally compares the exact origin, while keeping
all original slot/template/maximum-durability and loadout-membership restrictions.
Duplicate/swapped/forged origins, even with repeated template IDs, cannot authorize
movement. Durability callbacks preserve origin without allocating another item.

Projection now reads the retained origin and uses the **identical** legacy
`benilla-native-equipment-guid-v2` tuple `(world, player, slot, item)` serialization,
first six hash bytes and high-item GUID tag. No existing admitted GUID changes.
Independent legacy-recipe tests cover all19 slot positions and boundary entries;
actual lethal NPC damage/durability loss and checkpoint reopen retain the same
origin and projected item GUID. Rechecksummed malformed and omitted origins reject.

This is a persistence/projection prerequisite, not usable equip/unequip gameplay.
No new command is accepted; current and historical melee/spell/progression recipe
membership checks remain strict. Future movement needs source-ordered membership
and stat/resource transition proofs, stable carried/equipped identity, item-use
requirements and complete callback ownership. Damaged carried items, bind-on-equip
transitions, weapons/bags, loadout substitutions and native admission remain outside
this phase. The current native format rejects earlier formats; no save migration is claimed.

## Shared local equipment handles

The current native format additionally registers each source-initialized worn object in the existing
local item-handle space. Initial backpack creation keeps its existing order and
IDs; then ascending genesis equipment slots receive one checked, contiguous handle
each. The resulting frontier is the next ordinary creation handle. This names
objects already created by the source initializer: it does **not** execute
`CreateItem`/`CloneItem`, claim the upstream global low-GUID allocation order, or
place an item in the backpack. Future grants/loot therefore receive shifted local
IDs deliberately; existing initial carried IDs and current worn GUIDs are unchanged.

The private registry and frontier enter loot-genesis reconstruction v4. Validation
rebuilds both the initial inventory and exact registry from the authoritative
starter input, rather than trusting a self-rehashed map. A registered handle resolves
to the same legacy equipment-origin GUID. No equipment object is published twice.
While membership remains fixed, checkpoint restore rejects any reserved handle in
a carried slot or item receipt, including an otherwise locally valid self-rehashed
receipt. The ordinary pristine/binding/allocator inventory checks are unchanged.

This supplies shared identity, not armor movement. No live command, loadout/stat
recipe relaxation, damaged carried support, ghost state or native admission is
added. Controller save-failure/retry/reopen and headless parser tests validate this
boundary; they do not establish a rendered or user-tested equip/unequip feature.
The current native format has no migration from earlier formats.
