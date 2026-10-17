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
Identical item templates in two slots are legal. Slot plus item ID identifies an
item in this bounded domain; moving, replacing, stacking and inventory are absent.

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
